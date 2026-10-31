//! Sound files. WAV is read here - every coding OMSI's sounds come in, quickly and without
//! a copy: PCM of 8 to 32 bits in any container size (20-bit audio in 24-bit words, 12-bit
//! in 16), 32- and 64-bit float, A-law and mu-law, `WAVE_FORMAT_EXTENSIBLE` (whose sub-format
//! says integer or float), RF64 / BW64 for files past 4 GB, and the damaged headers the
//! sound tools of the OMSI years left (a data size past the end of the file, or 0 / -1 for
//! "to the end"). Everything else goes to symphonia ([`parse_compressed`]): ADPCM WAVs, Ogg
//! Vorbis, FLAC, MP3 / MP2, AAC and ALAC in MP4 / M4A, AIFF, CAF and Matroska audio.
//! More than two channels are mixed down to stereo as the standard downmix does.

use anyhow::{anyhow, Result};

pub struct WavData {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved samples as 16-bit integers (-32768..32767 for -1..1): half the memory of
    /// floats, and what nearly every OMSI sound is anyway (the decoded sounds of a big map
    /// with its fleet took 320 MB as floats).
    pub samples: Vec<i16>,
}

fn quantize(x: f32) -> i16 {
    (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16
}

/// G.711 A-law and mu-law to linear 16-bit.
fn alaw(v: u8) -> i16 {
    let v = v ^ 0x55;
    let t = ((v & 0x0F) as i16) << 4;
    let seg = (v & 0x70) >> 4;
    let t = match seg {
        0 => t + 8,
        1 => t + 0x108,
        _ => (t + 0x108) << (seg - 1),
    };
    if v & 0x80 != 0 { t } else { -t }
}

fn mulaw(v: u8) -> i16 {
    let v = !v;
    let t = ((((v & 0x0F) as i32) << 3) + 0x84) << ((v & 0x70) >> 4);
    (if v & 0x80 != 0 { 0x84 - t } else { t - 0x84 }) as i16
}

/// Whether the bytes are a WAV file this reader is meant for (RIFF/RIFX/RF64/BW64 WAVE).
pub fn is_wav(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && matches!(&bytes[0..4], b"RIFF" | b"RF64" | b"BW64") && &bytes[8..12] == b"WAVE"
}

pub fn parse_wav(bytes: &[u8]) -> Result<WavData> {
    if !is_wav(bytes) {
        return Err(anyhow!("not a RIFF/WAVE file"));
    }
    let mut p = 12;
    // format tag, channels, rate, bits, block align
    let mut fmt: Option<(u16, u16, u32, u16, u16)> = None;
    let mut data: Option<&[u8]> = None;
    // RF64: the real data size lives in the `ds64` chunk
    let mut ds64_data: Option<u64> = None;
    while p + 8 <= bytes.len() {
        let id = &bytes[p..p + 4];
        let raw = u32::from_le_bytes(bytes[p + 4..p + 8].try_into().unwrap());
        let mut size = raw as usize;
        if id == b"data" {
            // a size of -1 (RF64, or a recording stopped before its header was finished),
            // 0 (a stream) or past the end: the data goes on to the end of the file
            if let Some(n) = ds64_data.filter(|_| raw == u32::MAX) {
                size = n as usize;
            }
            if raw == 0 || raw == u32::MAX || p + 8 + size > bytes.len() {
                size = bytes.len() - (p + 8);
            }
        }
        let body_end = (p + 8 + size).min(bytes.len());
        let body = &bytes[p + 8..body_end];
        match id {
            b"fmt " if body.len() >= 16 => {
                let mut tag = u16::from_le_bytes([body[0], body[1]]);
                let ch = u16::from_le_bytes([body[2], body[3]]);
                let rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
                let align = u16::from_le_bytes([body[12], body[13]]);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                // WAVE_FORMAT_EXTENSIBLE: the sub-format GUID begins with the real tag
                if tag == 0xFFFE && body.len() >= 26 {
                    tag = u16::from_le_bytes([body[24], body[25]]);
                }
                fmt = Some((tag, ch, rate, bits, align));
            }
            b"ds64" if body.len() >= 16 => ds64_data = Some(u64::from_le_bytes(body[8..16].try_into().unwrap())),
            b"data" => {
                // (the first data chunk; a second one is a tool's leftover)
                if data.is_none() {
                    data = Some(body);
                }
            }
            _ => {}
        }
        // a chunk claiming more than the file holds ends the walk
        p = match (p + 8).checked_add(size + (size & 1)) {
            Some(n) => n,
            None => break,
        };
    }
    let (tag, channels, sample_rate, bits, align) = fmt.ok_or_else(|| anyhow!("missing fmt chunk"))?;
    let data = data.ok_or_else(|| anyhow!("missing data chunk"))?;
    let channels = channels.max(1);
    if sample_rate == 0 {
        return Err(anyhow!("a sample rate of 0"));
    }
    // the container size of one sample: what the block align says (20-bit audio sits in
    // 3 bytes, 12-bit in 2), else what the bits round up to
    let width = match (align as usize).checked_div(channels as usize) {
        Some(w) if (1..=8).contains(&w) && w * 8 >= bits as usize => w,
        _ => (bits as usize).div_ceil(8),
    };
    let samples: Vec<i16> = match (tag, width) {
        (1, 1) => data.iter().map(|b| ((*b as i16) - 128) << 8).collect(),
        (1, 2) => data.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect(),
        (1, 3) => data.chunks_exact(3).map(|c| i16::from_le_bytes([c[1], c[2]])).collect(),
        (1, 4) => data.chunks_exact(4).map(|c| i16::from_le_bytes([c[2], c[3]])).collect(),
        (3, 4) => data.chunks_exact(4).map(|c| quantize(f32::from_le_bytes(c.try_into().unwrap()))).collect(),
        (3, 8) => data.chunks_exact(8).map(|c| quantize(f64::from_le_bytes(c.try_into().unwrap()) as f32)).collect(),
        (6, 1) => data.iter().map(|b| alaw(*b)).collect(),
        (7, 1) => data.iter().map(|b| mulaw(*b)).collect(),
        // ADPCM and the rest: symphonia's decoders
        _ => return parse_compressed(bytes).map_err(|e| anyhow!("WAV format tag {tag} / {bits} bit: {e}")),
    };
    Ok(to_stereo(WavData { sample_rate, channels, samples }))
}

/// More than two channels mixed down to two (ITU: the centre and the surrounds at -3 dB, the
/// LFE left out), in WAV's channel order FL FR FC LFE BL BR SL SR; mono and stereo as they are.
pub fn to_stereo(w: WavData) -> WavData {
    let ch = w.channels as usize;
    if ch <= 2 {
        return w;
    }
    const K: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let mut out = Vec::with_capacity(w.samples.len() / ch * 2);
    for f in w.samples.chunks_exact(ch) {
        let s = |i: usize| f.get(i).map(|v| *v as f32).unwrap_or(0.0);
        // left: FL, FC, BL, SL; right: FR, FC, BR, SR (beyond 8 channels: the rest by parity)
        let (mut l, mut r) = (s(0), s(1));
        if ch >= 3 {
            l += K * s(2);
            r += K * s(2);
        }
        for i in 4..ch {
            if i % 2 == 0 {
                l += K * s(i);
            } else {
                r += K * s(i);
            }
        }
        // (scaled by what one side sums up, so that nothing clips)
        let side = 1.0 + K + K * (4..ch).filter(|i| i % 2 == 0).count() as f32;
        let norm = 1.0 / side;
        out.push((l * norm).round().clamp(-32768.0, 32767.0) as i16);
        out.push((r * norm).round().clamp(-32768.0, 32767.0) as i16);
    }
    WavData { sample_rate: w.sample_rate, channels: 2, samples: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(tag: u16, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&44100u32.to_le_bytes());
        b.extend_from_slice(&(44100u32 * bits as u32 / 8).to_le_bytes());
        b.extend_from_slice(&(bits / 8).to_le_bytes());
        b.extend_from_slice(&bits.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    #[test]
    fn extensible_float_double_g711_and_open_ended_data() {
        // WAVE_FORMAT_EXTENSIBLE with the float sub-format: read as floats, not integers
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&0xFFFEu16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&48000u32.to_le_bytes());
        b.extend_from_slice(&(48000u32 * 4).to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(&22u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&[3, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
        b.extend_from_slice(b"data");
        // -1: to the end of the file
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        b.extend_from_slice(&0.5f32.to_le_bytes());
        b.extend_from_slice(&(-0.25f32).to_le_bytes());
        assert_eq!(parse_wav(&b).unwrap().samples, vec![16384, -8192]);
        // 64-bit float
        let d: Vec<u8> = [0.5f64, -1.0].iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(parse_wav(&wav(3, 64, &d)).unwrap().samples, vec![16384, -32768]);
        // A-law and mu-law silence and peaks
        assert_eq!(parse_wav(&wav(7, 8, &[0xFF, 0x80, 0x00])).unwrap().samples, vec![0, 32124, -32124]);
        assert_eq!(parse_wav(&wav(6, 8, &[0xD5, 0xAA])).unwrap().samples, vec![8, 32256]);
        // a data size past the end of the file (a truncated download)
        let mut t = wav(1, 16, &[1, 0, 2, 0]);
        let n = t.len();
        t[n - 8..n - 4].copy_from_slice(&1000u32.to_le_bytes());
        assert_eq!(parse_wav(&t).unwrap().samples, vec![1, 2]);
    }

    #[test]
    fn surround_is_mixed_down_to_stereo() {
        // 5.1: FL FR FC LFE BL BR - a centre alone is heard equally on both sides
        let w = to_stereo(WavData { sample_rate: 48000, channels: 6, samples: vec![0, 0, 10000, 30000, 0, 0] });
        assert_eq!(w.channels, 2);
        assert_eq!(w.samples[0], w.samples[1]);
        assert!(w.samples[0] > 2000 && w.samples[0] < 10000, "{:?}", w.samples);
    }

    #[test]
    fn aiff_is_read() {
        // big-endian 16-bit mono AIFF, rate 44100 as an 80-bit extended float
        let mut comm = Vec::new();
        comm.extend_from_slice(&1u16.to_be_bytes());
        comm.extend_from_slice(&2u32.to_be_bytes());
        comm.extend_from_slice(&16u16.to_be_bytes());
        comm.extend_from_slice(&[0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        let mut ssnd = vec![0u8; 8];
        ssnd.extend_from_slice(&0x4000i16.to_be_bytes());
        ssnd.extend_from_slice(&(-0x4000i16).to_be_bytes());
        let mut body = b"AIFF".to_vec();
        for (id, c) in [(b"COMM", &comm), (b"SSND", &ssnd)] {
            body.extend_from_slice(id);
            body.extend_from_slice(&(c.len() as u32).to_be_bytes());
            body.extend_from_slice(c);
        }
        let mut b = b"FORM".to_vec();
        b.extend_from_slice(&(body.len() as u32).to_be_bytes());
        b.extend_from_slice(&body);
        assert_eq!(sniff(&b), Some("aiff"));
        let w = parse_any(&b).unwrap();
        assert_eq!((w.sample_rate, w.channels), (44100, 1));
        assert_eq!(w.samples, vec![0x4000, -0x4000]);
    }

    #[test]
    fn an_empty_sound_file_is_refused_without_probing() {
        assert!(parse_any(&[]).err().is_some_and(|e| e.to_string().contains("empty")));
        assert!(parse_any(&[0, 1]).is_err());
    }

    #[test]
    fn every_format_becomes_16_bit() {
        // 8-bit is unsigned around 128
        assert_eq!(parse_wav(&wav(1, 8, &[0, 128, 255])).unwrap().samples, vec![-32768, 0, 32512]);
        assert_eq!(parse_wav(&wav(1, 16, &[0x00, 0x80, 0xff, 0x7f])).unwrap().samples, vec![-32768, 32767]);
        // 24 and 32 bit keep their top 16 bits
        assert_eq!(parse_wav(&wav(1, 24, &[0x11, 0x00, 0x40])).unwrap().samples, vec![0x4000]);
        assert_eq!(parse_wav(&wav(1, 32, &[0x11, 0x22, 0x00, 0xc0])).unwrap().samples, vec![-0x4000]);
        let f: Vec<u8> = [0.5f32, -1.0, 2.0].iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(parse_wav(&wav(3, 32, &f)).unwrap().samples, vec![16384, -32768, 32767]);
    }
}

/// An MP3 file: an ID3 tag first, or an MPEG audio frame's sync word (11 bits set).
pub fn is_mp3(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3") || (bytes.len() > 2 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0 && bytes[1] & 0x06 != 0)
}

/// The container a sound file is by its signature, as symphonia's hint names it.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    let at = |o: usize, s: &[u8]| bytes.get(o..o + s.len()) == Some(s);
    Some(if is_wav(bytes) {
        "wav"
    } else if at(0, b"OggS") {
        "ogg"
    } else if at(0, b"fLaC") {
        "flac"
    } else if at(0, b"FORM") && (at(8, b"AIFF") || at(8, b"AIFC")) {
        "aiff"
    } else if at(0, b"caff") {
        "caf"
    } else if at(4, b"ftyp") {
        "m4a"
    } else if at(0, &[0x1A, 0x45, 0xDF, 0xA3]) {
        "mka"
    } else if is_mp3(bytes) {
        "mp3"
    } else {
        return None;
    })
}

/// A sound file of any format symphonia reads, decoded whole (the ambience's recordings ship
/// as MP3 / FLAC; mods bring OGG, M4A, AIFF ...).
pub fn parse_compressed(bytes: &[u8]) -> Result<WavData> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    // an empty (or cut-off) file would only make symphonia's probe log an error of its own
    if bytes.len() < 4 {
        return Err(anyhow!("the sound file is empty ({} bytes)", bytes.len()));
    }
    let mss = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes.to_vec())), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(sniff(bytes).unwrap_or("ogg"));
    let mut format = symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())?.format;
    let track = format.default_track().ok_or_else(|| anyhow!("no audio track"))?.clone();
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut channels = track.codec_params.channels.map(|c| c.count() as u16).unwrap_or(0);
    let mut samples = Vec::new();
    let mut buf: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track.id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        channels = spec.channels.count() as u16;
        let b = buf.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
        if b.capacity() < decoded.capacity() * spec.channels.count() {
            *b = SampleBuffer::new(decoded.capacity() as u64, spec);
        }
        b.copy_interleaved_ref(decoded);
        samples.extend(b.samples().iter().map(|x| quantize(*x)));
    }
    if rate == 0 || channels == 0 {
        return Err(anyhow!("no sound in the file"));
    }
    Ok(to_stereo(WavData { sample_rate: rate, channels, samples }))
}

/// Decode a sound file of any supported format (by its content).
pub fn parse_any(bytes: &[u8]) -> Result<WavData> {
    if is_wav(bytes) {
        parse_wav(bytes)
    } else {
        parse_compressed(bytes)
    }
}
