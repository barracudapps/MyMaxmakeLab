use crate::realtime;
use crate::safety::{Anomaly, JobGuard, SafetyMonitor};
use crate::status::{MachineState, MachineStatus, parse_status};
use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// GRBL has a 128-byte serial receive buffer; one byte is kept free.
const RX_BUFFER: usize = 127;
const STATUS_PERIOD: Duration = Duration::from_millis(200);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);
const STATUS_TIMEOUT: Duration = Duration::from_millis(1500);

/// How lines are paced to the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Protocol {
    /// GRBL and grblHAL: character counting on the receive buffer.
    #[default]
    Grbl,
    /// Smoothieware, Makera Carvera and other GRBL-like firmwares: each
    /// line waits for its `ok`, the safe choice when the buffer size
    /// is unknown.
    SendAndWait,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    Serial {
        port: String,
        baud: u32,
    },
    /// Wi-Fi bridges (ESP32 based controllers, HiMill Wi-Fi) expose the
    /// raw GRBL stream on a TCP port.
    Tcp {
        host: String,
        port: u16,
    },
}

impl std::fmt::Display for Connection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serial { port, baud } => write!(formatter, "{port} @ {baud}"),
            Self::Tcp { host, port } => write!(formatter, "{host}:{port}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// One line through the planner buffer (G-code or `$` command).
    Line(String),
    Realtime(u8),
    StartJob {
        lines: Vec<String>,
        guard: Option<JobGuard>,
    },
    SetProtocol(Protocol),
    /// Feed hold and immediate reset. The position may be lost (alarm 3
    /// when moving) but everything stops at once. Software only: a
    /// physical emergency stop that cuts power remains mandatory.
    EmergencyStop,
    Pause,
    Resume,
    /// Feed hold, wait for the head to stop, then reset: the position is
    /// kept, unlike a reset while moving which raises alarm 3.
    Stop,
    Disconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobOutcome {
    Completed,
    Stopped,
    /// The controller rejected a line; the job was stopped.
    Error {
        line: usize,
        code: u32,
    },
    Alarm(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Connected {
        banner: String,
    },
    Disconnected {
        reason: Option<String>,
    },
    Status(MachineStatus),
    Sent(String),
    Received(String),
    /// The controller answered ok to a line sent outside a job; for
    /// `$H` this is the end of homing.
    Acknowledged(String),
    Error {
        code: u32,
        line: String,
    },
    Alarm(u32),
    Setting {
        key: u32,
        value: String,
    },
    JobProgress {
        acknowledged: usize,
        total: usize,
    },
    JobFinished {
        outcome: JobOutcome,
        elapsed: Duration,
    },
    /// `paused` tells whether the job was put on feed hold for it.
    Anomaly {
        anomaly: Anomaly,
        paused: bool,
    },
    /// A homing cycle ended; a duration much longer than usual hints at
    /// mechanical resistance (dirty rail, loose belt, binding axis).
    HomingFinished {
        duration: Duration,
        success: bool,
    },
    /// The job reached its `index`-th program pause (`M0`/`M1`) and
    /// waits for cycle start: machine button or `Resume`.
    ProgramPaused {
        index: usize,
    },
}

pub fn list_serial_ports() -> Vec<(String, String)> {
    serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter(|port| {
            !port.port_name.contains("Bluetooth") && !port.port_name.starts_with("/dev/tty.")
        })
        .map(|port| {
            let label = match &port.port_type {
                serialport::SerialPortType::UsbPort(usb) => format!(
                    "{} ({})",
                    port.port_name,
                    usb.product
                        .clone()
                        .or(usb.manufacturer.clone())
                        .unwrap_or_else(|| "USB".into())
                ),
                _ => port.port_name.clone(),
            };
            (port.port_name, label)
        })
        .collect()
}

pub trait Transport: Read + Write + Send {}
impl<T: Read + Write + Send> Transport for T {}

fn open(connection: &Connection) -> std::io::Result<Box<dyn Transport>> {
    match connection {
        Connection::Serial { port, baud } => {
            let port = serialport::new(port, *baud)
                .timeout(Duration::from_millis(10))
                .open()
                .map_err(std::io::Error::other)?;
            Ok(Box::new(port))
        }
        Connection::Tcp { host, port } => {
            let address = (host.as_str(), *port)
                .to_socket_addrs()?
                .next()
                .ok_or_else(|| std::io::Error::new(ErrorKind::NotFound, "unknown host"))?;
            let stream = TcpStream::connect_timeout(&address, Duration::from_secs(4))?;
            stream.set_read_timeout(Some(Duration::from_millis(10)))?;
            stream.set_nodelay(true)?;
            Ok(Box::new(stream))
        }
    }
}

/// Handle owned by the user interface.
pub struct Controller {
    commands: Sender<Command>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
    pub protocol: Protocol,
}

impl Controller {
    pub fn connect(connection: Connection) -> Self {
        Self::connect_with(connection, Protocol::Grbl)
    }

    #[cfg(test)]
    fn connect_with_transport(transport: Box<dyn Transport>, protocol: Protocol) -> Self {
        let mut controller = Self::spawn(move || Ok(transport));
        controller.send(Command::SetProtocol(protocol));
        controller.protocol = protocol;
        controller
    }

    pub fn connect_with(connection: Connection, protocol: Protocol) -> Self {
        let mut controller = Self::spawn(move || open(&connection));
        controller.send(Command::SetProtocol(protocol));
        controller.protocol = protocol;
        controller
    }

    /// Runs the protocol over any transport; used by tests with a
    /// simulated controller.
    pub fn spawn<F>(opener: F) -> Self
    where
        F: FnOnce() -> std::io::Result<Box<dyn Transport>> + Send + 'static,
    {
        let (command_sender, command_receiver) = channel();
        let (event_sender, event_receiver) = channel();
        let thread = std::thread::Builder::new()
            .name("grbl".into())
            .spawn(move || match opener() {
                Ok(transport) => {
                    let reason =
                        Worker::new(transport, command_receiver, event_sender.clone()).run();
                    let _ = event_sender.send(Event::Disconnected { reason });
                }
                Err(error) => {
                    let _ = event_sender.send(Event::Disconnected {
                        reason: Some(error.to_string()),
                    });
                }
            })
            .expect("cannot start the controller thread");
        Self {
            commands: command_sender,
            events: event_receiver,
            thread: Some(thread),
            protocol: Protocol::Grbl,
        }
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub fn events(&self) -> impl Iterator<Item = Event> + '_ {
        self.events.try_iter()
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Disconnect);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    User,
    Job(usize),
}

struct Job {
    lines: Vec<String>,
    next: usize,
    acknowledged: usize,
    started: Instant,
    last_progress: Instant,
    pauses: usize,
}

struct Worker {
    transport: Box<dyn Transport>,
    commands: Receiver<Command>,
    events: Sender<Event>,
    queue: VecDeque<(String, Origin)>,
    in_flight: VecDeque<(usize, Origin, String)>,
    buffered: usize,
    job: Option<Job>,
    stopping: Option<Instant>,
    status: MachineStatus,
    incoming: Vec<u8>,
    last_poll: Instant,
    last_status: Instant,
    monitor: SafetyMonitor,
    homing_since: Option<Instant>,
    protocol: Protocol,
}

impl Worker {
    fn new(
        transport: Box<dyn Transport>,
        commands: Receiver<Command>,
        events: Sender<Event>,
    ) -> Self {
        Self {
            transport,
            commands,
            events,
            queue: VecDeque::new(),
            in_flight: VecDeque::new(),
            buffered: 0,
            job: None,
            stopping: None,
            status: MachineStatus::default(),
            incoming: Vec::with_capacity(256),
            last_poll: Instant::now() - STATUS_PERIOD,
            last_status: Instant::now(),
            monitor: SafetyMonitor::default(),
            homing_since: None,
            protocol: Protocol::Grbl,
        }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    fn run(mut self) -> Option<String> {
        // Wakes controllers that do not reset on connection.
        if let Err(error) = self.transport.write_all(b"\r\n\r\n") {
            return Some(error.to_string());
        }
        loop {
            loop {
                match self.commands.try_recv() {
                    Ok(Command::Disconnect) | Err(TryRecvError::Disconnected) => return None,
                    Ok(command) => {
                        if let Err(error) = self.handle(command) {
                            return Some(error.to_string());
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            if let Err(error) = self.tick() {
                return Some(error.to_string());
            }
        }
    }

    fn realtime(&mut self, byte: u8) -> std::io::Result<()> {
        self.transport.write_all(&[byte])?;
        self.transport.flush()
    }

    fn handle(&mut self, command: Command) -> std::io::Result<()> {
        match command {
            Command::Line(line) => {
                let line = line.trim().to_owned();
                if !line.is_empty() && self.job.is_none() {
                    self.queue.push_back((line, Origin::User));
                }
            }
            Command::Realtime(byte) => {
                if byte == realtime::SOFT_RESET {
                    self.clear_buffers();
                }
                self.realtime(byte)?;
            }
            Command::EmergencyStop => {
                self.realtime(realtime::FEED_HOLD)?;
                self.realtime(realtime::SOFT_RESET)?;
                self.stopping = None;
                self.clear_buffers();
                self.finish_job(JobOutcome::Stopped);
            }
            Command::StartJob { lines, guard } => {
                if self.job.is_none() {
                    self.monitor
                        .start_job(guard.unwrap_or_default(), Some(&self.status));
                    let lines: Vec<String> = lines
                        .into_iter()
                        .map(|line| strip_comment(&line))
                        .filter(|line| !line.is_empty())
                        .collect();
                    let now = Instant::now();
                    self.job = Some(Job {
                        lines,
                        next: 0,
                        acknowledged: 0,
                        started: now,
                        last_progress: now,
                        pauses: 0,
                    });
                }
            }
            Command::Pause => self.realtime(realtime::FEED_HOLD)?,
            Command::Resume => self.realtime(realtime::CYCLE_START)?,
            Command::Stop => {
                self.queue.clear();
                if let Some(job) = &mut self.job {
                    job.next = job.lines.len();
                }
                self.realtime(realtime::FEED_HOLD)?;
                self.stopping = Some(Instant::now());
            }
            Command::SetProtocol(protocol) => self.protocol = protocol,
            Command::Disconnect => {}
        }
        Ok(())
    }

    fn clear_buffers(&mut self) {
        self.queue.clear();
        self.in_flight.clear();
        self.buffered = 0;
    }

    fn finish_job(&mut self, outcome: JobOutcome) {
        self.monitor.end_job();
        if let Some(job) = self.job.take() {
            self.emit(Event::JobFinished {
                outcome,
                elapsed: job.started.elapsed(),
            });
        }
    }

    fn tick(&mut self) -> std::io::Result<()> {
        self.fill_buffer()?;
        if self.last_poll.elapsed() >= STATUS_PERIOD {
            self.realtime(realtime::STATUS)?;
            self.last_poll = Instant::now();
        }
        if self.job.is_some() && self.last_status.elapsed() > STATUS_TIMEOUT {
            let seconds = self.last_status.elapsed().as_secs_f64();
            if let Some(anomaly) = self.monitor.report(Anomaly::StatusTimeout { seconds }) {
                self.emit(Event::Anomaly {
                    anomaly,
                    paused: false,
                });
            }
        }
        if let Some(since) = self.stopping {
            let stopped = matches!(
                self.status.state,
                MachineState::HoldComplete | MachineState::Idle
            );
            if stopped || since.elapsed() > STOP_TIMEOUT {
                self.stopping = None;
                self.clear_buffers();
                self.realtime(realtime::SOFT_RESET)?;
                self.finish_job(JobOutcome::Stopped);
            }
        }
        let mut chunk = [0u8; 512];
        match self.transport.read(&mut chunk) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "connection closed",
                ));
            }
            Ok(count) => {
                self.incoming.extend_from_slice(&chunk[..count]);
                while let Some(end) = self.incoming.iter().position(|&byte| byte == b'\n') {
                    let raw: Vec<u8> = self.incoming.drain(..=end).collect();
                    let line = String::from_utf8_lossy(&raw).trim().to_owned();
                    if !line.is_empty() {
                        self.on_line(&line)?;
                    }
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
        Ok(())
    }

    fn track_homing(&mut self, status: &MachineStatus) {
        match (status.state == MachineState::Home, self.homing_since) {
            (true, None) => self.homing_since = Some(Instant::now()),
            (false, Some(since)) => {
                self.homing_since = None;
                let success = status.state == MachineState::Idle;
                self.emit(Event::HomingFinished {
                    duration: since.elapsed(),
                    success,
                });
            }
            _ => {}
        }
    }

    fn fill_buffer(&mut self) -> std::io::Result<()> {
        if self.stopping.is_none()
            && self.queue.is_empty()
            && let Some(job) = &mut self.job
            && job.next < job.lines.len()
        {
            // Pull job lines lazily so that the queue stays short.
            while job.next < job.lines.len() && self.queue.len() < 32 {
                self.queue
                    .push_back((job.lines[job.next].clone(), Origin::Job(job.next)));
                job.next += 1;
            }
        }
        while let Some((line, _)) = self.queue.front() {
            let size = line.len() + 1;
            let full = match self.protocol {
                Protocol::Grbl => self.buffered + size > RX_BUFFER,
                Protocol::SendAndWait => true,
            };
            if full && !self.in_flight.is_empty() {
                break;
            }
            let (line, origin) = self.queue.pop_front().unwrap();
            self.transport.write_all(line.as_bytes())?;
            self.transport.write_all(b"\n")?;
            self.buffered += size;
            if origin == Origin::User {
                self.emit(Event::Sent(line.clone()));
            }
            self.in_flight.push_back((size, origin, line));
        }
        self.transport.flush()
    }

    fn on_line(&mut self, line: &str) -> std::io::Result<()> {
        if line.starts_with('<') {
            if let Some(status) = parse_status(line, &self.status) {
                self.last_status = Instant::now();
                self.track_homing(&status);
                let pending = !self.in_flight.is_empty() || !self.queue.is_empty();
                let changing_tool = self
                    .in_flight
                    .iter()
                    .any(|(_, _, line)| crate::safety::is_tool_change(line));
                self.monitor.set_tool_change(changing_tool);
                for anomaly in self.monitor.observe(&status, pending) {
                    let Some(anomaly) = self.monitor.report(anomaly) else {
                        continue;
                    };
                    let pause = anomaly.is_critical()
                        && self.job.is_some()
                        && self.monitor.guard().is_some_and(|guard| guard.auto_pause);
                    if pause {
                        self.realtime(realtime::FEED_HOLD)?;
                    }
                    self.emit(Event::Anomaly {
                        anomaly,
                        paused: pause,
                    });
                }
                self.status = status.clone();
                self.emit(Event::Status(status));
                let drained = self.job.as_ref().is_some_and(|job| {
                    job.acknowledged == job.lines.len() && job.next == job.lines.len()
                });
                if drained && self.status.state == MachineState::Idle && self.stopping.is_none() {
                    self.finish_job(JobOutcome::Completed);
                }
            }
            return Ok(());
        }
        if line == "ok" || line.starts_with("error:") {
            let (size, origin, sent) =
                self.in_flight
                    .pop_front()
                    .unwrap_or((0, Origin::User, String::new()));
            self.buffered = self.buffered.saturating_sub(size);
            if let Some(code) = line.strip_prefix("error:") {
                let code = code.trim().parse().unwrap_or(0);
                self.emit(Event::Error { code, line: sent });
                if let Origin::Job(index) = origin {
                    self.queue.clear();
                    self.realtime(realtime::FEED_HOLD)?;
                    self.stopping = Some(Instant::now());
                    let elapsed = self
                        .job
                        .as_ref()
                        .map(|job| job.started.elapsed())
                        .unwrap_or_default();
                    self.job = None;
                    self.emit(Event::JobFinished {
                        outcome: JobOutcome::Error {
                            line: index + 1,
                            code,
                        },
                        elapsed,
                    });
                }
                return Ok(());
            }
            match origin {
                Origin::Job(_) => {
                    if is_program_pause(&sent)
                        && let Some(job) = &mut self.job
                    {
                        let index = job.pauses;
                        job.pauses += 1;
                        self.emit(Event::ProgramPaused { index });
                    }
                    if let Some(job) = &mut self.job {
                        job.acknowledged += 1;
                        let done = job.acknowledged == job.lines.len();
                        if done || job.last_progress.elapsed() > Duration::from_millis(100) {
                            job.last_progress = Instant::now();
                            let (acknowledged, total) = (job.acknowledged, job.lines.len());
                            self.emit(Event::JobProgress {
                                acknowledged,
                                total,
                            });
                        }
                    }
                }
                Origin::User => {
                    self.emit(Event::Received(line.to_owned()));
                    self.emit(Event::Acknowledged(sent));
                }
            }
            return Ok(());
        }
        if let Some(code) = line.strip_prefix("ALARM:") {
            let code = code.trim().parse().unwrap_or(0);
            self.clear_buffers();
            self.stopping = None;
            self.emit(Event::Alarm(code));
            self.finish_job(JobOutcome::Alarm(code));
            return Ok(());
        }
        if line.starts_with("Grbl") || line.starts_with("GrblHAL") {
            self.clear_buffers();
            self.emit(Event::Connected {
                banner: line.to_owned(),
            });
            if self.stopping.is_none() {
                self.finish_job(JobOutcome::Stopped);
            }
            self.queue.push_back(("$$".into(), Origin::User));
            return Ok(());
        }
        if let Some(setting) = line.strip_prefix('$')
            && let Some((key, value)) = setting.split_once('=')
            && let Ok(key) = key.parse()
        {
            self.emit(Event::Setting {
                key,
                value: value.to_owned(),
            });
        }
        self.emit(Event::Received(line.to_owned()));
        Ok(())
    }
}

fn is_program_pause(line: &str) -> bool {
    let code = line.trim().to_ascii_uppercase();
    code == "M0" || code == "M1" || code == "M00" || code == "M01"
}

fn strip_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_paren = false;
    for character in line.chars() {
        match character {
            ';' if !in_paren => break,
            '(' => in_paren = true,
            ')' => in_paren = false,
            c if !in_paren => out.push(c),
            _ => {}
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Minimal GRBL: answers `ok` to each line, `error:20` to `BAD`,
    /// and reports Idle on status requests.
    #[derive(Clone, Default)]
    struct FakeGrbl {
        received: Arc<Mutex<Vec<String>>>,
        outgoing: Arc<Mutex<VecDeque<u8>>>,
        partial: Arc<Mutex<String>>,
    }

    impl Read for FakeGrbl {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let mut outgoing = self.outgoing.lock().unwrap();
            if outgoing.is_empty() {
                drop(outgoing);
                std::thread::sleep(Duration::from_millis(1));
                return Err(ErrorKind::TimedOut.into());
            }
            let count = buffer.len().min(outgoing.len());
            for slot in buffer.iter_mut().take(count) {
                *slot = outgoing.pop_front().unwrap();
            }
            Ok(count)
        }
    }

    impl Write for FakeGrbl {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            let mut outgoing = self.outgoing.lock().unwrap();
            let mut partial = self.partial.lock().unwrap();
            for &byte in data {
                match byte {
                    b'?' => outgoing.extend(b"<Idle|MPos:0.000,0.000,0.000|FS:0,0>\r\n"),
                    b'\n' => {
                        let line = std::mem::take(&mut *partial);
                        let line = line.trim().to_owned();
                        if line.is_empty() {
                            continue;
                        }
                        let reply: &[u8] = if line == "BAD" {
                            b"error:20\r\n"
                        } else {
                            b"ok\r\n"
                        };
                        outgoing.extend(reply);
                        self.received.lock().unwrap().push(line);
                    }
                    byte if byte < 0x80 && byte != b'\r' => partial.push(byte as char),
                    _ => {}
                }
            }
            Ok(data.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn wait_for(controller: &Controller, predicate: impl Fn(&Event) -> bool) -> Vec<Event> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            for event in controller.events() {
                let matched = predicate(&event);
                seen.push(event);
                if matched {
                    return seen;
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("event not received, saw {seen:?}");
    }

    #[test]
    fn job_lines_are_streamed_in_order_without_comments() {
        let grbl = FakeGrbl::default();
        let received = grbl.received.clone();
        let controller = Controller::spawn(move || Ok(Box::new(grbl) as Box<dyn Transport>));
        let lines: Vec<String> = (0..500)
            .map(|index| format!("G1 X{index} F1000 ; move"))
            .collect();
        controller.send(Command::StartJob { lines, guard: None });
        let events = wait_for(&controller, |event| {
            matches!(event, Event::JobFinished { .. })
        });
        assert!(matches!(
            events.last(),
            Some(Event::JobFinished {
                outcome: JobOutcome::Completed,
                ..
            })
        ));
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 500);
        assert_eq!(received[499], "G1 X499 F1000");
    }

    #[test]
    fn rejected_line_stops_the_job_and_reports_its_number() {
        let grbl = FakeGrbl::default();
        let controller = Controller::spawn(move || Ok(Box::new(grbl) as Box<dyn Transport>));
        controller.send(Command::StartJob {
            lines: vec!["G0 X1".into(), "BAD".into(), "G0 X2".into()],
            guard: None,
        });
        let events = wait_for(&controller, |event| {
            matches!(event, Event::JobFinished { .. })
        });
        assert!(matches!(
            events.last(),
            Some(Event::JobFinished {
                outcome: JobOutcome::Error { line: 2, code: 20 },
                ..
            })
        ));
    }

    #[test]
    fn send_and_wait_keeps_a_single_line_in_flight() {
        let grbl = FakeGrbl::default();
        let received = grbl.received.clone();
        let controller = Controller::connect_with_transport(Box::new(grbl), Protocol::SendAndWait);
        controller.send(Command::StartJob {
            lines: (0..50).map(|index| format!("G1 X{index}")).collect(),
            guard: None,
        });
        let events = wait_for(&controller, |event| {
            matches!(event, Event::JobFinished { .. })
        });
        assert!(matches!(
            events.last(),
            Some(Event::JobFinished {
                outcome: JobOutcome::Completed,
                ..
            })
        ));
        assert_eq!(received.lock().unwrap().len(), 50);
    }

    #[test]
    fn comments_are_stripped() {
        assert_eq!(strip_comment("G1 X1 (feed) Y2 ; tail"), "G1 X1  Y2");
    }
}
