//! The requests of the players' buses on a mods connection (the map's own, `LIST` and `GET`,
//! are the host's): each read and checked once, here, into what its handler works with - so
//! no handler parses or checks its words again.
//!
//! What is asked wrongly is answered as it always was: a file or a bus the host cannot have
//! gets `ERR`, and the connection goes on; numbers that make no sense end the connection,
//! since a body of that size may follow and the stream can no longer be read in step.

use super::*;
use crate::mods::failure::says;

/// A SHA-256 as the requests name a kept file by: 64 hex digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mods) struct Sha256(String);

impl Sha256 {
    pub(in crate::mods) fn parse(s: &str) -> Option<Sha256> {
        (s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| Sha256(s.to_string()))
    }

    pub(in crate::mods) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bus's fingerprint as the requests name a kept bus by (`omsi_net::is_fingerprint`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mods) struct Fingerprint(String);

impl Fingerprint {
    pub(in crate::mods) fn parse(s: &str) -> Option<Fingerprint> {
        omsi_net::is_fingerprint(s).then(|| Fingerprint(s.to_string()))
    }

    pub(in crate::mods) fn as_str(&self) -> &str {
        &self.0
    }
}

/// `PUTPIECE <index> <piece> <raw> <codec> <wire>`: piece `piece` of the offer's file `index`,
/// `raw` bytes sent as `wire` bytes packed with `codec` (see `pieces`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mods) struct PutPiece {
    pub(in crate::mods) index: usize,
    pub(in crate::mods) piece: u64,
    pub(in crate::mods) raw: u64,
    pub(in crate::mods) codec: String,
    pub(in crate::mods) wire: u64,
}

/// A request of the players' buses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mods) enum Request {
    /// `OFFER <len>` and a list: which of its files the host lacks.
    Offer { len: u64 },
    /// `OFFER2 <len>` and a list: which pieces of them.
    OfferPieces { len: u64 },
    /// `PUT <index> <len>` and the file.
    Put { index: usize, len: u64 },
    /// `PUTPIECE ...` and the piece.
    PutPiece(PutPiece),
    /// `ZSTD`: pieces may go packed with zstd to this asker.
    Zstd,
    /// `PIECE <sha> <piece>`: one piece of a kept file.
    Piece { sha: Sha256, piece: u64 },
    /// `DONE`: the offered bus is all here.
    Done,
    /// `MANIFEST <fp>`: a kept bus's list.
    Manifest(Fingerprint),
    /// `BLOB <sha>`: a kept file, whole.
    Blob(Sha256),
}

