//! The pictures a player's scripts draw on their bus's displays, for the copies the others
//! draw (`DISP`).
//!
//! A matrix display is drawn by the bus's scripts into a `[scripttexture]`. The copy of the
//! bus the others draw runs the same scripts with the sender's variables pinned (`vars.rs`),
//! but what it draws is its own: its scrolling runs on its own clock, a font or a picture
//! file it has not got stays blank, and a bus of other files (another version) draws what
//! its own scripts make of the variables. This sends the picture itself: every fifth of a
//! second the textures that changed since they were last sent, and each of them in turn
//! every few seconds (a lost datagram is mended, a player who joins late sees them). A
//! picture goes as its pixels over a palette of up to 255 colours (a matrix's amber on
//! black), or as they are when there are more, deflated, and cut into datagrams.
//!
//! ```text
//! byte 0      0xB6 (no text message starts with it)
//! byte 1      protocol
//! bytes 2-5   sender's id (u32 LE)
//! byte 6      texture index (the vehicle's [scripttexture] entries in order)
//! bytes 7-8   width, 9-10 height (u16 LE)
//! bytes 11-12 picture number (u16 LE, wrapping): the chunks of one picture share it
//! byte 13     chunk index, byte 14 the number of chunks
//! then        the chunk's bytes of the deflated picture
//! ```
//! The deflated picture: byte 0 is 0 for a palette (byte 1 its size, then that many RGBA
//! colours, then width x height indices into it) or 1 for the raw RGBA pixels.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

/// The first byte of a `DISP` datagram.
pub const DISP_MAGIC: u8 = 0xB6;
const HEADER: usize = 15;
/// What one datagram holds at most (within `MAX_DATAGRAM`, with room to spare).
const ROOM: usize = 1300;
/// A picture of more chunks than this is not sent (a photo on a display).
const MAX_CHUNKS: usize = 16;
/// How often changed pictures go out (s), and how often one of the unchanged goes again.
const EVERY: f32 = 0.2;
const KEY_EVERY: f32 = 3.0;
/// Datagrams a tick sends at most.
const MAX_PER_TICK: usize = 8;
/// A texture bigger than this is not sent.
const MAX_PIXELS: u32 = 512 * 512;
/// A picture whose chunks have not all come in this long is forgotten.
const STALE: Duration = Duration::from_secs(3);

/// A picture of a player's display as it came (`LanSession::take_displays`).
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayIn {
    pub id: u32,
    pub tex: usize,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Our side: the pictures as they were last sent.
#[derive(Debug, Default)]
pub struct DisplaySender {
    last: Vec<Vec<u8>>,
    seq: u16,
    acc: f32,
    key_acc: f32,
    key_at: usize,
}

impl DisplaySender {
    /// Our vehicle's script textures (`textures[k]`: width, height, RGBA of the k-th) for
    /// this frame: the datagrams due now (none most frames).
    pub fn tick(&mut self, protocol: u8, id: u32, textures: &[(u32, u32, &[u8])], dt: f32) -> Vec<Vec<u8>> {
        if self.last.len() != textures.len() {
            // another vehicle: everything is new
            self.last = vec![Vec::new(); textures.len()];
            self.acc = EVERY;
        }
        self.acc += dt;
        self.key_acc += dt;
        if self.acc < EVERY || textures.is_empty() {
            return Vec::new();
        }
        self.acc = 0.0;
        let mut key = None;
        if self.key_acc >= KEY_EVERY {
            self.key_acc = 0.0;
            key = Some(self.key_at % textures.len());
            self.key_at = self.key_at.wrapping_add(1);
        }
        let mut out = Vec::new();
        for (k, &(w, h, rgba)) in textures.iter().enumerate().take(256) {
            let fresh = rgba != self.last[k].as_slice();
            if !fresh && key != Some(k) {
                continue;
            }
            if out.len() >= MAX_PER_TICK {
                break;
            }
            // (what cannot go is remembered as sent: not encoded again five times a second)
            self.last[k] = rgba.to_vec();
            let Some(enc) = encode_picture(w, h, rgba) else { continue };
            let chunks = enc.chunks(ROOM).count();
            if chunks > MAX_CHUNKS {
                continue;
            }
            self.seq = self.seq.wrapping_add(1);
            for (c, part) in enc.chunks(ROOM).enumerate() {
                let mut d = Vec::with_capacity(HEADER + part.len());
                d.push(DISP_MAGIC);
                d.push(protocol);
                d.extend_from_slice(&id.to_le_bytes());
                d.push(k as u8);
                d.extend_from_slice(&(w as u16).to_le_bytes());
                d.extend_from_slice(&(h as u16).to_le_bytes());
                d.extend_from_slice(&self.seq.to_le_bytes());
                d.push(c as u8);
                d.push(chunks as u8);
                d.extend_from_slice(part);
                out.push(d);
            }
        }
        out
    }
}

/// A picture deflated as it goes (None: not a picture that can go).
fn encode_picture(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || w > u16::MAX as u32 || h > u16::MAX as u32 || w * h > MAX_PIXELS {
        return None;
    }
    let n = (w * h * 4) as usize;
    let rgba = rgba.get(..n)?;
    let mut body = Vec::with_capacity(n / 4 + 1024);
    let mut palette: HashMap<[u8; 4], u8> = HashMap::new();
    let mut colours: Vec<u8> = Vec::new();
    let mut indices: Vec<u8> = Vec::with_capacity(n / 4);
    let mut fits = true;
    for px in rgba.chunks_exact(4) {
        let c = [px[0], px[1], px[2], px[3]];
        let i = match palette.get(&c) {
            Some(i) => *i,
            None => {
                if palette.len() >= 255 {
                    fits = false;
                    break;
                }
                let i = palette.len() as u8;
                palette.insert(c, i);
                colours.extend_from_slice(&c);
                i
            }
        };
        indices.push(i);
    }
    if fits {
        body.push(0);
        body.push(palette.len() as u8);
        body.extend_from_slice(&colours);
        body.extend_from_slice(&indices);
    } else {
        body.push(1);
        body.extend_from_slice(rgba);
    }
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(6));
    enc.write_all(&body).ok()?;
    enc.finish().ok()
}

