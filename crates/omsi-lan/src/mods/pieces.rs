//! Players' buses in pieces: every file goes as pieces of `PIECE` bytes (a small file is one
//! piece), each compressed when that makes it smaller, several connections at once. A piece
//! is kept (sealed, `store`) the moment it comes, so a transfer that breaks off goes on where
//! it stopped - whatever the size of the file it was in - and loses at most the pieces on
//! their way. A file is checked once all its pieces are here: its SHA-256, then what it is
//! (`quarantine`); only then is it kept whole and its pieces go.
//!
//! On the mods connection (beside `BLOB`/`PUT`, which older games use):
//!
//!   ZSTD                                    → OK (pieces may come as `s` from now on)
//!   PIECE <sha> <i>                         → OK <raw> <codec> <wire> + wire bytes
//!   OFFER2 <len> + JSON VehicleManifest     → OK <len> + JSON [[index, [piece, ...]], ...]
//!   PUTPIECE <index> <i> <raw> <codec> <wire> + wire bytes → OK | ERR why
//!
//! `codec` is `z` (zlib), `s` (zstd: only to a game that said `ZSTD` - an older one does not
//! know it, an older host answers `ERR what`) or `-` (as it is: a picture that is compressed
//! already). A piece never comes bigger than it is, nor unpacks to more than it should (a
//! bomb ends there). The bytes a piece unpacks to are the file's own: how it went never
//! touches its SHA-256, nor the fingerprint of its bus.

use std::collections::VecDeque;
use std::io::{BufRead, Read, Write};
use std::sync::Mutex;

use super::failure::Failure;

/// The size of a piece.
pub(crate) const PIECE: u64 = 1 << 20;

/// How many connections a bus goes over at once.
pub(crate) const CONNECTIONS: usize = 4;

/// How many pieces one connection asks for before it reads the answers.
const BATCH: usize = 8;

/// How many pieces a file of `size` bytes has (an empty file: one of none).
pub(crate) fn count(size: u64) -> u64 {
    size.div_ceil(PIECE).max(1)
}

/// Where piece `i` of a file of `size` bytes is: (start, length).
pub(crate) fn span(size: u64, i: u64) -> (u64, u64) {
    let start = i * PIECE;
    (start, size.saturating_sub(start).min(PIECE))
}

/// A piece as it goes: (codec, bytes) - compressed when that is smaller.
pub(crate) fn pack(data: &[u8]) -> (char, Vec<u8>) {
    let mut z = flate2::write::ZlibEncoder::new(Vec::with_capacity(data.len() / 2), flate2::Compression::fast());
    if z.write_all(data).is_ok() {
        if let Ok(packed) = z.finish() {
            if packed.len() < data.len() {
                return ('z', packed);
            }
        }
    }
    ('-', data.to_vec())
}

/// zstd at the speed of `pack` (a piece packed the moment it is asked for)...
pub(crate) const ZSTD_FAST: i32 = 3;
/// ...and at its smallest, once and for all on a host (`vehicles::recompress`): a fifth less
/// than `pack` over a whole bus, at a megabyte or two a second.
pub(crate) const ZSTD_BEST: i32 = 19;

/// A piece as it goes with zstd at `level`: (codec, bytes) - as it is when that is smaller.
pub(crate) fn pack_zstd(data: &[u8], level: i32) -> (char, Vec<u8>) {
    match zstd::bulk::compress(data, level) {
        Ok(packed) if packed.len() < data.len() => ('s', packed),
        _ => ('-', data.to_vec()),
    }
}

/// A piece as it came, back to its `raw` bytes (never more: a bomb is refused).
pub(crate) fn unpack(codec: &str, wire: Vec<u8>, raw: u64) -> Result<Vec<u8>, String> {
    let data = match codec {
        "-" => wire,
        "z" => {
            let mut out = Vec::with_capacity(raw as usize);
            flate2::read::ZlibDecoder::new(wire.as_slice()).take(raw + 1).read_to_end(&mut out).map_err(|e| format!("a piece does not unpack: {e}"))?;
            out
        }
        // (into room for `raw` bytes and no more: a piece that would unpack to more fails)
        "s" => zstd::bulk::decompress(&wire, raw as usize).map_err(|e| format!("a piece does not unpack: {e}"))?,
        _ => return Err(format!("a piece in an unknown form ({codec})")),
    };
    if data.len() as u64 != raw {
        return Err(format!("a piece of {} bytes, {raw} said", data.len()));
    }
    Ok(data)
}

