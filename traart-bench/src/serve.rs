//! The bench as a server: it hands out the VR page and the clock the headset
//! slaves to.
//!
//! The point is that there is exactly **one** playback cursor. The audio thread
//! owns it (`Shared::cursor_ns`, in experiment nanoseconds over the flattened
//! timeline), and the headset does not run a clock of its own -- it asks for
//! this one and predicts forward between polls. Whatever you hear and whatever
//! you see are then the same number, by construction rather than by agreement.
//!
//! Why the page is not just told "draw frame N": at a stretch of 1000, one
//! frame of wall clock is 16 microseconds of experiment time. Streaming that
//! would be absurd. Instead `/clock` returns the cursor plus the rate, the page
//! fits the offset between the two machines the way NTP does, and runs its own
//! rAF loop off the result. Four polls a second is plenty and the visual lands
//! within a frame of the sound.
//!
//! HTTPS, because WebXR refuses immersive mode on anything but a secure origin
//! and a LAN address over plain http is not one. The certificate is
//! self-signed and shared with `vr/serve.py` -- same `.certs/` directory, same
//! filenames -- so the headset only has to accept it once whichever of the two
//! is serving.

use crate::engine::Shared;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_PORT: u16 = 8443;

/// What the server may ask the UI to do. The UI owns the demo list and the
/// engine's parameters, so the server only ever posts intentions here and the
/// next frame of `App::update` drains them.
#[derive(Debug, Clone, Copy)]
pub enum Cmd {
    TogglePlay,
    SetPlaying(bool),
    NextDemo,
    SetDemo(usize),
    Stretch(f32),
    Rewind,
}

/// The narrow strip of UI state the server needs to describe, plus the command
/// queue coming back the other way.
pub struct Link {
    pub demo_id: String,
    pub demo_title: String,
    pub demo_index: usize,
    pub demo_count: usize,
    pub shots: u32,
    /// Raw bundle JSON, so a headset can fetch the exact score being played
    /// rather than a subset baked into the page months ago.
    pub raw: Arc<serde_json::Value>,
    pub cmds: VecDeque<Cmd>,
    /// Wall-clock ms of the last successful /clock poll, for the UI's tell-tale.
    pub last_poll_ms: u64,
    pub polls: u64,
    /// Bumped by the bench's "Exit VR" button; the page ends its immersive
    /// session whenever it sees the number change.
    pub exit_vr: u64,
}

