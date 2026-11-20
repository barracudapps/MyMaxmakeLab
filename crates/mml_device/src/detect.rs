//! Finds machines without the user typing ports, speeds or addresses:
//! USB serial ports are asked who they are, the local network is
//! scanned on the ports machine controllers listen on, and Carvera
//! beacons and mDNS announcements are collected.
//!
//! Only harmless queries are sent (status, build info, version), but
//! opening a USB serial port resets most Arduino-based boards: the scan
//! runs only when the user asks for it.

use crate::Protocol;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Firmware {
    Grbl,
    GrblHal,
    FluidNc,
    Smoothieware,
    Carvera,
    Marlin,
    Unknown,
}

impl Firmware {
    pub fn name(self) -> &'static str {
        match self {
            Self::Grbl => "GRBL",
            Self::GrblHal => "grblHAL",
            Self::FluidNc => "FluidNC",
            Self::Smoothieware => "Smoothieware",
            Self::Carvera => "Makera Carvera",
            Self::Marlin => "Marlin",
            Self::Unknown => "?",
        }
    }

    /// How to stream to it.
    pub fn protocol(self) -> Protocol {
        match self {
            Self::Smoothieware | Self::Carvera | Self::Marlin => Protocol::SendAndWait,
            _ => Protocol::Grbl,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Serial { port: String, baud: u32 },
    Network { host: String, port: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub link: Link,
    pub firmware: Firmware,
    /// Banner or version line, for the user.
    pub details: String,
}

/// Recognises the firmware from what it answered.
pub fn classify(reply: &str) -> Option<Firmware> {
    let lower = reply.to_lowercase();
    if lower.contains("carvera") {
        Some(Firmware::Carvera)
    } else if lower.contains("fluidnc") {
        Some(Firmware::FluidNc)
    } else if lower.contains("grblhal")
        || lower.contains("[ver:1.1f.2")
        || lower.contains("grbl 1.1f [")
    {
        Some(Firmware::GrblHal)
    } else if lower.contains("grbl") || reply.contains("<Idle") || reply.contains("<Alarm") {
        Some(Firmware::Grbl)
    } else if lower.contains("smoothie") || lower.contains("build version") {
        Some(Firmware::Smoothieware)
    } else if lower.contains("firmware_name:marlin") || lower.contains("marlin") {
        Some(Firmware::Marlin)
    } else if reply.lines().any(|line| line.trim() == "ok") {
        Some(Firmware::Unknown)
    } else {
        None
    }
}

/// First informative line of a reply.
fn details(reply: &str) -> String {
    reply
        .lines()
        .map(str::trim)
        .find(|line| line.len() > 3 && *line != "ok")
        .unwrap_or_default()
        .chars()
        .take(80)
        .collect()
}

/// Sends the queries and gathers the answer for `wait`.
fn interrogate(stream: &mut dyn ReadWrite, wait: Duration) -> String {
    let mut reply = Vec::new();
    let mut buffer = [0u8; 512];
    let deadline = Instant::now() + wait;
    let mut asked = false;
    while Instant::now() < deadline {
        // Let the boot banner arrive first (boards reset on open).
        if !asked && deadline - Instant::now() < wait * 2 / 3 {
            let _ = stream.write_all(b"\r\n$I\r\n?version\nM115\n");
            let _ = stream.flush();
            asked = true;
        }
        match stream.read(&mut buffer) {
            Ok(0) => std::thread::sleep(Duration::from_millis(20)),
            Ok(count) => reply.extend_from_slice(&buffer[..count]),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    String::from_utf8_lossy(&reply).into_owned()
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

fn probe_serial(port: &str) -> Option<Found> {
    for baud in [115_200, 250_000, 57_600, 9_600] {
        let Ok(mut stream) = serialport::new(port, baud)
            .timeout(Duration::from_millis(30))
            .open()
        else {
            return None;
        };
        let reply = interrogate(&mut stream, Duration::from_millis(2_200));
        if let Some(firmware) = classify(&reply) {
            return Some(Found {
                link: Link::Serial {
                    port: port.to_owned(),
                    baud,
                },
                firmware,
                details: details(&reply),
            });
        }
    }
    None
}

fn probe_tcp(address: SocketAddr) -> Option<Found> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(250)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(30)))
        .ok()?;
    let reply = interrogate(&mut stream, Duration::from_millis(1_200));
    // A listening port that says nothing is still worth showing.
    let firmware = classify(&reply).unwrap_or(match address.port() {
        2222 => Firmware::Carvera,
        _ => Firmware::Unknown,
    });
    Some(Found {
        link: Link::Network {
            host: address.ip().to_string(),
            port: address.port(),
        },
        firmware,
        details: details(&reply),
    })
}

/// Address of this computer on the local network: the route a datagram
/// would take, found without sending anything.
fn local_address() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("10.255.255.255:9").ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(address) if !address.ip().is_loopback() => Some(*address.ip()),
        _ => None,
    }
}

/// Machine controller ports: telnet (GRBL Wi-Fi boards, ESP3D,
/// FluidNC, HiMill) and the Carvera.
pub const NETWORK_PORTS: [u16; 2] = [23, 2222];

