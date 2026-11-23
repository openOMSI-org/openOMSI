//! The mods channel's encryption: every connection agrees on its own keys (ephemeral X25519,
//! HKDF-SHA256 over the shared secret, salted with the LAN session id and bound to both
//! public keys), and everything after the greeting goes in ChaCha20-Poly1305 frames - one
//! key per direction, the frame's number as its nonce, so a frame that was changed, dropped,
//! repeated or reordered ends the connection.
//!
//! The greeting is plain text: `OMSIMODS/3 <session> <client key>` and `OK <host key> <session key> <signature>`. A
//! host answers a game of an older protocol (`OMSIMODS/1`, `OMSIMODS/2`) with an `ERR` line
//! that says to update.

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
use ring::agreement::{agree_ephemeral, EphemeralPrivateKey, UnparsedPublicKey, X25519};
use ring::hkdf::{Salt, HKDF_SHA256};
use ring::rand::SystemRandom;
use std::io::{self, BufRead, Read, Write};

/// The protocol a game speaks now.
pub(crate) const MAGIC: &str = "OMSIMODS/3";
/// The unencrypted protocol of the games before it.
pub(crate) const OLD_MAGIC: &str = "OMSIMODS/1";
const OLD_MAGIC_2: &str = "OMSIMODS/2";

pub(crate) fn is_old(magic: &str) -> bool {
    magic == OLD_MAGIC || magic == OLD_MAGIC_2
}

fn proof_text(session: u64, client: &str, host: &str) -> Vec<u8> {
    format!("{MAGIC} {} {client} {host}", omsi_net::session_hex(session)).into_bytes()
}

pub(crate) fn prove(key: &omsi_net::hostkey::HostKey, client: &str, host: &str) -> String {
    format!("{} {}", hex(key.public()), hex(&key.sign("mods", &proof_text(key.session(), client, host))))
}

pub(crate) fn proven(session: u64, client: &str, host: &str, public: &str, sig: &str) -> bool {
    match (unhex(public), unhex(sig)) {
        (Some(p), Some(s)) => omsi_net::hostkey::verify(session, &p, "mods", &proof_text(session, client, host), &s),
        _ => false,
    }
}
/// What a host tells a game of the old protocol.
pub(crate) const OLD_PEER: &str = "ERR the host's openBus sends its mods encrypted (OMSIMODS/3): update openBus to join with them";

/// The most plain bytes one frame carries.
const FRAME: usize = 64 * 1024;
const TAG: usize = 16;

/// This side of a key agreement: its key pair, until the other side's public key is known.
pub(crate) struct Handshake {
    private: EphemeralPrivateKey,
    public: [u8; 32],
}

impl Handshake {
    pub(crate) fn new() -> io::Result<Handshake> {
        let rng = SystemRandom::new();
        let private = EphemeralPrivateKey::generate(&X25519, &rng).map_err(|_| io::Error::other("no key pair"))?;
        let mut public = [0u8; 32];
        public.copy_from_slice(private.compute_public_key().map_err(|_| io::Error::other("no public key"))?.as_ref());
        Ok(Handshake { private, public })
    }

    /// The public key, as the greeting spells it.
    pub(crate) fn public_hex(&self) -> String {
        hex(&self.public)
    }

    /// The keys of the connection: (ours for sending, ours for receiving). `client` says
    /// which side this is; `peer` is the other side's public key (hex).
    pub(crate) fn finish(self, session: u64, client: bool, peer: &str) -> io::Result<(LessSafeKey, LessSafeKey)> {
        let peer = unhex(peer).filter(|p| p.len() == 32).ok_or_else(|| bad("a bad key in the greeting"))?;
        let (client_pub, server_pub) = if client { (self.public.to_vec(), peer.clone()) } else { (peer.clone(), self.public.to_vec()) };
        let mut salt = b"openBus LAN mods 2 ".to_vec();
        salt.extend_from_slice(&session.to_be_bytes());
        let derive = |shared: &[u8]| -> Result<(LessSafeKey, LessSafeKey), ring::error::Unspecified> {
            let prk = Salt::new(HKDF_SHA256, &salt).extract(shared);
            let key = |dir: &[u8]| -> Result<LessSafeKey, ring::error::Unspecified> {
                let info = [dir, &client_pub, &server_pub];
                Ok(LessSafeKey::new(UnboundKey::from(prk.expand(&info, &CHACHA20_POLY1305)?)))
            };
            let (c2s, s2c) = (key(b"client to host")?, key(b"host to client")?);
            Ok(if client { (c2s, s2c) } else { (s2c, c2s) })
        };
        agree_ephemeral(self.private, &UnparsedPublicKey::new(&X25519, &peer), derive)
            .map_err(|_| bad("the key agreement failed"))?
            .map_err(|_| bad("the key agreement failed"))
    }
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn nonce(seq: u64) -> Nonce {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&seq.to_be_bytes());
    Nonce::assume_unique_for_key(n)
}

/// The sending half: what is written goes out in sealed frames when a frame is full and on
/// `flush`.
pub(crate) struct SecureWriter<W: Write> {
    inner: W,
    key: LessSafeKey,
    seq: u64,
    buf: Vec<u8>,
}

impl<W: Write> SecureWriter<W> {
    pub(crate) fn new(inner: W, key: LessSafeKey) -> Self {
        SecureWriter { inner, key, seq: 0, buf: Vec::with_capacity(FRAME + TAG) }
    }