impl Link {
    pub fn new(raw: Arc<serde_json::Value>) -> Link {
        Link {
            demo_id: String::new(),
            demo_title: String::new(),
            demo_index: 0,
            demo_count: 0,
            shots: 1,
            raw,
            cmds: VecDeque::new(),
            last_poll_ms: 0,
            polls: 0,
            exit_vr: 0,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

// ── addresses ───────────────────────────────────────────────────────────────

/// Every IPv4 this machine answers on, the outward-facing one first.
pub fn local_ips() -> Vec<String> {
    let mut ips: Vec<String> = Vec::new();
    // Which source address would the kernel pick to reach the outside? UDP
    // connect() only sets up a route, it sends nothing, so this works offline.
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        if s.connect("192.0.2.1:1").is_ok() {
            if let Ok(a) = s.local_addr() {
                if let IpAddr::V4(v4) = a.ip() {
                    if !v4.is_loopback() {
                        ips.push(v4.to_string());
                    }
                }
            }
        }
    }
    for cmd in ["ifconfig", "ip"] {
        let out = if cmd == "ip" {
            std::process::Command::new("ip").args(["-4", "addr"]).output()
        } else {
            std::process::Command::new("ifconfig").output()
        };
        let Ok(out) = out else { continue };
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let mut it = line.split_whitespace();
            while let Some(tok) = it.next() {
                if tok != "inet" {
                    continue;
                }
                if let Some(v) = it.next() {
                    let v = v.split('/').next().unwrap_or(v);
                    if v.parse::<std::net::Ipv4Addr>().is_ok()
                        && v != "127.0.0.1"
                        && !ips.iter().any(|e| e == v)
                    {
                        ips.push(v.to_string());
                    }
                }
            }
        }
        if !ips.is_empty() {
            break;
        }
    }
    ips
}

// ── certificate ─────────────────────────────────────────────────────────────

/// Load `.certs/{cert,key}.pem`, regenerating when this machine's addresses
/// have moved. `ips.txt` records what the stored certificate was made for, so
/// the check costs no X.509 parsing.
fn load_or_make_cert(
    dir: &Path,
    ips: &[String],
) -> Result<
    (
        Vec<rustls::pki_types::CertificateDer<'static>>,
        rustls::pki_types::PrivateKeyDer<'static>,
    ),
    String,
> {
    let cdir = dir.join(".certs");
    let cert_p = cdir.join("cert.pem");
    let key_p = cdir.join("key.pem");
    let ips_p = cdir.join("ips.txt");
    let want = ips.join(",");

    let fresh = std::fs::read_to_string(&ips_p).map(|s| s.trim() == want).unwrap_or(false)
        && cert_p.exists()
        && key_p.exists();

    if !fresh {
        let mut names: Vec<String> = vec!["localhost".into(), "127.0.0.1".into()];
        names.extend(ips.iter().cloned());
        let ck = rcgen::generate_simple_self_signed(names)
            .map_err(|e| format!("could not generate a certificate: {e}"))?;
        std::fs::create_dir_all(&cdir).map_err(|e| e.to_string())?;
        std::fs::write(&cert_p, ck.cert.pem()).map_err(|e| e.to_string())?;
        std::fs::write(&key_p, ck.key_pair.serialize_pem()).map_err(|e| e.to_string())?;
        std::fs::write(&ips_p, &want).map_err(|e| e.to_string())?;
    }

    let mut cr = std::io::BufReader::new(
        std::fs::File::open(&cert_p).map_err(|e| format!("{}: {e}", cert_p.display()))?,
    );
    let certs: Vec<_> = rustls_pemfile::certs(&mut cr)
        .collect::<Result<_, _>>()
        .map_err(|e| format!("bad certificate: {e}"))?;
    let mut kr = std::io::BufReader::new(
        std::fs::File::open(&key_p).map_err(|e| format!("{}: {e}", key_p.display()))?,
    );
    let key = rustls_pemfile::private_key(&mut kr)
        .map_err(|e| format!("bad key: {e}"))?
        .ok_or_else(|| "no private key in key.pem".to_string())?;
    if certs.is_empty() {
        return Err("no certificate in cert.pem".into());
    }
    Ok((certs, key))
}

// ── the server ──────────────────────────────────────────────────────────────

pub struct Serving {
    pub urls: Vec<String>,
    pub port: u16,
    pub dir: PathBuf,
}

/// Start listening. Returns once the socket is bound; the work happens on
/// detached threads, one per connection.
pub fn spawn(
    shared: Arc<Mutex<Shared>>,
    link: Arc<Mutex<Link>>,
    dir: PathBuf,
    port: u16,
) -> Result<Serving, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let ips = local_ips();
    let (certs, key) = load_or_make_cert(&dir, &ips)?;
    let cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("TLS config: {e}"))?;
    let cfg = Arc::new(cfg);

    let listener = TcpListener::bind(("0.0.0.0", port))
        .map_err(|e| format!("cannot listen on port {port}: {e}"))?;

    let urls: Vec<String> = if ips.is_empty() {
        vec![format!("https://localhost:{port}")]
    } else {
        ips.iter().map(|ip| format!("https://{ip}:{port}")).collect()
    };

    let d = dir.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(sock) = stream else { continue };
            let cfg = cfg.clone();
            let shared = shared.clone();
            let link = link.clone();
            let d = d.clone();
            // One thread per connection, and the handshake happens *in* it:
            // doing it in the accept loop lets one silent preconnect -- which
            // Chromium-based browsers open constantly -- wedge the whole server.
            std::thread::spawn(move || {
                let _ = sock.set_read_timeout(Some(Duration::from_secs(20)));
                let _ = sock.set_write_timeout(Some(Duration::from_secs(30)));
                serve_conn(sock, cfg, shared, link, d);
            });
        }
    });

    Ok(Serving { urls, port, dir })
}

