//! A room owns its listener and every accepted file connection.

use super::*;
use std::net::Shutdown;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
struct State {
    stopped: AtomicBool,
    connections: Mutex<HashMap<SocketAddr, TcpStream>>,
}

/// Dropping the room's server closes the listener and interrupts existing transfers.
pub struct Server {
    state: Arc<State>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.state.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        for (_, stream) in self
            .state
            .connections
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
        {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}

/// Serve only while the caller retains the returned room-owned handle.
pub fn serve(
    listener: TcpListener,
    key: Arc<omsi_net::hostkey::HostKey>,
    args: &ModSource,
) -> Option<Server> {
    if omsi_cfg::flags::OMSI_NO_LAN_MODS.is_set() {
        return None;
    }
    let port = listener.local_addr().ok()?.port();
    if let Err(e) = listener.set_nonblocking(true) {
        log::warn!("LAN mods: cannot make the listener interruptible: {e}");
        return None;
    }
    let ready: Arc<Mutex<Option<Arc<(Manifest, Vec<PathBuf>)>>>> = Arc::new(Mutex::new(None));
    let state = Arc::new(State::default());
    upload::open();
    {
        let (ready, state, args) = (ready.clone(), state.clone(), args.clone());
        std::thread::Builder::new().name("lan-mods-list".into()).spawn(move || {
            let manifest = if SHARE_MAPS.load(Ordering::Relaxed) {
                collect(&args)
            } else {
                log::info!("LAN mods: the map is not passed on (share_maps = 0): players need {} installed", args.map);
                (Manifest { map: args.map.replace('\\', "/"), bus: String::new(), entries: Vec::new() }, Vec::new())
            };
            if !state.stopped.load(Ordering::Acquire) {
                *ready.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(manifest));
            }
        }).ok()?;
    }
    log::info!("LAN mods: serving this session's mods on TCP port {port}");
    let serving = state.clone();
    let worker = std::thread::Builder::new()
        .name("lan-mods".into())
        .spawn(move || {
            while !serving.stopped.load(Ordering::Acquire) {
                let stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(e) => {
                        log::warn!("LAN mods: accept failed: {e}");
                        break;
                    }
                };
                if serving.stopped.load(Ordering::Acquire) {
                    break;
                }
                let Ok(peer) = stream.peer_addr() else {
                    continue;
                };
                let Ok(socket) = stream.try_clone() else {
                    continue;
                };
                // Accepted sockets must remain blocking on platforms that inherit the flag.
                if stream.set_nonblocking(false).is_err() {
                    continue;
                }
                {
                    let mut connections = serving
                        .connections
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    if connections.len() >= MAX_CONNECTIONS {
                        log::info!(
                            "LAN mods: {MAX_CONNECTIONS} connections open already; try again later"
                        );
                        let mut stream = stream;
                        stream
                            .set_write_timeout(Some(Duration::from_millis(100)))
                            .ok();
                        let _ = stream.write_all(b"ERR busy\n");
                        continue;
                    }
                    connections.insert(peer, socket);
                }
                let (ready, key, state) = (ready.clone(), key.clone(), serving.clone());
                let spawned = std::thread::Builder::new()
                    .name("lan-mods-conn".into())
                    .spawn(move || {
                        if let Err(e) = handle(stream, &key, &ready, &state.stopped) {
                            log::info!("LAN mods: connection from {peer} ended: {e}");
                        }
                        state
                            .connections
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(&peer);
                    });
                if spawned.is_err() {
                    serving
                        .connections
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(&peer);
                }
            }
        })
        .ok()?;
    Some(Server {
        state,
        worker: Some(worker),
    })
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
