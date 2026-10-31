//! The game's end of `omsi-plugin-host`: a 32-bit plugin library run in a process of its
//! own, a frame one round trip over its standard input and output.

use crate::{wire, Frame, Procs, Reply};
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A plugin library in `omsi-plugin-host`.
pub struct Remote {
    child: Child,
    /// The library's name, for the log.
    name: String,
    /// The frames for the host, and its answers: the round trips run on a thread of their
    /// own (`exchange`), so that the game can stop waiting for one.
    asks: mpsc::Sender<Ask>,
    answers: mpsc::Receiver<Answer>,
    /// Since when the host has owed the answer of a frame the game went on without.
    owed: Option<Instant>,
    /// It was late before: its frames are asked with `SLOW_WAIT` only, until one answers in it.
    slow: bool,
    procs: Procs,
}

enum Ask {
    Frame(Frame),
    Finalize,
}

enum Answer {
    Frame(std::io::Result<Reply>),
    Finalized,
}

/// How long a frame waits for the host's answer before the game goes on without it. The
/// plugins seen answer within a millisecond or two; one that takes longer is busy in its own
/// code, and the game is not held for it (its answer is taken when it comes, see
/// `Plugin::frame`): SuperRadio, asked to switch the bus radio on, connects to
/// the station and then waits in the call for something that never comes in the host, and
/// the game stood still with it - "the frame's stage 'integrations' has been running for
/// 71 s" - until it was killed.
const ANSWER_WAIT: Duration = Duration::from_millis(500);

/// How long a plugin may owe an answer before it is given up: its host is ended and the
/// plugin left out for the rest of the session (the bus radio still plays: openOMSI's own
/// radio has the stations of SuperRadio and the like, `omsi_soundscape::radio`).
const GIVE_UP: Duration = Duration::from_secs(20);

/// How long a frame waits for a plugin that was late before. One slow on every call (not
/// stuck: half a second a frame, say) made the game wait `ANSWER_WAIT` again each time its
/// late answer had come and the next frame was asked - 2 frames a second for as long as it
/// ran; now it runs at its own pace beside the game, a frame behind, and waits its full time
/// again once it answers within this.
const SLOW_WAIT: Duration = Duration::from_millis(4);