fn serve_conn(
    mut sock: TcpStream,
    cfg: Arc<rustls::ServerConfig>,
    shared: Arc<Mutex<Shared>>,
    link: Arc<Mutex<Link>>,
    dir: PathBuf,
) {
    // A TLS ClientHello starts with a handshake record, 0x16. Anything else is
    // a browser that assumed http:// for a bare `address:port` -- which every
    // URL bar does, and which shows up in a headset as an eternal spinner.
    // Send them back with the right scheme instead of failing the handshake.
    let mut first = [0u8; 1];
    match sock.peek(&mut first) {
        Ok(1) if first[0] != 0x16 => {
            let req = read_request(&mut sock).unwrap_or_default();
            let (_, path, host) = parse_request(&req);
            let host = if host.is_empty() {
                sock.local_addr().map(|a| a.to_string()).unwrap_or_default()
            } else {
                host
            };
            let body = b"";
            let head = format!(
                "HTTP/1.0 301 Moved Permanently\r\nLocation: https://{host}{path}\r\n\
                 Content-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = sock.write_all(head.as_bytes());
            let _ = sock.write_all(body);
            return;
        }
        Ok(_) => {}
        Err(_) => return,
    }

    let Ok(conn) = rustls::ServerConnection::new(cfg) else { return };
    let mut tls = rustls::StreamOwned::new(conn, sock);
    let Some(req) = read_request(&mut tls) else { return };
    let (method, path, _) = parse_request(&req);
    if method != "GET" && method != "HEAD" {
        let _ = respond(&mut tls, 405, "text/plain", b"method not allowed");
        return;
    }
    route(&mut tls, &path, &shared, &link, &dir);
}

fn read_request<S: Read>(s: &mut S) -> Option<String> {
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    while buf.len() < 8192 {
        match s.read(&mut byte) {
            Ok(0) | Err(_) => return None,
            Ok(_) => buf.push(byte[0]),
        }
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            return Some(String::from_utf8_lossy(&buf).into_owned());
        }
    }
    None
}

/// -> (method, path-with-query, host)
fn parse_request(req: &str) -> (String, String, String) {
    let mut lines = req.lines();
    let first = lines.next().unwrap_or("");
    let mut it = first.split_whitespace();
    let method = it.next().unwrap_or("").to_string();
    let path = it.next().unwrap_or("/").to_string();
    let mut host = String::new();
    for l in lines {
        if let Some(v) = l.strip_prefix("Host:").or_else(|| l.strip_prefix("host:")) {
            host = v.trim().to_string();
        }
    }
    (method, path, host)
}

