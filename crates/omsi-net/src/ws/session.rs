//! Session-code status queries shared by launcher frontends.

use super::ServerInfo;
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Query<T> = Box<dyn FnOnce() -> Result<T, String> + Send>;

/// Ask every address and the tunnel concurrently: a dead VPN must not hold up a LAN host.
/// The first valid reply wins; slow workers finish under their own HTTP timeouts.
pub fn query_session(code: &str) -> Result<ServerInfo, String> {
    let code = crate::SessionCode::decode(code)?;
    let mut jobs: Vec<Query<ServerInfo>> = code
        .addrs()
        .into_iter()
        .map(|a| {
            Box::new(move || {
                super::query(
                    &format!("http://{}:{}", a.ip(), a.port().saturating_add(10)),
                    false,
                )
            }) as Query<ServerInfo>
        })
        .collect();
    jobs.push(Box::new(move || {
        let url = crate::bridge::lookup_tunnel(code.session).ok_or("the host did not answer")?;
        super::query(&url, false)
    }));
    first_success(jobs, Duration::from_secs(30))
}

fn first_success<T: Send + 'static>(jobs: Vec<Query<T>>, timeout: Duration) -> Result<T, String> {
    let deadline = Instant::now() + timeout;
    let (tx, rx) = mpsc::channel();
    for job in jobs {
        let tx = tx.clone();
        let _ = std::thread::Builder::new()
            .name("session status".into())
            .spawn(move || {
                let _ = tx.send(job());
            });
    }
    drop(tx);
    let mut failure = None;
    loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(e)) => failure = Some(e),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(failure.unwrap_or_else(|| "the host did not answer".into()))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(format!(
                    "the host did not answer within {} seconds",
                    timeout.as_secs_f32()
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, TcpListener};
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn a_session_code_queries_its_actual_status_port() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 2048];
            let n = stream.read(&mut request).unwrap();
            assert!(request[..n].starts_with(b"GET /status "));
            let body = super::ServerInfo {
                name: "Local test host".into(),
                ..Default::default()
            }
            .to_json();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let code =
            crate::SessionCode::single(crate::PROTOCOL as u8, Ipv4Addr::LOCALHOST, port - 10, 42)
                .encode();
        let result = super::query_session(&code);
        worker.join().unwrap();
        let info = result.unwrap();
        assert_eq!(info.name, "Local test host");
        assert_eq!(info.reached_at, format!("http://127.0.0.1:{port}"));
    }

    #[test]
    fn an_invalid_code_is_rejected_before_starting_queries() {
        assert!(super::query_session("OMSI-INVALID").is_err());
    }

    #[test]
    fn a_responsive_address_does_not_wait_for_a_stalled_one() {
        let (release, blocked) = mpsc::channel();
        let (started, began) = mpsc::channel();
        let jobs: Vec<super::Query<&str>> = vec![
            Box::new(move || {
                started.send(()).unwrap();
                let _ = blocked.recv();
                Err("slow address".into())
            }),
            Box::new(move || {
                began.recv().unwrap();
                Ok("reachable host")
            }),
        ];
        let result = super::first_success(jobs, Duration::from_secs(1));
        let _ = release.send(());
        assert_eq!(result.as_deref(), Ok("reachable host"));
    }

    #[test]
    fn a_failed_address_does_not_hide_a_later_success() {
        let jobs: Vec<super::Query<&str>> = vec![
            Box::new(|| Err("unreachable".into())),
            Box::new(|| Ok("host")),
        ];
        assert_eq!(
            super::first_success(jobs, Duration::from_secs(1)).as_deref(),
            Ok("host")
        );
    }

    #[test]
    fn all_failures_are_reported() {
        let jobs: Vec<super::Query<&str>> = vec![Box::new(|| Err("unreachable".into()))];
        assert_eq!(
            super::first_success(jobs, Duration::from_secs(1)).unwrap_err(),
            "unreachable"
        );
    }

    #[test]
    fn a_stalled_query_has_an_overall_deadline() {
        let (release, blocked) = mpsc::channel();
        let jobs: Vec<super::Query<&str>> = vec![Box::new(move || {
            let _ = blocked.recv();
            Ok("too late")
        })];
        let result = super::first_success(jobs, Duration::from_millis(30));
        let _ = release.send(());
        assert_eq!(
            result.unwrap_err(),
            "the host did not answer within 0.03 seconds"
        );
    }
}
