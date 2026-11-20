//! Finds Makera Carvera machines on the local network. Each machine
//! broadcasts `name,ip,port,busy` on UDP port 3333 every few seconds;
//! listening is enough, nothing is sent.

use std::net::UdpSocket;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

pub const BEACON_PORT: u16 = 3333;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    pub name: String,
    pub host: String,
    pub port: u16,
    /// Another program is already connected to it.
    pub busy: bool,
}

pub fn parse_beacon(data: &[u8]) -> Option<Discovered> {
    let text = std::str::from_utf8(data)
        .ok()?
        .trim_matches(['\0', '\r', '\n', ' ']);
    let mut fields = text.split(',').map(str::trim);
    let name = fields.next().filter(|name| !name.is_empty())?.to_owned();
    let host = fields.next()?;
    host.parse::<std::net::Ipv4Addr>().ok()?;
    let port = fields.next()?.parse().ok()?;
    let busy = fields.next()? == "1";
    Some(Discovered {
        name,
        host: host.to_owned(),
        port,
        busy,
    })
}

/// Listens for `duration` in a background thread and sends the distinct
/// machines heard, or the reason listening failed.
pub fn discover(duration: Duration) -> Receiver<Result<Vec<Discovered>, String>> {
    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        let _ = sender.send(listen(BEACON_PORT, duration));
    });
    receiver
}

fn listen(port: u16, duration: Duration) -> Result<Vec<Discovered>, String> {
    let socket = UdpSocket::bind(("0.0.0.0", port)).map_err(|error| error.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|error| error.to_string())?;
    let mut found: Vec<Discovered> = Vec::new();
    let mut buffer = [0u8; 256];
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if let Ok((size, _)) = socket.recv_from(&mut buffer)
            && let Some(machine) = parse_beacon(&buffer[..size])
            && !found.iter().any(|known| known.name == machine.name)
        {
            found.push(machine);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacons_are_parsed_and_junk_rejected() {
        assert_eq!(
            parse_beacon(b"Carvera_01,192.168.1.42,2222,0\0"),
            Some(Discovered {
                name: "Carvera_01".into(),
                host: "192.168.1.42".into(),
                port: 2222,
                busy: false,
            })
        );
        assert!(parse_beacon(b"Carvera,192.168.1.42,2222,1").unwrap().busy);
        assert_eq!(parse_beacon(b"hello"), None);
        assert_eq!(parse_beacon(b"x,not-an-ip,2222,0"), None);
        assert_eq!(parse_beacon(&[0xff, 0xfe]), None);
    }

    #[test]
    fn a_broadcast_machine_is_heard_once() {
        let port = {
            let probe = UdpSocket::bind("127.0.0.1:0").unwrap();
            probe.local_addr().unwrap().port()
        };
        let listener = std::thread::spawn(move || listen(port, Duration::from_millis(900)));
        std::thread::sleep(Duration::from_millis(150));
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        for _ in 0..3 {
            sender
                .send_to(b"Carvera_A,127.0.0.1,2222,0", ("127.0.0.1", port))
                .unwrap();
            std::thread::sleep(Duration::from_millis(50));
        }
        let found = listener.join().unwrap().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].port, 2222);
    }
}
