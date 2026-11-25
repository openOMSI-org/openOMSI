//! Say where the official server is reached, for a server whose own binary does not do it
//! (`OMSI_OFFICIAL_KEY` in the server does the same from inside):
//! `official_announce <key file> <server.log>` follows the log for the tunnel's address and
//! posts it, signed, now and every five minutes (see `omsi_net::official`).
use std::time::{Duration, Instant};

fn latest_tunnel(log: &str) -> Option<String> {
    let text = std::fs::read(log).ok()?;
    // (only the end: the log grows for days)
    let tail = &text[text.len().saturating_sub(4 << 20)..];
    let s = String::from_utf8_lossy(tail);
    let mut found = None;
    for (i, _) in s.match_indices("https://") {
        let rest = &s[i..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '/' | '.' | '-')))
            .unwrap_or(rest.len());
        let url = &rest[..end];
        if url.ends_with(".trycloudflare.com") && !url.contains("api.trycloudflare") {
            found = Some(url.to_string());
        }
    }
    found
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(key), Some(log)) = (args.next(), args.next()) else {
        eprintln!("official_announce <key file> <server.log>");
        std::process::exit(2);
    };
    let key = std::fs::read(&key).expect("key file");
    let mut last: Option<(String, Instant)> = None;
    loop {
        if let Some(url) = latest_tunnel(&log) {
            let due = last
                .as_ref()
                .is_none_or(|(u, t)| *u != url || t.elapsed() > Duration::from_secs(300));
            if due {
                match omsi_net::official::announce(&url, &key) {
                    Ok(()) => println!("announced {url}"),
                    Err(e) => eprintln!("not announced: {e}"),
                }
                last = Some((url, Instant::now()));
            }
        }
        std::thread::sleep(Duration::from_secs(10));
    }
}
