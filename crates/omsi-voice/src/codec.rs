//! Opus for speech: 48 kHz mono in 20 ms frames, about 24 kbit/s, with in-band forward error
//! correction (a lost frame is mostly recovered from the next one) and the decoder's own
//! loss concealment.
//!
//! The codec is `unsafe-libopus` (libopus transpiled to Rust, BSD-3-Clause like libopus
//! itself); this module is the only place that touches its raw interface.

use unsafe_libopus::{
    OPUS_APPLICATION_VOIP, OPUS_SET_BITRATE_REQUEST, OPUS_SET_COMPLEXITY_REQUEST,
    OPUS_SET_INBAND_FEC_REQUEST, OPUS_SET_PACKET_LOSS_PERC_REQUEST, OPUS_SET_SIGNAL_REQUEST,
    OPUS_SIGNAL_VOICE, OpusDecoder, OpusEncoder, opus_decode, opus_decoder_create,
    opus_decoder_destroy, opus_encode, opus_encoder_create, opus_encoder_ctl, opus_encoder_destroy,
};

pub const RATE: u32 = 48_000;
/// Samples of a frame (20 ms).
pub const FRAME: usize = 960;
/// Frames last 20 ms.
pub const FRAME_SECS: f32 = 0.02;
pub const BITRATE: i32 = 24_000;
/// The largest packet the encoder may give (the relay takes `omsi_net::voice_net::MAX_PAYLOAD`).
pub const MAX_PACKET: usize = 400;

pub struct Encoder {
    raw: *mut OpusEncoder,
}

// (the encoder is used from one thread at a time: the voice thread)
unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new() -> Option<Encoder> {
        let mut err = 0i32;
        let raw = unsafe { opus_encoder_create(RATE as i32, 1, OPUS_APPLICATION_VOIP, &mut err) };
        if raw.is_null() || err != 0 {
            return None;
        }
        unsafe {
            opus_encoder_ctl!(raw, OPUS_SET_BITRATE_REQUEST, BITRATE);
            opus_encoder_ctl!(raw, OPUS_SET_COMPLEXITY_REQUEST, 5);
            opus_encoder_ctl!(raw, OPUS_SET_SIGNAL_REQUEST, OPUS_SIGNAL_VOICE);
            opus_encoder_ctl!(raw, OPUS_SET_INBAND_FEC_REQUEST, 1);
            opus_encoder_ctl!(raw, OPUS_SET_PACKET_LOSS_PERC_REQUEST, 10);
        }
        Some(Encoder { raw })
    }

    /// One frame of `FRAME` samples (-1..1) as a packet; None when the encoder failed.
    pub fn encode(&mut self, pcm: &[f32]) -> Option<Vec<u8>> {
        if pcm.len() != FRAME {
            return None;
        }
        let mut samples = [0i16; FRAME];
        for (o, s) in samples.iter_mut().zip(pcm) {
            *o = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        }
        let mut out = vec![0u8; MAX_PACKET];
        let n = unsafe {
            opus_encode(
                self.raw,
                samples.as_ptr(),
                FRAME as i32,
                out.as_mut_ptr(),
                MAX_PACKET as i32,
            )
        };
        if n <= 0 {
            return None;
        }
        out.truncate(n as usize);
        Some(out)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe { opus_encoder_destroy(self.raw) }
    }
}

pub struct Decoder {
    raw: *mut OpusDecoder,
}

unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new() -> Option<Decoder> {
        let mut err = 0i32;
        let raw = unsafe { opus_decoder_create(RATE as i32, 1, &mut err) };
        (!raw.is_null() && err == 0).then_some(Decoder { raw })
    }

    fn run(&mut self, packet: Option<&[u8]>, fec: bool) -> Option<[f32; FRAME]> {
        // (an empty packet means a lost one to Opus; here it is not a packet)
        if packet.is_some_and(|p| p.is_empty() || p.len() > MAX_PACKET) {
            return None;
        }
        let mut pcm = [0i16; FRAME];
        let (data, len) = match packet {
            Some(p) => (p.as_ptr(), p.len() as i32),
            None => (std::ptr::null(), 0),
        };
        let n = unsafe {
            opus_decode(
                self.raw,
                data,
                len,
                pcm.as_mut_ptr(),
                FRAME as i32,
                fec as i32,
            )
        };
        if n != FRAME as i32 {
            return None;
        }
        let mut out = [0f32; FRAME];
        for (o, s) in out.iter_mut().zip(pcm) {
            *o = s as f32 / 32768.0;
        }
        Some(out)
    }

    /// The frame a packet holds; None for a packet that is not Opus.
    pub fn decode(&mut self, packet: &[u8]) -> Option<[f32; FRAME]> {
        self.run(Some(packet), false)
    }

    /// The frame before `next`, rebuilt from the error correction `next` carries.
    pub fn recover(&mut self, next: &[u8]) -> Option<[f32; FRAME]> {
        self.run(Some(next), true)
    }

    /// A frame made up for one that never came (the decoder carries the sound on).
    pub fn conceal(&mut self) -> [f32; FRAME] {
        self.run(None, false).unwrap_or([0.0; FRAME])
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { opus_decoder_destroy(self.raw) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn tone(freq: f32, frame: usize, amp: f32) -> Vec<f32> {
        (0..FRAME)
            .map(|i| {
                amp * (2.0
                    * std::f32::consts::PI
                    * freq
                    * ((frame * FRAME + i) as f32 / RATE as f32))
                    .sin()
            })
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// Energy of `x` at `freq` (a one-bin DFT), to see the tone survived.
    fn at(x: &[f32], freq: f32) -> f32 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, s) in x.iter().enumerate() {
            let a = 2.0 * std::f32::consts::PI * freq * i as f32 / RATE as f32;
            re += s * a.cos();
            im += s * a.sin();
        }
        (re * re + im * im).sqrt() / x.len() as f32 * 2.0
    }

    #[test]
    fn a_tone_round_trips_and_the_packets_are_small() {
        let (mut e, mut d) = (Encoder::new().unwrap(), Decoder::new().unwrap());
        let mut out = Vec::new();
        let mut bytes = 0;
        for f in 0..25 {
            let p = e.encode(&tone(440.0, f, 0.5)).unwrap();
            assert!(p.len() <= MAX_PACKET);
            bytes += p.len();
            out.extend(d.decode(&p).unwrap());
        }
        // 24 kbit/s is 60 bytes a frame; allow for the first frames
        assert!(bytes / 25 < 120, "{} bytes a frame", bytes / 25);
        let tail = &out[out.len() - 5 * FRAME..];
        let level = at(tail, 440.0);
        assert!(level > 0.35 && level < 0.65, "440 Hz at {level}");
        assert!(at(tail, 1500.0) < 0.05);
    }

    #[test]
    fn a_lost_frame_is_recovered_or_concealed() {
        let (mut e, mut d) = (Encoder::new().unwrap(), Decoder::new().unwrap());
        let packets: Vec<_> = (0..12)
            .map(|f| e.encode(&tone(300.0, f, 0.5)).unwrap())
            .collect();
        for p in &packets[..8] {
            d.decode(p).unwrap();
        }
        // frame 8 is lost: frame 9 carries it
        let healed = d.recover(&packets[9]).unwrap();
        let after = d.decode(&packets[9]).unwrap();
        assert!(
            rms(&healed) > 0.1,
            "recovered frame is not silent: {}",
            rms(&healed)
        );
        assert!(rms(&after) > 0.1);
        // two lost in a row: concealment carries on (not a click, not a silent hole at once)
        let c = d.conceal();
        assert!(rms(&c) > 0.01 && c.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn garbage_is_not_a_packet() {
        let mut d = Decoder::new().unwrap();
        assert!(d.decode(&[]).is_none());
        // (random bytes may parse as some Opus frame - it must not crash, nor give a wrong size)
        for seed in 0..50u8 {
            let junk: Vec<u8> = (0..40u8)
                .map(|i| seed.wrapping_mul(31).wrapping_add(i.wrapping_mul(7)))
                .collect();
            let _ = d.decode(&junk);
        }
        assert!(Encoder::new().unwrap().encode(&[0.0; 10]).is_none());
    }
}