fn scan_network() -> Vec<Found> {
    let Some(local) = local_address() else {
        return Vec::new();
    };
    let [a, b, c, own] = local.octets();
    let (sender, receiver) = channel();
    let mut workers = Vec::new();
    // 254 hosts in parallel batches; a closed port answers at once and
    // a missing host times out after 250 ms.
    for chunk in (1u8..=254)
        .filter(|host| *host != own)
        .collect::<Vec<_>>()
        .chunks(32)
    {
        let chunk = chunk.to_vec();
        let sender = sender.clone();
        workers.push(std::thread::spawn(move || {
            for host in chunk {
                for port in NETWORK_PORTS {
                    let address = SocketAddr::from((Ipv4Addr::new(a, b, c, host), port));
                    if let Some(found) = probe_tcp(address) {
                        let _ = sender.send(found);
                    }
                }
            }
        }));
    }
    drop(sender);
    for worker in workers {
        let _ = worker.join();
    }
    receiver.into_iter().collect()
}

/// mDNS query for controllers that announce themselves (FluidNC and
/// ESP3D boards advertise HTTP and telnet services).
fn mdns_hosts(wait: Duration) -> Vec<Ipv4Addr> {
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    let _ = socket.set_read_timeout(Some(Duration::from_millis(100)));
    let mut query = vec![0u8, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0];
    for service in ["_telnet", "_http"] {
        for label in [service, "_tcp", "local"] {
            query.push(label.len() as u8);
            query.extend_from_slice(label.as_bytes());
        }
        query.extend_from_slice(&[0, 0, 12, 0, 1]);
    }
    let _ = socket.send_to(&query, "224.0.0.251:5353");
    let mut hosts = Vec::new();
    let deadline = Instant::now() + wait;
    let mut buffer = [0u8; 1500];
    while Instant::now() < deadline {
        if let Ok((_, SocketAddr::V4(from))) = socket.recv_from(&mut buffer)
            && !hosts.contains(from.ip())
        {
            hosts.push(*from.ip());
        }
    }
    hosts
}

/// Everything found, USB first. `usb` probes the serial ports.
pub fn detect(usb: bool) -> Vec<Found> {
    let serial: Vec<std::thread::JoinHandle<Option<Found>>> = if usb {
        crate::list_serial_ports()
            .into_iter()
            .map(|(port, _)| std::thread::spawn(move || probe_serial(&port)))
            .collect()
    } else {
        Vec::new()
    };
    let beacons = crate::discovery::discover(Duration::from_secs(3));
    let announced = std::thread::spawn(|| mdns_hosts(Duration::from_secs(2)));
    let mut found: Vec<Found> = serial
        .into_iter()
        .filter_map(|worker| worker.join().ok().flatten())
        .collect();
    let mut network = scan_network();
    for host in announced.join().unwrap_or_default() {
        let known = network
            .iter()
            .any(|item| matches!(&item.link, Link::Network { host: known, .. } if *known == host.to_string()));
        if !known {
            for port in NETWORK_PORTS {
                if let Some(item) = probe_tcp(SocketAddr::from((host, port))) {
                    network.push(item);
                }
            }
        }
    }
    if let Ok(Ok(machines)) = beacons.recv() {
        for machine in machines {
            let link = Link::Network {
                host: machine.host.clone(),
                port: machine.port,
            };
            if !network.iter().any(|item| item.link == link) {
                network.push(Found {
                    link,
                    firmware: Firmware::Carvera,
                    details: machine.name,
                });
            }
        }
    }
    found.append(&mut network);
    found
}

/// `detect` in the background.
pub fn detect_in_background(usb: bool) -> Receiver<Vec<Found>> {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let _ = sender.send(detect(usb));
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn firmwares_are_recognised_from_their_answers() {
        assert_eq!(
            classify("Grbl 1.1h ['$' for help]\r\nok"),
            Some(Firmware::Grbl)
        );
        assert_eq!(
            classify("[VER:1.1f.20230120:]\n[OPT:VNMSL,35,1024,3]\nok"),
            Some(Firmware::GrblHal)
        );
        assert_eq!(
            classify("[MSG:INFO: FluidNC v3.7.8]"),
            Some(Firmware::FluidNc)
        );
        assert_eq!(
            classify("Build version: edge-1a2b, Build date: ..."),
            Some(Firmware::Smoothieware)
        );
        assert_eq!(classify("version = Carvera 1.0.4"), Some(Firmware::Carvera));
        assert_eq!(
            classify("FIRMWARE_NAME:Marlin 2.1.2 SOURCE_CODE_URL:..."),
            Some(Firmware::Marlin)
        );
        assert_eq!(
            classify("<Idle|MPos:0.000,0.000,0.000|FS:0,0>"),
            Some(Firmware::Grbl)
        );
        assert_eq!(classify(""), None);
        assert_eq!(Firmware::Carvera.protocol(), Protocol::SendAndWait);
        assert_eq!(Firmware::FluidNc.protocol(), Protocol::Grbl);
    }

    #[test]
    fn a_network_controller_is_identified_by_its_banner() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(b"Grbl 1.1h ['$' for help]\r\n").unwrap();
            let mut buffer = [0u8; 64];
            let _ = stream.read(&mut buffer);
            stream.write_all(b"[VER:1.1h.20190825:]\r\nok\r\n").unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });
        let found = probe_tcp(address).unwrap();
        assert_eq!(found.firmware, Firmware::Grbl);
        assert!(found.details.starts_with("Grbl 1.1h"), "{}", found.details);
        server.join().unwrap();
    }
}