/// The pixels of a deflated picture (None: not one of `w` x `h`).
fn decode_picture(w: u32, h: u32, data: &[u8]) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || w * h > MAX_PIXELS {
        return None;
    }
    let n = (w * h) as usize;
    let mut body = Vec::with_capacity(n * 4 + 1026);
    flate2::read::DeflateDecoder::new(data).take((n * 4 + 1026) as u64).read_to_end(&mut body).ok()?;
    match *body.first()? {
        0 => {
            let count = *body.get(1)? as usize;
            let colours = body.get(2..2 + count * 4)?;
            let indices = body.get(2 + count * 4..2 + count * 4 + n)?;
            let mut rgba = Vec::with_capacity(n * 4);
            for &i in indices {
                rgba.extend_from_slice(colours.get(i as usize * 4..i as usize * 4 + 4)?);
            }
            Some(rgba)
        }
        1 => Some(body.get(1..1 + n * 4)?.to_vec()),
        _ => None,
    }
}

/// The sender's id of a `DISP` datagram (None: not one, or another protocol).
pub fn sender_of(data: &[u8], protocol: u8) -> Option<u32> {
    if data.len() < HEADER || data[0] != DISP_MAGIC || data[1] != protocol {
        return None;
    }
    Some(u32::from_le_bytes(data[2..6].try_into().ok()?))
}

struct Partial {
    at: Instant,
    width: u32,
    height: u32,
    chunks: Vec<Option<Vec<u8>>>,
}

/// The other side: the chunks of the pictures on their way in.
#[derive(Default)]
pub struct DisplayAssembler {
    parts: HashMap<(u32, u8, u16), Partial>,
}

