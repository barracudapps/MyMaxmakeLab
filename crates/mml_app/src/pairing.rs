//! Pairing with a tablet or phone: a small HTTP server on the local
//! network serves a drawing page (stylus pressure, coalesced events),
//! shows the current design behind the drawing and receives strokes.
//! A random token in the address keeps other devices of the network
//! out; nothing leaves the local network.

use mml_core::document::Document;
use mml_core::geometry::Rect;
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PAGE: &str = include_str!("../assets/web/draw.html");

/// What the page shows behind the drawing, and the drawing area.
#[derive(Default)]
struct SharedView {
    svg: String,
    width: f64,
    height: f64,
}

pub type Stroke = Vec<[f64; 2]>;

pub struct Pairing {
    pub url: String,
    /// Machine area the page maps to.
    pub area: Rect,
    pub strokes: Receiver<Vec<Stroke>>,
    shared: Arc<Mutex<SharedView>>,
    stop: Arc<AtomicBool>,
    pub received: usize,
}

impl Drop for Pairing {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Address of this computer on the local network. Connecting a UDP
/// socket sends nothing; it only selects the outgoing interface.
fn local_address() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("192.168.1.1:9")?;
            socket.local_addr()
        })
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".into())
}

fn random_token() -> String {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    format!("{:016x}", hasher.finish())
}

impl Pairing {
    pub fn start(area: Rect) -> std::io::Result<Self> {
        let listener =
            TcpListener::bind("0.0.0.0:47820").or_else(|_| TcpListener::bind("0.0.0.0:0"))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let token = random_token();
        let url = format!("http://{}:{port}/?t={token}", local_address());
        let shared = Arc::new(Mutex::new(SharedView {
            svg: String::new(),
            width: area.width(),
            height: area.height(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, strokes) = channel();
        let thread_shared = shared.clone();
        let thread_stop = stop.clone();
        std::thread::Builder::new()
            .name("pairing".into())
            .spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let _ = serve(stream, &token, &thread_shared, &sender);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(30));
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(200)),
                    }
                }
            })?;
        Ok(Self {
            url,
            area,
            strokes,
            shared,
            stop,
            received: 0,
        })
    }

    pub fn update_view(&self, document: &Document) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.svg = design_svg(document, self.area);
            shared.width = self.area.width();
            shared.height = self.area.height();
        }
    }
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)
}

fn serve(
    mut stream: TcpStream,
    token: &str,
    shared: &Mutex<SharedView>,
    sender: &Sender<Vec<Stroke>>,
) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..count]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        if buffer.len() > 64 * 1024 {
            return respond(
                &mut stream,
                "413 Payload Too Large",
                "text/plain",
                b"too large",
            );
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.lines();
    let request = lines.next().unwrap_or_default();
    let mut parts = request.split_whitespace();
    let (method, target) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let authorised = query.split('&').any(|pair| pair == format!("t={token}"));
    if !authorised {
        return respond(
            &mut stream,
            "403 Forbidden",
            "text/plain",
            b"pairing code required",
        );
    }
    let content_length: usize = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0)
        .min(8 * 1024 * 1024);
    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..count]);
    }
    match (method, path) {
        ("GET", "/") => respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
        ),
        ("GET", "/area") => {
            let (width, height) = shared
                .lock()
                .map(|view| (view.width, view.height))
                .unwrap_or((100.0, 100.0));
            let json = format!("{{\"width\":{width},\"height\":{height}}}");
            respond(&mut stream, "200 OK", "application/json", json.as_bytes())
        }
        ("GET", "/design.svg") => {
            let svg = shared
                .lock()
                .map(|view| view.svg.clone())
                .unwrap_or_default();
            respond(&mut stream, "200 OK", "image/svg+xml", svg.as_bytes())
        }
        ("POST", "/strokes") => match parse_strokes(&body) {
            Some(strokes) => {
                let _ = sender.send(strokes);
                respond(&mut stream, "200 OK", "text/plain", b"ok")
            }
            None => respond(
                &mut stream,
                "400 Bad Request",
                "text/plain",
                b"invalid strokes",
            ),
        },
        _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
    }
}

