//! openOMSI staging protocol, NOT a NVIDIA or UniversalDLSS5 ABI.
use anyhow::{Result, bail, ensure};
use std::io::{Read, Write};

pub const HEADER_SIZE: usize = 384;
pub const MAX_PIXELS: u64 = 3840 * 2160;
pub const REQUEST: u32 = 1;
pub const RESPONSE: u32 = 2;
pub const LOADED: u32 = 1;
pub const SUPPORTED: u32 = 2;
pub const INITIALIZED: u32 = 4;
pub const ATTEMPTED: u32 = 8;
pub const SUCCEEDED: u32 = 16;
pub const COMPLETE: u32 = LOADED | SUPPORTED | INITIALIZED | ATTEMPTED | SUCCEEDED;

#[derive(Clone, Debug)]
pub struct Header {
    pub kind: u32,
    pub session: u64,
    pub frame: u64,
    pub generation: u32,
    pub width: u32,
    pub height: u32,
    /// bit0 reverse Z, bit1 reset, bit2 sRGB encoded LDR; others reserved.
    pub flags: u32,
    pub payload: u64,
    pub stages: u32,
    pub result: i32,
    /// Row-major viewToClip, clipToView, clipToPrevClip, prevClipToClip;
    /// camera position, up, right, forward; near, far, FOV radians, aspect.
    pub camera: [f32; 80],
}
impl Header {
    pub fn pixels(&self) -> Result<usize> {
        let n = u64::from(self.width) * u64::from(self.height);
        ensure!(
            self.width > 0 && self.height > 0 && n <= MAX_PIXELS,
            "invalid extent"
        );
        Ok(n as usize)
    }
    pub fn write(&self, mut out: impl Write) -> Result<()> {
        let mut b = Vec::with_capacity(HEADER_SIZE);
        b.extend_from_slice(b"OMSINR01");
        for v in [1, self.kind] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for v in [self.session, self.frame] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for v in [self.generation, self.width, self.height, self.flags] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&self.payload.to_le_bytes());
        b.extend_from_slice(&self.stages.to_le_bytes());
        b.extend_from_slice(&self.result.to_le_bytes());
        for v in self.camera {
            b.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(b.len(), HEADER_SIZE);
        out.write_all(&b)?;
        Ok(())
    }
    pub fn read(mut input: impl Read) -> Result<Self> {
        let mut b = [0u8; HEADER_SIZE];
        input.read_exact(&mut b)?;
        ensure!(&b[..8] == b"OMSINR01", "bad magic");
        let u32_at = |i| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let u64_at = |i| u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        ensure!(u32_at(8) == 1, "unsupported protocol");
        let h = Self {
            kind: u32_at(12),
            session: u64_at(16),
            frame: u64_at(24),
            generation: u32_at(32),
            width: u32_at(36),
            height: u32_at(40),
            flags: u32_at(44),
            payload: u64_at(48),
            stages: u32_at(56),
            result: u32_at(60) as i32,
            camera: std::array::from_fn(|i| f32::from_bits(u32_at(64 + i * 4))),
        };
        h.pixels()?;
        ensure!(h.camera.iter().all(|v| v.is_finite()), "nonfinite camera");
        ensure!(h.flags & !7 == 0, "unsupported flags");
        Ok(h)
    }
    pub fn validate_response(&self, request: &Self) -> Result<()> {
        ensure!(
            self.kind == RESPONSE
                && self.session == request.session
                && self.frame == request.frame
                && self.generation == request.generation
                && self.width == request.width
                && self.height == request.height
                && self.flags == request.flags,
            "stale or unrelated output"
        );
        ensure!(
            self.payload == self.pixels()? as u64 * 4,
            "wrong output byte count"
        );
        if self.result != 0 || self.stages != COMPLETE {
            bail!(
                "host did not complete neural evaluation: stages={:#x}, result={:#x}",
                self.stages,
                self.result
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn header() -> Header {
        Header {
            kind: REQUEST,
            session: 13,
            frame: 6,
            generation: 2,
            width: 2,
            height: 3,
            flags: 7,
            payload: 102,
            stages: 0,
            result: 0,
            camera: [0.; 80],
        }
    }
    #[test]
    fn wire_roundtrip_and_bounded_parser() {
        let h = header();
        let mut b = Vec::new();
        h.write(&mut b).unwrap();
        assert_eq!(b.len(), 384);
        assert_eq!(Header::read(&b[..]).unwrap().frame, 6);
        assert!(Header::read(&b[..383]).is_err());
        b[8] = 2;
        assert!(Header::read(&b[..]).is_err());
        b[8] = 1;
        b[36..40].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Header::read(&b[..]).is_err());
    }
    #[test]
    fn copy_runtime_presence_and_old_frames_are_not_success() {
        let h = header();
        let mut out = h.clone();
        out.kind = RESPONSE;
        out.payload = 24;
        for stages in [0, LOADED, LOADED | SUPPORTED, COMPLETE & !SUCCEEDED] {
            out.stages = stages;
            assert!(out.validate_response(&h).is_err());
        }
        out.stages = COMPLETE;
        out.validate_response(&h).unwrap();
        out.frame -= 1;
        assert!(out.validate_response(&h).is_err());
        out.frame += 1;
        out.generation += 1;
        assert!(out.validate_response(&h).is_err());
        out.generation -= 1;
        out.result = -1;
        assert!(out.validate_response(&h).is_err());
    }
}