impl Remote {
    /// Start `host` (with `runner`, e.g. `wine`, in front when given) on the library in
    /// `dir` and have it call `PluginStart`.
    pub fn spawn(runner: Option<&Path>, host: &Path, dll: &Path, dir: &Path) -> Result<Remote, String> {
        let mut cmd = match runner {
            Some(r) => {
                let mut c = Command::new(r);
                c.arg(host);
                c
            }
            None => Command::new(host),
        };
        cmd.arg(dll).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).current_dir(dir);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("could not start {}: {e}", host.display()))?;
        if let Some(err) = child.stderr.take() {
            std::thread::spawn(move || {
                for line in std::io::BufRead::lines(BufReader::new(err)).map_while(Result::ok) {
                    log::warn!("plugin host: {line}");
                }
            });
        }
        let mut to = BufWriter::new(child.stdin.take().ok_or("no stdin")?);
        let mut from = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        let answer = (|| -> std::io::Result<(u8, u8)> {
            wire::put_u8(&mut to, wire::START)?;
            to.flush()?;
            Ok((wire::get_u8(&mut from)?, wire::get_u8(&mut from)?))
        })();
        let procs = match answer {
            Ok((1, flags)) => Procs::from_flags(flags),
            Ok(_) => {
                let _ = child.kill();
                return Err(format!("the host could not load {}", dll.display()));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("the host for {} ended: {e}", dll.display()));
            }
        };
        let (asks, asked) = mpsc::channel();
        let (answered, answers) = mpsc::channel();
        let spawned = std::thread::Builder::new().name("plugin host".into()).spawn(move || exchange(to, from, procs, asked, answered));
        if let Err(e) = spawned {
            let _ = child.kill();
            return Err(format!("no thread for the host of {}: {e}", dll.display()));
        }
        let name = dll.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Remote { child, name, asks, answers, owed: None, slow: false, procs })
    }

    pub fn procs(&self) -> Procs {
        self.procs
    }

    /// One frame of the plugin: its answer, or None when it did not answer within
    /// `ANSWER_WAIT` (`SLOW_WAIT` once it was late) - the game goes on, and the answer is fetched later with
    /// [`Remote::late`] (no frame is asked before it has come). An error when the host
    /// ended.
    pub fn frame(&mut self, f: &Frame) -> std::io::Result<Option<Reply>> {
        debug_assert!(self.owed.is_none(), "a frame asked while the last one's answer is owed");
        self.asks.send(Ask::Frame(f.clone())).map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        match self.answers.recv_timeout(if self.slow { SLOW_WAIT } else { ANSWER_WAIT }) {
            Ok(Answer::Frame(r)) => {
                self.slow = false;
                r.map(Some)
            }
            Ok(Answer::Finalized) => Err(std::io::ErrorKind::BrokenPipe.into()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !self.slow {
                    log::warn!("plugin {}: no answer within {} ms - the game goes on without it, at its own pace", self.name, ANSWER_WAIT.as_millis());
                }
                self.slow = true;
                self.owed = Some(Instant::now());
                Ok(None)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(std::io::ErrorKind::BrokenPipe.into()),
        }
    }

    /// The answer of the frame that did not answer in time, once it has come (None while
    /// it is still owed, or when nothing is). An error when the host ended, or owed it
    /// longer than `GIVE_UP` (the host is ended then).
    pub fn late(&mut self) -> std::io::Result<Option<Reply>> {
        let Some(since) = self.owed else { return Ok(None) };
        match self.answers.try_recv() {
            Ok(Answer::Frame(r)) => {
                log::info!("plugin {}: answers again after {:.1} s", self.name, since.elapsed().as_secs_f32());
                self.owed = None;
                r.map(Some)
            }
            Ok(Answer::Finalized) | Err(mpsc::TryRecvError::Disconnected) => Err(std::io::ErrorKind::BrokenPipe.into()),
            Err(mpsc::TryRecvError::Empty) if since.elapsed() < GIVE_UP => Ok(None),
            Err(mpsc::TryRecvError::Empty) => {
                let _ = self.child.kill();
                Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("no answer for {} s, stuck in its own code", GIVE_UP.as_secs())))
            }
        }
    }

    /// Whether the host owes the answer of a frame.
    pub fn owes(&self) -> bool {
        self.owed.is_some()
    }

    /// `PluginFinalize`, and the host's end. A host that owes an answer is ended at once
    /// (it is stuck, and would hold the game's end as it held its frames), one that does not
    /// finish soon after it is ended too.
    pub fn finalize(&mut self) {
        if self.owed.is_none() && self.asks.send(Ask::Finalize).is_ok() {
            let _ = self.answers.recv_timeout(Duration::from_secs(2));
            let until = Instant::now() + Duration::from_secs(2);
            while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The game's side of the host's pipes: one round trip for each frame asked, until the game
/// finalizes the plugin or goes, or the host ends.
fn exchange(mut to: BufWriter<ChildStdin>, mut from: BufReader<ChildStdout>, procs: Procs, asks: mpsc::Receiver<Ask>, answers: mpsc::Sender<Answer>) {
    for ask in asks {
        match ask {
            Ask::Frame(f) => {
                let reply = (|| -> std::io::Result<Reply> {
                    wire::put_u8(&mut to, wire::FRAME)?;
                    wire::put_frame(&mut to, &f, procs.texture, procs.pointer)?;
                    if procs.ticket {
                        wire::put_tickets(&mut to, &f.tickets)?;
                    }
                    to.flush()?;
                    let mut reply = wire::get_reply(&mut from, &f, procs.texture)?;
                    if procs.ticket {
                        reply.tickets = wire::get_ticket_answers(&mut from, f.tickets.len())?;
                    }
                    Ok(reply)
                })();
                let ended = reply.is_err();
                if answers.send(Answer::Frame(reply)).is_err() || ended {
                    return;
                }
            }
            Ask::Finalize => {
                let _ = wire::put_u8(&mut to, wire::FINALIZE).and_then(|_| to.flush());
                let _ = wire::get_u8(&mut from);
                let _ = answers.send(Answer::Finalized);
                return;
            }
        }
    }
}