/// `{"strokes": [[[x, y], ...], ...]}` in millimetres of the area.
pub fn parse_strokes(body: &[u8]) -> Option<Vec<Stroke>> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let strokes = value.get("strokes")?.as_array()?;
    let parsed: Vec<Stroke> = strokes
        .iter()
        .filter_map(|stroke| {
            let points: Stroke = stroke
                .as_array()?
                .iter()
                .filter_map(|point| {
                    let point = point.as_array()?;
                    Some([point.first()?.as_f64()?, point.get(1)?.as_f64()?])
                })
                .filter(|[x, y]| x.is_finite() && y.is_finite())
                .collect();
            (points.len() >= 2).then_some(points)
        })
        .collect();
    (!parsed.is_empty()).then_some(parsed)
}

/// The design inside `area` as SVG (Y up converted to SVG Y down), drawn
/// behind the tablet strokes so drawing aligns with existing work.
pub fn design_svg(document: &Document, area: Rect) -> String {
    let (width, height) = (area.width(), area.height());
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width:.3} {height:.3}\" preserveAspectRatio=\"none\">"
    );
    for shape in &document.shapes {
        let color = document
            .layer(shape.layer)
            .map_or([0, 0, 0], |layer| layer.color);
        for path in shape.world_paths() {
            if path.points.len() < 2 {
                continue;
            }
            let points: Vec<String> = path
                .points
                .iter()
                .map(|point| {
                    format!(
                        "{:.2},{:.2}",
                        point.x - area.min.x,
                        height - (point.y - area.min.y)
                    )
                })
                .collect();
            let element = if path.closed { "polygon" } else { "polyline" };
            svg.push_str(&format!(
                "<{element} points=\"{}\" fill=\"none\" stroke=\"rgb({},{},{})\" stroke-width=\"{:.3}\"/>",
                points.join(" "),
                color[0],
                color[1],
                color[2],
                width.max(height) / 300.0
            ));
        }
    }
    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use mml_core::geometry::Point;

    #[test]
    fn strokes_are_parsed_and_invalid_ones_dropped() {
        let body = br#"{"strokes":[[[0,0],[1,2],[3,4]],[[5,5]],[["a",1],[2,2]]]}"#;
        let strokes = parse_strokes(body).unwrap();
        assert_eq!(strokes.len(), 1);
        assert_eq!(strokes[0][2], [3.0, 4.0]);
        assert!(parse_strokes(b"nonsense").is_none());
    }

    #[test]
    fn server_refuses_requests_without_the_pairing_code() {
        let area = Rect::from_points(Point::new(0.0, 0.0), Point::new(100.0, 50.0));
        let pairing = Pairing::start(area).unwrap();
        let address = pairing
            .url
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap()
            .to_owned();
        let port: u16 = address.rsplit(':').next().unwrap().parse().unwrap();
        let request = |target: &str, body: &str| -> String {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let method = if body.is_empty() { "GET" } else { "POST" };
            write!(
                stream,
                "{method} {target} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        };
        let refused = request("/", "");
        assert!(
            refused.starts_with("HTTP/1.1 403"),
            "got {refused:?} from {}",
            pairing.url
        );
        let token = pairing.url.split("t=").nth(1).unwrap().to_owned();
        assert!(request(&format!("/?t={token}"), "").contains("MyMaxmakeLab"));
        assert!(request(&format!("/area?t={token}"), "").contains("\"width\":100"));
        let ok = request(
            &format!("/strokes?t={token}"),
            r#"{"strokes":[[[1,1],[9,9]]]}"#,
        );
        assert!(ok.starts_with("HTTP/1.1 200"));
        let received = pairing
            .strokes
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(received[0][1], [9.0, 9.0]);
    }
}
