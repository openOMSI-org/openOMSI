//! The gateway's HTTP requests read off the socket: the whole head before anything is decided
//! from it, the body, all within one deadline, and an answer that turns a request away so its
//! client reads it.

use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// The longest request head read (the headers a browser and a proxy send fit well).
pub(crate) const HEAD_MAX: usize = 8192;
/// How long a request's head and body may take to come in, all of it (a byte at a time
/// would otherwise keep a place for hours).
pub(crate) const REQUEST_TIME: Duration = Duration::from_secs(10);

fn head_complete(b: &[u8]) -> bool {
    b.windows(4).any(|w| w == b"\r\n\r\n")
}

/// Each read waits only for what is left until `deadline`; false once it has passed.
fn wait_until(s: &TcpStream, deadline: Instant) -> bool {
    let left = deadline.saturating_duration_since(Instant::now());
    !left.is_zero() && s.set_read_timeout(Some(left)).is_ok()
}

/// The request's head as it stands on the socket, without taking it off - the whole head, as
/// long as it comes before `deadline` and within `HEAD_MAX` (what is decided from it - the
/// path, a WebSocket, who the client is - must not be decided from a part).
pub(crate) fn peek_head(s: &TcpStream, deadline: Instant) -> Vec<u8> {
    let mut buf = vec![0u8; HEAD_MAX];
    let mut seen = 0;
    loop {
        let n = s.peek(&mut buf).unwrap_or(0);
        if n == 0 || head_complete(&buf[..n]) || n >= HEAD_MAX || !wait_until(s, deadline) {
            buf.truncate(n);
            return buf;
        }
        if n == seen {
            // (nothing new yet: a peek gives back what it gave)
            std::thread::sleep(Duration::from_millis(10));
        }
        seen = n;
    }
}

/// Read on until the head's end (`\r\n\r\n`): Err with the answer when it never comes
/// (too long, too slow, or the connection ended first).
pub(crate) fn read_head(s: &mut TcpStream, request: &mut Vec<u8>, deadline: Instant) -> Result<(), &'static str> {
    let mut buf = [0u8; 1024];
    loop {
        if head_complete(request) {
            return Ok(());
        }
        if request.len() >= HEAD_MAX {
            return Err("431 Request Header Fields Too Large");
        }
        if !wait_until(s, deadline) {
            return Err("408 Request Timeout");
        }
        match s.read(&mut buf) {
            Ok(0) => return Err("400 Bad Request"),
            Ok(n) => request.extend_from_slice(&buf[..n]),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return Err("408 Request Timeout"),
            Err(_) => return Err("400 Bad Request"),
        }
    }
}

/// The rest of a request's body, up to its `Content-Length` (4 KiB at most), before
/// `deadline`.
pub(crate) fn read_body(s: &mut TcpStream, request: &mut Vec<u8>, deadline: Instant) {
    // (the head's end found in the bytes: an offset into the lossy text is not one into them)
    let Some(head_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else { return };
    let text = String::from_utf8_lossy(&request[..head_end]).to_string();
    let want = header(&text, "content-length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0).min(4096);
    let mut buf = [0u8; 1024];
    while request.len() < head_end + 4 + want && wait_until(s, deadline) {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => request.extend_from_slice(&buf[..n]),
        }
    }
}

/// Turn a request away with `status`, so the client reads the answer: what it still sends is
/// read and dropped for a moment before the connection closes (closed with bytes unread,
/// Windows resets it and the answer is lost).
pub(crate) fn refuse(mut s: TcpStream, status: &str, extra: &str) {
    let _ = s.write_all(format!("HTTP/1.1 {status}\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n").as_bytes());
    let _ = s.shutdown(std::net::Shutdown::Write);
    let until = Instant::now() + Duration::from_secs(1);
    let mut buf = [0u8; 4096];
    let mut drained = 0;
    while drained < 64 * 1024 && wait_until(&s, until) {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => drained += n,
        }
    }
}

pub(crate) fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim()))
}

/// A header's last line (a proxy adds its own after the client's).
pub(crate) fn last_header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).filter_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim())).last()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A head that comes a byte at a time has its time, all of it: not each byte's.
    #[test]
    fn a_slow_head_has_one_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut c = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut srv, _) = listener.accept().unwrap();
        std::thread::spawn(move || {
            for b in b"GET /status HTTP/1.1\r\nX-Slow: ".iter().cycle().take(200) {
                if c.write_all(&[*b]).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        let t0 = Instant::now();
        let mut request = Vec::new();
        assert_eq!(read_head(&mut srv, &mut request, t0 + Duration::from_millis(400)), Err("408 Request Timeout"));
        assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
        assert!(!request.is_empty());
    }
}