/// A request asked wrongly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mods) enum Rejected {
    /// Answered `ERR <words>`; the connection goes on.
    Answer(&'static str),
    /// The connection ends: what follows the line cannot be read in step.
    End(&'static str),
}

/// The longest list an offer may come with.
const MAX_OFFER: u64 = 256 << 20;

impl Request {
    /// The request these words make, if they are one of the players' buses (None: the host's
    /// own, or a word it does not know).
    pub(in crate::mods) fn parse(words: &[&str]) -> Option<Result<Request, Rejected>> {
        use Rejected::{Answer, End};
        let offer = |len: &str| {
            len.parse::<u64>()
                .ok()
                .filter(|l| *l <= MAX_OFFER)
                .ok_or(End("bad offer"))
        };
        let r = match words {
            ["OFFER", len] => offer(len).map(|len| Request::Offer { len }),
            ["OFFER2", len] => offer(len).map(|len| Request::OfferPieces { len }),
            ["PUT", index, len] => match (index.parse(), len.parse()) {
                (Ok(index), Ok(len)) => Ok(Request::Put { index, len }),
                _ => Err(End("bad put")),
            },
            ["PUTPIECE", index, piece, raw, codec, wire] => {
                match (index.parse(), piece.parse(), raw.parse(), wire.parse()) {
                    (Ok(index), Ok(piece), Ok(raw), Ok(wire)) => Ok(Request::PutPiece(PutPiece {
                        index,
                        piece,
                        raw,
                        codec: codec.to_string(),
                        wire,
                    })),
                    _ => Err(End("bad piece")),
                }
            }
            ["ZSTD"] => Ok(Request::Zstd),
            ["PIECE", sha, piece] => match (Sha256::parse(sha), piece.parse()) {
                (Some(sha), Ok(piece)) => Ok(Request::Piece { sha, piece }),
                _ => Err(Answer(says::NO_SUCH_FILE)),
            },
            ["DONE"] => Ok(Request::Done),
            ["MANIFEST", fp] => Fingerprint::parse(fp)
                .map(Request::Manifest)
                .ok_or(Answer(says::NO_SUCH_BUS)),
            ["BLOB", sha] => Sha256::parse(sha)
                .map(Request::Blob)
                .ok_or(Answer(says::NO_SUCH_FILE)),
            _ => return None,
        };
        Some(r)
    }
}

/// The players' buses on one mods connection: the bus offered on it, and whether its pieces
/// may go packed with zstd.
#[derive(Default)]
pub(in crate::mods) struct BusRequests {
    incoming: Incoming,
    zstd: bool,
}

impl BusRequests {
    /// Answer one request (its body, when it has one, read from `input`).
    pub(in crate::mods) fn serve(
        &mut self,
        request: Request,
        input: &mut impl Read,
        out: &mut impl Write,
    ) -> std::io::Result<()> {
        match request {
            Request::Offer { len } => self.incoming.offer(input, out, len),
            Request::OfferPieces { len } => self.incoming.offer_pieces(input, out, len),
            Request::Put { index, len } => self.incoming.put(input, out, index, len),
            Request::PutPiece(p) => self.incoming.put_piece(input, out, &p),
            Request::Zstd => {
                self.zstd = true;
                writeln!(out, "OK")
            }
            Request::Piece { sha, piece } => send_piece(out, &sha, piece, self.zstd),
            Request::Done => self.incoming.done(out),
            Request::Manifest(fp) => send_manifest(out, &fp),
            Request::Blob(sha) => send_blob(out, &sha),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Option<Result<Request, Rejected>> {
        Request::parse(&line.split_whitespace().collect::<Vec<_>>())
    }

    #[test]
    fn each_request_is_read_into_what_its_handler_works_with() {
        let sha = "ab".repeat(32);
        let fp = format!("v1-{}", "cd".repeat(32));
        assert_eq!(parse("OFFER 120"), Some(Ok(Request::Offer { len: 120 })));
        assert_eq!(parse("OFFER2 7"), Some(Ok(Request::OfferPieces { len: 7 })));
        assert_eq!(
            parse("PUT 3 4096"),
            Some(Ok(Request::Put {
                index: 3,
                len: 4096
            }))
        );
        assert_eq!(
            parse("PUTPIECE 3 1 1048576 y 2048"),
            Some(Ok(Request::PutPiece(PutPiece {
                index: 3,
                piece: 1,
                raw: 1 << 20,
                codec: "y".into(),
                wire: 2048
            })))
        );
        assert_eq!(parse("ZSTD"), Some(Ok(Request::Zstd)));
        assert_eq!(
            parse(&format!("PIECE {sha} 9")),
            Some(Ok(Request::Piece {
                sha: Sha256::parse(&sha).unwrap(),
                piece: 9
            }))
        );
        assert_eq!(parse("DONE"), Some(Ok(Request::Done)));
        assert_eq!(
            parse(&format!("MANIFEST {fp}")),
            Some(Ok(Request::Manifest(Fingerprint::parse(&fp).unwrap())))
        );
        assert_eq!(
            parse(&format!("BLOB {sha}")),
            Some(Ok(Request::Blob(Sha256::parse(&sha).unwrap())))
        );
        // (the host's own requests, and words it does not know, are not these)
        assert_eq!(parse("LIST"), None);
        assert_eq!(parse("GET 4"), None);
        assert_eq!(parse("OFFER"), None);
        assert_eq!(parse("HELLO there"), None);
    }

    #[test]
    fn what_is_asked_wrongly_is_answered_as_it_always_was() {
        // a file or bus that cannot be: ERR, and the connection goes on
        assert_eq!(
            parse("PIECE nothex 1"),
            Some(Err(Rejected::Answer(says::NO_SUCH_FILE)))
        );
        assert_eq!(
            parse(&format!("PIECE {} x", "0".repeat(64))),
            Some(Err(Rejected::Answer(says::NO_SUCH_FILE)))
        );
        assert_eq!(
            parse("BLOB 1234"),
            Some(Err(Rejected::Answer(says::NO_SUCH_FILE)))
        );
        assert_eq!(
            parse("MANIFEST nobus"),
            Some(Err(Rejected::Answer(says::NO_SUCH_BUS)))
        );
        // numbers that make no sense (a body of their size may follow): the connection ends
        assert_eq!(parse("OFFER many"), Some(Err(Rejected::End("bad offer"))));
        assert_eq!(
            parse(&format!("OFFER {}", MAX_OFFER + 1)),
            Some(Err(Rejected::End("bad offer")))
        );
        assert_eq!(parse("PUT x 10"), Some(Err(Rejected::End("bad put"))));
        assert_eq!(
            parse("PUTPIECE 0 0 ten y 5"),
            Some(Err(Rejected::End("bad piece")))
        );
    }
}
