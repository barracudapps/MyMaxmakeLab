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
const MONITOR: &str = include_str!("../assets/web/monitor.html");

/// What a phone may ask: only commands that make the machine safer.
/// Nothing can start or resume a job from the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCommand {
    Pause,
    EmergencyStop,
}

/// What the page shows behind the drawing, and the drawing area.
#[derive(Default)]
struct SharedView {
    svg: String,
    width: f64,
    height: f64,
    /// Job and machine state for the monitoring page, as JSON.
    status: String,
    /// Translated texts of the drawing page, as a JSON object.
    labels: String,
}

/// One stroke of the tablet, in millimetres of the area (Y up).
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// Identifier chosen by the page, stable while the stroke exists.
    pub id: String,
    pub style: mml_core::brush::BrushStyle,
    /// Intensity 1 (light) to 3 (strong).
    pub level: u8,
    pub width: f64,
    /// x, y and stylus pressure.
    pub points: Vec<[f64; 3]>,
}

/// Every stroke a page holds: strokes no longer listed were erased.
#[derive(Debug, Clone, PartialEq)]
pub struct Sync {
    /// Random per page load, so two tablets do not erase each other.
    pub session: String,
    pub strokes: Vec<Stroke>,
}

pub struct Pairing {
    pub url: String,
    /// Machine area the page maps to.
    pub area: Rect,
    pub strokes: Receiver<Sync>,
    pub remote: Receiver<RemoteCommand>,
    /// Address of the monitoring page.
    pub monitor_url: String,
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
        let address = local_address();
        let url = format!("http://{address}:{port}/?t={token}");
        let monitor_url = format!("http://{address}:{port}/monitor?t={token}");
        let shared = Arc::new(Mutex::new(SharedView {
            svg: String::new(),
            width: area.width(),
            height: area.height(),
            status: "{}".into(),
            labels: "{}".into(),
        }));
        let (remote_sender, remote) = channel();
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
                            let _ = serve(stream, &token, &thread_shared, &sender, &remote_sender);
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
            remote,
            monitor_url,
            shared,
            stop,
            received: 0,
        })
    }

    pub fn update_status(&self, json: String) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.status = json;
        }
    }

    pub fn update_labels(&self, json: String) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.labels = json;
        }
    }

    /// Design shown behind the drawing, without the shapes that came
    /// from the tablet (the page draws those itself).
    pub fn update_view(&self, document: &Document, skip: &[mml_core::document::ShapeId]) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.svg = design_svg(document, self.area, skip);
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
    sender: &Sender<Sync>,
    remote: &Sender<RemoteCommand>,
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
        ("GET", "/monitor") => respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            MONITOR.as_bytes(),
        ),
        ("GET", "/status") => {
            let json = shared
                .lock()
                .map(|view| view.status.clone())
                .unwrap_or_default();
            respond(&mut stream, "200 OK", "application/json", json.as_bytes())
        }
        ("POST", "/pause") => {
            let _ = remote.send(RemoteCommand::Pause);
            respond(&mut stream, "200 OK", "text/plain", b"ok")
        }
        ("POST", "/estop") => {
            let _ = remote.send(RemoteCommand::EmergencyStop);
            respond(&mut stream, "200 OK", "text/plain", b"ok")
        }
        ("GET", "/area") => {
            let (width, height, labels) = shared
                .lock()
                .map(|view| (view.width, view.height, view.labels.clone()))
                .unwrap_or((100.0, 100.0, "{}".into()));
            let json = format!("{{\"width\":{width},\"height\":{height},\"labels\":{labels}}}");
            respond(&mut stream, "200 OK", "application/json", json.as_bytes())
        }
        ("GET", "/design.svg") => {
            let svg = shared
                .lock()
                .map(|view| view.svg.clone())
                .unwrap_or_default();
            respond(&mut stream, "200 OK", "image/svg+xml", svg.as_bytes())
        }
        ("POST", "/sync") => match parse_sync(&body) {
            Some(sync) => {
                let _ = sender.send(sync);
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

/// `{"session": "...", "strokes": [{"id": "...", "style": "brush",
/// "level": 2, "width": 1.5, "points": [[x, y, pressure], ...]}, ...]}`
/// in millimetres of the area. Invalid strokes are dropped; an empty
/// list is valid (everything erased).
pub fn parse_sync(body: &[u8]) -> Option<Sync> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let session = value.get("session")?.as_str()?.chars().take(64).collect();
    let strokes = value
        .get("strokes")?
        .as_array()?
        .iter()
        .take(20_000)
        .filter_map(|stroke| {
            let id = stroke.get("id")?.as_str()?.chars().take(64).collect();
            let style = stroke
                .get("style")
                .and_then(|style| serde_json::from_value(style.clone()).ok())
                .unwrap_or_default();
            let level = stroke
                .get("level")
                .and_then(|level| level.as_u64())
                .unwrap_or(3)
                .clamp(1, 3) as u8;
            let width = stroke
                .get("width")
                .and_then(|width| width.as_f64())
                .filter(|width| width.is_finite())
                .unwrap_or(1.0)
                .clamp(0.05, 100.0);
            let points: Vec<[f64; 3]> = stroke
                .get("points")?
                .as_array()?
                .iter()
                .filter_map(|point| {
                    let point = point.as_array()?;
                    let pressure = point.get(2).and_then(|p| p.as_f64()).unwrap_or(0.5);
                    Some([point.first()?.as_f64()?, point.get(1)?.as_f64()?, pressure])
                })
                .filter(|point| point.iter().all(|value| value.is_finite()))
                .collect();
            (!points.is_empty()).then_some(Stroke {
                id,
                style,
                level,
                width,
                points,
            })
        })
        .collect();
    Some(Sync { session, strokes })
}

/// The design inside `area` as SVG (Y up converted to SVG Y down), drawn
/// behind the tablet strokes so drawing aligns with existing work.
pub fn design_svg(document: &Document, area: Rect, skip: &[mml_core::document::ShapeId]) -> String {
    let (width, height) = (area.width(), area.height());
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width:.3} {height:.3}\" preserveAspectRatio=\"none\">"
    );
    for shape in document
        .shapes
        .iter()
        .filter(|shape| !skip.contains(&shape.id))
    {
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
        let body = br#"{"session":"s1","strokes":[
            {"id":"a","style":"brush","level":2,"width":1.5,"points":[[0,0,0.2],[1,2,0.9]]},
            {"id":"b","points":[]},
            {"id":"c","style":"marker","level":9,"points":[[5,5]]}]}"#;
        let sync = parse_sync(body).unwrap();
        assert_eq!(sync.session, "s1");
        let strokes = &sync.strokes;
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes[0].style, mml_core::brush::BrushStyle::Brush);
        assert_eq!(strokes[0].points[1], [1.0, 2.0, 0.9]);
        assert_eq!((strokes[1].level, strokes[1].points[0][2]), (3, 0.5));
        assert!(
            parse_sync(br#"{"session":"s","strokes":[]}"#)
                .unwrap()
                .strokes
                .is_empty()
        );
        assert!(parse_sync(b"nonsense").is_none());
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
            &format!("/sync?t={token}"),
            r#"{"session":"x","strokes":[{"id":"1","points":[[1,1],[9,9]]}]}"#,
        );
        assert!(ok.starts_with("HTTP/1.1 200"));
        let received = pairing
            .strokes
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(received.strokes[0].points[1], [9.0, 9.0, 0.5]);
        // Monitoring: the page and the status need the code too, and a
        // phone can only pause or stop.
        assert!(request("/status", "").starts_with("HTTP/1.1 403"));
        assert!(request("/pause", "x").starts_with("HTTP/1.1 403"));
        pairing.update_status("{\"state\":\"Run\"}".into());
        assert!(request(&format!("/monitor?t={token}"), "").contains("estop"));
        assert!(request(&format!("/status?t={token}"), "").contains("\"Run\""));
        assert!(request(&format!("/pause?t={token}"), "x").starts_with("HTTP/1.1 200"));
        assert!(request(&format!("/estop?t={token}"), "x").starts_with("HTTP/1.1 200"));
        assert_eq!(
            pairing.remote.recv_timeout(Duration::from_secs(2)).unwrap(),
            RemoteCommand::Pause
        );
        assert_eq!(
            pairing.remote.recv_timeout(Duration::from_secs(2)).unwrap(),
            RemoteCommand::EmergencyStop
        );
        assert!(request(&format!("/resume?t={token}"), "x").starts_with("HTTP/1.1 404"));
    }
}