/// Answer a `PIECE` request with `data`.
pub(crate) fn send(out: &mut impl Write, data: &[u8]) -> std::io::Result<()> {
    let (codec, wire) = pack(data);
    send_packed(out, data.len() as u64, codec, &wire)
}

/// Answer a `PIECE` request with a piece of `raw` bytes packed already.
pub(crate) fn send_packed(out: &mut impl Write, raw: u64, codec: char, wire: &[u8]) -> std::io::Result<()> {
    writeln!(out, "OK {raw} {codec} {}", wire.len())?;
    out.write_all(wire)
}

/// Read the answer to a `PIECE` request for a piece of `raw` bytes: the piece, and how many
/// bytes it took on the wire.
pub(crate) fn receive(input: &mut impl BufRead, raw: u64) -> Result<(Vec<u8>, u64), Failure> {
    let mut line = String::new();
    // (a header is a few words: a longer line from the host ends the transfer)
    if super::read_line_within(input, &mut line, super::MAX_REPLY).map_err(|e| Failure::ConnectionLost(e.to_string()))? == 0 {
        return Err(Failure::ConnectionLost("closed by the host".into()));
    }
    let t = line.trim();
    let Some(rest) = t.strip_prefix("OK ") else {
        return Err(Failure::HostSays(t.strip_prefix("ERR").unwrap_or(t).trim().to_string()));
    };
    let (said, codec, wire) = head(rest).ok_or_else(|| Failure::Other(format!("bad reply {t:?}")))?;
    if said != raw {
        return Err(Failure::Other(format!("a piece of {said} bytes, {raw} asked for")));
    }
    read_piece(input, codec, raw, wire).map(|d| (d, wire))
}

/// `<raw> <codec> <wire>` of a piece's header.
pub(crate) fn head(s: &str) -> Option<(u64, &str, u64)> {
    let mut it = s.split_whitespace();
    let raw = it.next()?.parse().ok()?;
    let codec = it.next()?;
    let wire = it.next()?.parse().ok()?;
    it.next().is_none().then_some((raw, codec, wire))
}

/// The `wire` bytes of a piece of `raw` bytes, read and unpacked.
pub(crate) fn read_piece(input: &mut impl Read, codec: &str, raw: u64, wire: u64) -> Result<Vec<u8>, Failure> {
    // (a piece never goes bigger than it is: anything more is no piece of ours)
    if raw > PIECE || wire > raw.max(64) {
        return Err(Failure::Other(format!("a piece of {wire} bytes for {raw}")));
    }
    let mut bytes = vec![0u8; wire as usize];
    input.read_exact(&mut bytes).map_err(|e| Failure::ConnectionLost(e.to_string()))?;
    unpack(codec, bytes, raw).map_err(Failure::Other)
}