fn respond<S: Write>(s: &mut S, code: u16, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    let head = format!(
        "HTTP/1.0 {code} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes())?;
    s.write_all(body)?;
    s.flush()
}

fn redirect<S: Write>(s: &mut S, to: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.0 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    s.write_all(head.as_bytes())?;
    s.flush()
}

fn route<S: Read + Write>(
    s: &mut S,
    raw_path: &str,
    shared: &Arc<Mutex<Shared>>,
    link: &Arc<Mutex<Link>>,
    dir: &Path,
) {
    let (path, query) = match raw_path.split_once('?') {
        Some((p, q)) => (p, q),
        None => (raw_path, ""),
    };

    let _ = match path {
        "/ping" => respond(s, 200, "text/plain", b"pong\n"),
        "/" | "/index.html" => redirect(s, "/trap_vr.html"),
        "/clock" => {
            let body = clock_json(shared, link);
            if let Ok(mut l) = link.lock() {
                l.last_poll_ms = now_ms();
                l.polls = l.polls.wrapping_add(1);
            }
            respond(s, 200, "application/json", body.as_bytes())
        }
        "/cmd" => {
            apply_cmd(query, link);
            let body = clock_json(shared, link);
            respond(s, 200, "application/json", body.as_bytes())
        }
        "/score" => match score_json(query, link) {
            Some(body) => respond(s, 200, "application/json", body.as_bytes()),
            None => respond(s, 404, "text/plain", b"no such run"),
        },
        _ => {
            serve_file(s, path, dir);
            Ok(())
        }
    };
}

/// The whole contract with the headset, in one small object.
fn clock_json(shared: &Arc<Mutex<Shared>>, link: &Arc<Mutex<Link>>) -> String {
    let (t_ns, total_ns, stretch, playing, tl_gen) = match shared.lock() {
        Ok(sh) => (
            sh.cursor_ns,
            sh.timeline.total_ns,
            sh.params.stretch,
            sh.params.playing,
            sh.timeline_gen,
        ),
        Err(_) => (0.0, 1.0, 1000.0, false, 0),
    };
    let (id, title, idx, count, shots, exit_vr) = match link.lock() {
        Ok(l) => (
            l.demo_id.clone(),
            l.demo_title.clone(),
            l.demo_index,
            l.demo_count,
            l.shots,
            l.exit_vr,
        ),
        Err(_) => (String::new(), String::new(), 0, 0, 1, 0),
    };
    format!(
        "{{\"ok\":true,\"t_ns\":{t_ns:.1},\"total_ns\":{total_ns:.1},\
         \"stretch\":{stretch},\"playing\":{playing},\"gen\":{tl_gen},\
         \"demo\":\"{id}\",\"title\":\"{}\",\"demo_index\":{idx},\
         \"demo_count\":{count},\"shots\":{shots},\"exit_vr\":{exit_vr},\
         \"server_us\":{}}}",
        title.replace('"', "'"),
        now_us()
    )
}

fn apply_cmd(query: &str, link: &Arc<Mutex<Link>>) {
    let Ok(mut l) = link.lock() else { return };
    for kv in query.split('&') {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        let cmd = match k {
            "toggle" => Some(Cmd::TogglePlay),
            "play" => Some(Cmd::SetPlaying(v != "0" && v != "false")),
            "next" => Some(Cmd::NextDemo),
            "rewind" => Some(Cmd::Rewind),
            "demo" => v.parse().ok().map(Cmd::SetDemo),
            "stretch" => v.parse().ok().map(Cmd::Stretch),
            _ => None,
        };
        if let Some(c) = cmd {
            if l.cmds.len() < 32 {
                l.cmds.push_back(c);
            }
        }
    }
}

/// One run of the bundle, verbatim. The headset builds its timeline from this,
/// so both sides are flattening the same numbers -- a subset baked into the
/// page would put every index out of step with the audio.
fn score_json(query: &str, link: &Arc<Mutex<Link>>) -> Option<String> {
    let Ok(l) = link.lock() else { return None };
    let mut want: Option<String> = None;
    for kv in query.split('&') {
        if let Some((k, v)) = kv.split_once('=') {
            if k == "demo" {
                want = Some(v.to_string());
            }
        }
    }
    let want = want.unwrap_or_else(|| l.demo_id.clone());
    let demos = l.raw.get("demos")?.as_array()?;
    let d = demos
        .iter()
        .find(|d| d.get("id").and_then(|v| v.as_str()) == Some(want.as_str()))?;
    serde_json::to_string(d).ok()
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, e)| e) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("json") => "application/json",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        Some("md") | Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn serve_file<S: Write>(s: &mut S, path: &str, dir: &Path) {
    let rel = path.trim_start_matches('/');
    if rel.is_empty()
        || rel.contains("..")
        || rel.starts_with('.')
        || rel.contains("/.")
        || Path::new(rel).is_absolute()
    {
        let _ = respond(s, 404, "text/plain", b"not found");
        return;
    }
    let full = dir.join(rel);
    match std::fs::read(&full) {
        Ok(bytes) => {
            let _ = respond(s, 200, content_type(rel), &bytes);
        }
        Err(_) => {
            let _ = respond(s, 404, "text/plain", b"not found");
        }
    }
}