impl DisplayAssembler {
    /// A `DISP` datagram taken in: the picture it completes, if any.
    pub fn take(&mut self, data: &[u8], protocol: u8) -> Option<DisplayIn> {
        let id = sender_of(data, protocol)?;
        let tex = data[6];
        let width = u16::from_le_bytes([data[7], data[8]]) as u32;
        let height = u16::from_le_bytes([data[9], data[10]]) as u32;
        let seq = u16::from_le_bytes([data[11], data[12]]);
        let (chunk, chunks) = (data[13] as usize, data[14] as usize);
        if chunks == 0 || chunks > MAX_CHUNKS || chunk >= chunks || data.len() - HEADER > ROOM {
            return None;
        }
        let now = Instant::now();
        self.parts.retain(|_, p| now.duration_since(p.at) < STALE);
        let key = (id, tex, seq);
        if !self.parts.contains_key(&key) && self.parts.len() >= 64 {
            return None;
        }
        let p = self.parts.entry(key).or_insert_with(|| Partial { at: now, width, height, chunks: vec![None; chunks] });
        if p.chunks.len() != chunks || (p.width, p.height) != (width, height) {
            return None;
        }
        p.chunks[chunk] = Some(data[HEADER..].to_vec());
        if p.chunks.iter().any(Option::is_none) {
            return None;
        }
        let p = self.parts.remove(&key)?;
        let bytes: Vec<u8> = p.chunks.into_iter().flatten().flatten().collect();
        let rgba = decode_picture(width, height, &bytes)?;
        // (the older pictures of this texture still on their way are not wanted any more)
        self.parts.retain(|k, _| !(k.0 == id && k.1 == tex));
        Some(DisplayIn { id, tex: tex as usize, width, height, rgba })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A matrix display: 256 x 64 pixels, amber letters on black.
    fn matrix(phase: u32) -> Vec<u8> {
        let mut rgba = vec![0u8; 256 * 64 * 4];
        for y in 0..64u32 {
            for x in 0..256u32 {
                if (x + phase) % 7 < 3 && y % 5 < 3 && (x / 20 + y / 10) % 2 == 0 {
                    let i = ((y * 256 + x) * 4) as usize;
                    rgba[i..i + 4].copy_from_slice(&[255, 160, 20, 255]);
                }
            }
        }
        rgba
    }

    /// A picture goes in a datagram or two, the changed one at once, an unchanged one again
    /// in its turn - and comes out as it went.
    #[test]
    fn a_display_picture_arrives_as_it_was_drawn() {
        let mut tx = DisplaySender::default();
        let mut rx = DisplayAssembler::default();
        let pic = matrix(0);
        let msgs = tx.tick(6, 7, &[(256, 64, &pic)], 0.1);
        assert!(!msgs.is_empty() && msgs.len() <= 2, "{} datagrams for a two-colour matrix", msgs.len());
        assert!(msgs.iter().all(|m| m.len() <= crate::MAX_DATAGRAM));
        let got: Vec<DisplayIn> = msgs.iter().filter_map(|m| rx.take(m, 6)).collect();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].id, got[0].tex, got[0].width, got[0].height), (7, 0, 256, 64));
        assert_eq!(got[0].rgba, pic);
        // nothing new: nothing goes until the key frame is due
        assert!(tx.tick(6, 7, &[(256, 64, &pic)], 0.25).is_empty());
        let pic2 = matrix(3);
        let msgs = tx.tick(6, 7, &[(256, 64, &pic2)], 0.25);
        let got: Vec<DisplayIn> = msgs.iter().filter_map(|m| rx.take(m, 6)).collect();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].rgba, pic2);
        // the key frame: the unchanged picture once more within a few seconds
        let mut again = 0;
        for _ in 0..20 {
            again += tx.tick(6, 7, &[(256, 64, &pic2)], 0.25).iter().filter_map(|m| rx.take(m, 6)).count();
        }
        assert_eq!(again, 1);
    }

    /// A picture of many colours (a photo on the display) goes raw; a lost chunk leaves
    /// nothing behind, and a foreign datagram is not taken.
    #[test]
    fn many_colours_go_raw_and_a_lost_chunk_is_forgotten() {
        // (noise: deflate cannot shrink it, so it takes several datagrams)
        let mut x = 0x9E37_79B9u32;
        let pic: Vec<u8> = (0..64 * 32 * 4)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })
            .collect();
        let mut tx = DisplaySender::default();
        let mut rx = DisplayAssembler::default();
        let msgs = tx.tick(6, 2, &[(64, 32, &pic)], 0.3);
        assert!(msgs.len() >= 2, "{}", msgs.len());
        // without the last chunk: nothing
        for m in &msgs[..msgs.len() - 1] {
            assert!(rx.take(m, 6).is_none());
        }
        assert!(rx.take(&msgs[msgs.len() - 1], 5).is_none(), "another protocol");
        assert!(rx.take(&msgs[msgs.len() - 1][..HEADER - 1], 6).is_none(), "cut short");
        assert!(rx.take(b"STATE|1", 6).is_none());
        let got = rx.take(&msgs[msgs.len() - 1], 6).unwrap();
        assert_eq!(got.rgba, pic);
        assert_eq!(sender_of(&msgs[0], 6), Some(2));
    }
}