/// Work `items` off over up to `connections` workers at once: each takes a few items at a
/// time from the common queue until none is left (so a big file is shared out, piece by
/// piece). The first failure is the answer; the others' work is kept all the same.
pub(crate) fn share_out<T: Send, E: Send + From<&'static str>>(items: Vec<T>, connections: usize, worker: impl Fn(&dyn Fn() -> Vec<T>) -> Result<(), E> + Sync) -> Result<(), E> {
    if items.is_empty() {
        return Ok(());
    }
    let n = connections.clamp(1, items.len());
    let queue = Mutex::new(VecDeque::from(items));
    let take = || {
        let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
        let k = BATCH.min(q.len());
        q.drain(..k).collect::<Vec<T>>()
    };
    let results: Vec<Result<(), E>> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..n).map(|_| s.spawn(|| worker(&take))).collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("a transfer stopped".into()))).collect()
    });
    results.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_piece_goes_packed_when_that_is_smaller_and_comes_back_as_it_was() {
        let flat = vec![7u8; PIECE as usize];
        let (codec, wire) = pack(&flat);
        assert_eq!(codec, 'z');
        assert!(wire.len() < flat.len() / 50);
        assert_eq!(unpack("z", wire, PIECE).unwrap(), flat);
        // (noise does not get smaller: it goes as it is)
        let mut x = 0x1234_5678u32;
        let noise: Vec<u8> = (0..70_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect();
        let (codec, wire) = pack(&noise);
        assert_eq!((codec, wire.len()), ('-', noise.len()));
        // over the wire, with its header
        let mut buf = Vec::new();
        send(&mut buf, &flat).unwrap();
        assert_eq!(receive(&mut buf.as_slice(), PIECE).unwrap().0, flat);
    }

    #[test]
    fn a_piece_that_unpacks_to_more_or_comes_bigger_than_it_is_is_refused() {
        // a bomb: a small piece that unpacks to far more than it says
        let (_, bomb) = pack(&vec![0u8; 64 << 20]);
        assert!(unpack("z", bomb.clone(), 1000).is_err());
        let mut buf = format!("OK 1000 z {}\n", bomb.len()).into_bytes();
        buf.extend_from_slice(&bomb);
        assert!(receive(&mut buf.as_slice(), 1000).is_err());
        // more bytes on the wire than the piece has
        let mut big = b"OK 10 - 5000\n".to_vec();
        big.extend(vec![1u8; 5000]);
        assert!(receive(&mut big.as_slice(), 10).is_err());
        // a piece larger than a piece
        assert!(receive(&mut format!("OK {} - {}\n", PIECE + 1, PIECE + 1).as_bytes(), PIECE + 1).is_err());
        assert!(unpack("?", vec![1], 1).is_err());
        assert!(receive(&mut b"ERR no such file\n".as_slice(), 10).unwrap_err().host_says("no such file"));
        // the same with zstd: one that unpacks to more than it says, or to less
        let (codec, bomb) = pack_zstd(&vec![0u8; 64 << 20], ZSTD_FAST);
        assert_eq!(codec, 's');
        assert!(unpack("s", bomb.clone(), 1000).is_err());
        let mut buf = format!("OK 1000 s {}\n", bomb.len()).into_bytes();
        buf.extend_from_slice(&bomb);
        assert!(receive(&mut buf.as_slice(), 1000).is_err());
        let (_, small) = pack_zstd(&[5u8; 500], ZSTD_FAST);
        assert!(unpack("s", small, 1000).is_err());
        assert!(unpack("s", b"no zstd at all".to_vec(), 14).is_err());
    }

    #[test]
    fn a_piece_packed_with_zstd_comes_back_byte_for_byte() {
        // (something like a texture: runs and noise)
        let mut x = 0x2468_ace1u32;
        let tex: Vec<u8> = (0..PIECE as usize)
            .map(|i| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                if i % 4096 < 3000 { (i / 4096) as u8 } else { x as u8 }
            })
            .collect();
        let (_, zlib) = pack(&tex);
        let (codec, fast) = pack_zstd(&tex, ZSTD_FAST);
        let (_, best) = pack_zstd(&tex, ZSTD_BEST);
        assert_eq!(codec, 's');
        // (the slowest is not smaller every time: `vehicles::recompress` keeps the smaller)
        assert!(fast.len() < zlib.len(), "zlib {} fast {}", zlib.len(), fast.len());
        for wire in [fast, best] {
            let mut buf = Vec::new();
            send_packed(&mut buf, PIECE, 's', &wire).unwrap();
            assert_eq!(receive(&mut buf.as_slice(), PIECE).unwrap().0, tex);
        }
        // what does not get smaller goes as it is
        let noise: Vec<u8> = (0..70_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect();
        assert_eq!(pack_zstd(&noise, ZSTD_BEST), ('-', noise));
    }

    #[test]
    fn pieces_cover_a_file_whatever_its_size() {
        assert_eq!(count(0), 1);
        assert_eq!(span(0, 0), (0, 0));
        assert_eq!(count(1), 1);
        assert_eq!(count(PIECE), 1);
        assert_eq!(count(PIECE + 1), 2);
        assert_eq!(span(PIECE + 1, 1), (PIECE, 1));
        assert_eq!(span(3 * PIECE, 2), (2 * PIECE, PIECE));
    }

    #[test]
    fn work_is_shared_out_and_every_item_done_once() {
        let done = Mutex::new(Vec::new());
        share_out((0..100).collect(), 4, |take| {
            loop {
                let batch = take();
                if batch.is_empty() {
                    return Ok(());
                }
                done.lock().unwrap().extend(batch);
            }
        })
        .map_err(|e: String| e)
        .unwrap();
        let mut d = done.into_inner().unwrap();
        d.sort();
        assert_eq!(d, (0..100).collect::<Vec<_>>());
        // a worker that fails: the answer is its failure
        assert!(share_out(vec![1, 2, 3], 2, |_| Err(String::from("broke"))).unwrap_err().contains("broke"));
    }
}