    fn send(&mut self) -> io::Result<()> {
        if self.buf.is_empty() {
            return Ok(());
        }
        self.key.seal_in_place_append_tag(nonce(self.seq), Aad::empty(), &mut self.buf).map_err(|_| io::Error::other("cannot seal a frame"))?;
        self.seq += 1;
        self.inner.write_all(&(self.buf.len() as u32).to_be_bytes())?;
        self.inner.write_all(&self.buf)?;
        self.buf.clear();
        Ok(())
    }
}

impl<W: Write> Write for SecureWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let n = data.len().min(FRAME - self.buf.len());
        self.buf.extend_from_slice(&data[..n]);
        if self.buf.len() >= FRAME {
            self.send()?;
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.send()?;
        self.inner.flush()
    }
}

/// The receiving half: frames are opened as they are read (a changed one is an error).
pub(crate) struct SecureReader<R: Read> {
    inner: R,
    key: LessSafeKey,
    seq: u64,
    plain: Vec<u8>,
    pos: usize,
}

impl<R: Read> SecureReader<R> {
    pub(crate) fn new(inner: R, key: LessSafeKey) -> Self {
        SecureReader { inner, key, seq: 0, plain: Vec::new(), pos: 0 }
    }

    /// The next frame into `plain`; false at the end of the stream.
    fn next_frame(&mut self) -> io::Result<bool> {
        let mut len = [0u8; 4];
        let mut got = 0;
        while got < 4 {
            match self.inner.read(&mut len[got..]) {
                Ok(0) if got == 0 => return Ok(false),
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => got += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        let len = u32::from_be_bytes(len) as usize;
        if !(TAG..=FRAME + TAG).contains(&len) {
            return Err(bad("a frame of a bad size"));
        }
        self.plain.resize(len, 0);
        self.inner.read_exact(&mut self.plain)?;
        let n = self.key.open_in_place(nonce(self.seq), Aad::empty(), &mut self.plain).map_err(|_| bad("a frame was changed on the way (or another key)"))?.len();
        self.seq += 1;
        self.plain.truncate(n);
        self.pos = 0;
        Ok(true)
    }
}

impl<R: Read> BufRead for SecureReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        while self.pos >= self.plain.len() {
            if !self.next_frame()? {
                return Ok(&[]);
            }
        }
        Ok(&self.plain[self.pos..])
    }

    fn consume(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.plain.len());
    }
}

impl<R: Read> Read for SecureReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let avail = self.fill_buf()?;
        let n = avail.len().min(out.len());
        out[..n].copy_from_slice(&avail[..n]);
        self.consume(n);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> ((LessSafeKey, LessSafeKey), (LessSafeKey, LessSafeKey)) {
        let (c, s) = (Handshake::new().unwrap(), Handshake::new().unwrap());
        let (cp, sp) = (c.public_hex(), s.public_hex());
        (c.finish(42, true, &sp).unwrap(), s.finish(42, false, &cp).unwrap())
    }

    #[test]
    fn frames_round_trip_and_hide_the_bytes() {
        let ((c_send, _), (_, s_recv)) = pair();
        let mut w = SecureWriter::new(Vec::new(), c_send);
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        w.write_all(b"GET 1\nPLAINTEXT-MARKER\n").unwrap();
        w.write_all(&big).unwrap();
        w.flush().unwrap();
        let wire = w.inner.clone();
        assert!(!wire.windows(16).any(|x| x == b"PLAINTEXT-MARKER"));
        let mut r = SecureReader::new(&wire[..], s_recv);
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        assert_eq!(line, "GET 1\n");
        line.clear();
        r.read_line(&mut line).unwrap();
        let mut back = Vec::new();
        r.read_to_end(&mut back).unwrap();
        assert_eq!(back, big);
    }

    #[test]
    fn a_changed_or_reordered_frame_is_refused() {
        let ((c_send, _), (_, s_recv)) = pair();
        let mut w = SecureWriter::new(Vec::new(), c_send);
        w.write_all(b"LIST\n").unwrap();
        w.flush().unwrap();
        w.write_all(b"BYE\n").unwrap();
        w.flush().unwrap();
        let wire = w.inner.clone();
        let mut changed = wire.clone();
        changed[6] ^= 0x20;
        let mut line = String::new();
        assert!(SecureReader::new(&changed[..], s_recv).read_line(&mut line).is_err());
        // the second frame first
        let ((c_send, _), (_, s_recv)) = pair();
        let mut w = SecureWriter::new(Vec::new(), c_send);
        w.write_all(b"LIST\n").unwrap();
        w.flush().unwrap();
        let first = w.inner.len();
        w.write_all(b"BYE\n").unwrap();
        w.flush().unwrap();
        let swapped = [&w.inner[first..], &w.inner[..first]].concat();
        assert!(SecureReader::new(&swapped[..], s_recv).read_line(&mut line).is_err());
    }

    #[test]
    fn keys_belong_to_the_session() {
        let (c, s) = (Handshake::new().unwrap(), Handshake::new().unwrap());
        let (cp, sp) = (c.public_hex(), s.public_hex());
        let (c_send, _) = c.finish(1, true, &sp).unwrap();
        let (_, s_recv) = s.finish(2, false, &cp).unwrap();
        let mut w = SecureWriter::new(Vec::new(), c_send);
        w.write_all(b"LIST\n").unwrap();
        w.flush().unwrap();
        let mut line = String::new();
        assert!(SecureReader::new(&w.inner[..], s_recv).read_line(&mut line).is_err());
        assert!(Handshake::new().unwrap().finish(1, true, "zz").is_err());
    }
}
