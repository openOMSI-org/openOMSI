use super::flush_ws;
use std::io::{ErrorKind, Read, Write};
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

pub(super) fn close<S: Read + Write>(ws: &mut WebSocket<S>, udp: &UdpSocket, buf: &mut [u8]) {
    loop {
        match udp.recv_from(buf) {
            Ok((n, _)) => {
                let _ = ws.send(Message::Binary(buf[..n].to_vec().into()));
            }
            Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
            Err(_) => break,
        }
    }
    let _ = flush_ws(ws);
    let _ = ws.close(None);
    let until = Instant::now() + Duration::from_secs(1);
    while Instant::now() < until {
        match ws.read() {
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                let _ = flush_ws(ws);
            }
            Err(_) => break,
        }
    }
}
