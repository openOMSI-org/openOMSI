//! Every file that came from another machine is looked at before it is kept or passed on -
//! a host's mods by the player who fetches them (and a player's bus by the server it is sent
//! to, and again by each player who fetches it): it must be what its name says it is (a
//! positive check of its format, not only the absence of a program's signature,
//! `looks_executable`), and a picture may not claim a size no texture has. One damaged or
//! crafted file would otherwise reach every player of the session at once (a parser's fault
//! there is a fault in every game that reads it). Nothing here limits how large a file is.

/// The largest side a texture the game takes has (it halves larger ones on cards that
/// cannot hold them; a picture that says more is no texture). Spline textures are long:
/// the Brasil roads have one of 412x17902.
const MAX_SIDE: u32 = 65536;
/// The most pixels a texture has (1 GB as RGBA): a picture that claims more is a bomb.
const MAX_PIXELS: u64 = 1 << 28;

/// What is taken from another machine passes here: no program (`looks_executable`, whatever
/// its name), and what its name says it is (`check`). The reason when it does not.
pub(crate) fn screen(path: &str, data: &[u8]) -> Result<(), String> {
    if super::looks_executable(&data[..data.len().min(8)]) {
        return Err(format!("{path}: a program"));
    }
    check(path, data)
}

/// Is `data` what a file named `path` should be? The reason when it is not.
pub(crate) fn check(path: &str, data: &[u8]) -> Result<(), String> {
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let why = match ext.as_str() {
        // (the game reads a picture by what it is, not by its name: the stock SD77's
        // repaints have bitmaps called .dds)
        "dds" | "tga" | "bmp" | "png" | "jpg" | "jpeg" => picture(data),
        "wav" => (data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE")
            .then_some(())
            .ok_or_else(|| "not a WAVE sound".into()),
        "ogg" => data
            .starts_with(b"OggS")
            .then_some(())
            .ok_or_else(|| "not an Ogg sound".into()),
        // (a mesh the reader turns down - cut short, nothing in it (a stub for a part left
        // out: the Caio Apache's interior_bancos0.o3d is 77 bytes) - is turned down the same
        // way by every game that draws the bus, its driver's too; only one that would stop
        // the reader is kept out)
        "o3d" => mesh(|| {
            let _ = omsi_o3d::parse_o3d(data);
        }),
        "x" => mesh(|| {
            let _ = omsi_o3d::xfile::parse_x(data);
        }),
        // OMSI's texts: configuration, scripts, lists, fonts, depot files - or a picture
        // under such a name (the Buildings_MC textures are .dds files called .sco; the
        // game reads a picture by what it is)
        "cfg" | "bus" | "ovh" | "zug" | "osc" | "txt" | "hof" | "oft" | "cti" | "sco" | "sli"
        | "hum" | "owt" | "clb" | "ctc" | "ini" => (text(data) || picture(data).is_ok())
            .then_some(())
            .ok_or_else(|| "not a text file".into()),
        _ => Ok(()),
    };
    why.map_err(|w| format!("{path}: {w}"))
}

/// A text the game reads: no zero byte - or UTF-16 with its mark (`FF FE`) and no zero
/// character, as `omsi_cfg::decode_text` reads it (the O560 E5's fonts are written so: a
/// zero byte in every character, and the whole bus was refused for one).
fn text(data: &[u8]) -> bool {
    match data.strip_prefix(&[0xFF, 0xFE]) {
        Some(utf16) => utf16.len() % 2 == 0 && !utf16.chunks(2).any(|c| c == [0, 0]),
        None => !data.contains(&0),
    }
}

/// A mesh is read the way the game reads it; one that would stop the reader is no mesh.
fn mesh(read: impl FnOnce()) -> Result<(), String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
        .map_err(|_| "a mesh the reader cannot take".into())
}

/// A picture of any kind the game reads, whatever it is called, of a size a texture can have.
fn picture(d: &[u8]) -> Result<(), String> {
    if d.starts_with(b"DDS ") {
        dds(d)
    } else if d.starts_with(b"BM") {
        bmp(d)
    } else if d.starts_with(&[0x89, b'P', b'N', b'G']) {
        png(d)
    } else if d.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Ok(())
    } else {
        // (a TGA has no signature: its header must hold together)
        tga(d).map_err(|_| "not a picture".to_string())
    }
}

fn side(name: &str, w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || w as u64 * h as u64 > MAX_PIXELS {
        return Err(format!("a {name} of {w}x{h} (no texture is that size)"));
    }
    Ok(())
}

fn u32le(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn dds(d: &[u8]) -> Result<(), String> {
    if d.len() < 128 || &d[..4] != b"DDS " || u32le(d, 4) != 124 {
        return Err("not a DDS texture".into());
    }
    side("DDS texture", u32le(d, 16), u32le(d, 12))
}

fn tga(d: &[u8]) -> Result<(), String> {
    if d.len() < 18
        || ![1, 2, 3, 9, 10, 11].contains(&d[2])
        || ![8, 15, 16, 24, 32].contains(&d[16])
    {
        return Err("not a TGA picture".into());
    }
    side(
        "TGA picture",
        u16::from_le_bytes([d[12], d[13]]) as u32,
        u16::from_le_bytes([d[14], d[15]]) as u32,
    )
}

fn bmp(d: &[u8]) -> Result<(), String> {
    if d.len() < 30 || &d[..2] != b"BM" {
        return Err("not a BMP picture".into());
    }
    // (the OS/2 header - 12 bytes, sides of 16 bits - of many an older font's bitmap:
    // read as the Windows one, it said "0 bits a pixel" and the font was refused)
    let (w, h, bpp) = if u32le(d, 14) == 12 {
        (
            u16::from_le_bytes([d[18], d[19]]) as u32,
            u16::from_le_bytes([d[20], d[21]]) as u32,
            u16::from_le_bytes([d[24], d[25]]),
        )
    } else {
        (
            i32::from_le_bytes([d[18], d[19], d[20], d[21]]).unsigned_abs(),
            i32::from_le_bytes([d[22], d[23], d[24], d[25]]).unsigned_abs(),
            u16::from_le_bytes([d[28], d[29]]),
        )
    };
    // (0: the bit count the picture's own data says - a compressed one, or what some font
    // tools write; the game's reader takes it or shows nothing. What is checked here is that
    // it is a picture of a size a texture has - a refused file kept the whole bus away)
    if ![0, 1, 4, 8, 16, 24, 32].contains(&bpp) {
        return Err(format!(
            "a BMP of {bpp} bits a pixel (header {} bytes, {w}x{h})",
            u32le(d, 14)
        ));
    }
    side("BMP picture", w, h)
}

fn png(d: &[u8]) -> Result<(), String> {
    if d.len() < 24
        || !d.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
        || &d[12..16] != b"IHDR"
    {
        return Err("not a PNG picture".into());
    }
    side(
        "PNG picture",
        u32::from_be_bytes([d[16], d[17], d[18], d[19]]),
        u32::from_be_bytes([d[20], d[21], d[22], d[23]]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dds_of(w: u32, h: u32) -> Vec<u8> {
        let mut d = vec![0u8; 128];
        d[..4].copy_from_slice(b"DDS ");
        d[4..8].copy_from_slice(&124u32.to_le_bytes());
        d[12..16].copy_from_slice(&h.to_le_bytes());
        d[16..20].copy_from_slice(&w.to_le_bytes());
        d
    }

    #[test]
    fn files_are_what_their_names_say() {
        assert!(check("Vehicles/B/t.dds", &dds_of(2048, 1024)).is_ok());
        // a picture that claims a size no texture has (a decompression bomb)
        assert!(
            check("Vehicles/B/t.dds", &dds_of(100_000, 100_000))
                .unwrap_err()
                .contains("100000x100000")
        );
        // a program, or anything else, under a picture's name
        assert!(
            check(
                "Vehicles/B/t.dds",
                b"MZ\x90\x00 not a texture at all, padded to a size"
            )
            .is_err()
        );
        // a picture of another kind under that name is a picture (as the game reads it)
        let mut bmp = vec![0u8; 30];
        bmp[..2].copy_from_slice(b"BM");
        bmp[18..22].copy_from_slice(&1024i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&1024i32.to_le_bytes());
        bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
        assert!(check("Vehicles/B/t.dds", &bmp).is_ok());
        // a font's bitmap with the OS/2 header (12 bytes, 16-bit sides): 512x512, 24 bits
        let mut os2 = vec![0u8; 30];
        os2[..2].copy_from_slice(b"BM");
        os2[14..18].copy_from_slice(&12u32.to_le_bytes());
        os2[18..20].copy_from_slice(&512u16.to_le_bytes());
        os2[20..22].copy_from_slice(&512u16.to_le_bytes());
        os2[22..24].copy_from_slice(&1u16.to_le_bytes());
        os2[24..26].copy_from_slice(&24u16.to_le_bytes());
        assert_eq!(check("Fonts/22_ANX_L_Alpha.bmp", &os2), Ok(()));
        // a bit count of 0 (the data says it) is taken; a count no BMP has is not
        os2[24..26].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(check("Fonts/3dtranstudio_GGstopwords.bmp", &os2), Ok(()));
        os2[24..26].copy_from_slice(&7u16.to_le_bytes());
        assert!(check("Fonts/x.bmp", &os2).is_err());
        // nor one of a size no texture has (60000x60000: more pixels than any)
        os2[18..20].copy_from_slice(&60000u16.to_le_bytes());
        os2[20..22].copy_from_slice(&60000u16.to_le_bytes());
        os2[24..26].copy_from_slice(&24u16.to_le_bytes());
        assert!(check("Fonts/x.bmp", &os2).is_err());
        assert!(
            check(
                "Vehicles/B/t.png",
                b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x01\0\0\0\x01\0"
            )
            .is_ok()
        );
        assert!(
            check(
                "Vehicles/B/t.png",
                b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\x7f\xff\xff\xff\0\0\x01\0"
            )
            .is_err()
        );
        assert!(check("Vehicles/B/s.wav", b"RIFF\0\0\0\0WAVEfmt ").is_ok());
        assert!(check("Vehicles/B/s.wav", b"ID3\x03 an mp3 called wav").is_err());
        assert!(check("Vehicles/B/model.cfg", b"[mesh]\nbody.o3d\n").is_ok());
        assert!(check("Vehicles/B/model.cfg", b"[mesh]\0\0binary").is_err());
        // a text in UTF-16 with its mark (a font of the O560 E5) is a text; without the mark,
        // with a zero character or half a character, it is not
        let utf16 = |s: &str| {
            [0xFF, 0xFE]
                .into_iter()
                .chain(s.encode_utf16().flat_map(|u| u.to_le_bytes()))
                .collect::<Vec<u8>>()
        };
        assert!(
            check(
                "Fonts/o560_e5_small.oft",
                &utf16("[newfont]\r\no560_e5_small\r\n")
            )
            .is_ok()
        );
        assert!(check("Fonts/x.oft", &utf16("[newfont]")[2..]).is_err());
        assert!(check("Fonts/x.oft", &utf16("[newfont]\0")).is_err());
        assert!(check("Fonts/x.oft", &utf16("[newfont]")[..7]).is_err());
        // a mesh the reader turns down, cut short or with nothing in it, goes: every game
        // turns it down the same way (the Caio Apache's 77-byte stub)
        assert!(check("Vehicles/B/body.o3d", b"\x84\x19 broken").is_ok());
        let mut stub = vec![0u8; 77];
        stub[..4].copy_from_slice(&[0x84, 0x19, 0x01, 0x17]);
        assert!(check("Vehicles/B/interior_bancos0.o3d", &stub).is_ok());
        assert!(check("Vehicles/B/body.o3d", b"").is_ok());
        let x =
            b"xof 0303txt 0032\nMesh m {\n 3;\n 0;0;0;,\n 1;0;0;,\n 0;1;0;;\n 1;\n 3;0,1,2;;\n}\n";
        assert!(check("Vehicles/B/box.x", x).is_ok());
        assert!(check("Vehicles/B/cut.x", &x[..30]).is_ok());
        // a picture under a text's name (the Buildings_MC textures are .dds called .sco)
        assert!(
            check(
                "Sceneryobjects/Buildings_MC/texture/Bolle.sco",
                &dds_of(512, 512)
            )
            .is_ok()
        );
        assert!(
            check(
                "Sceneryobjects/Buildings_MC/texture/Bolle.sco",
                &dds_of(100_000, 100_000)
            )
            .is_err()
        );
        // a spline's long texture (412x17902 on the Brasil roads) is a texture; a bomb is not
        bmp[18..22].copy_from_slice(&412i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&17902i32.to_le_bytes());
        assert!(check("Splines/B.V@piaui/texture/B.V_BR_A1.bmp", &bmp).is_ok());
        bmp[18..22].copy_from_slice(&40000i32.to_le_bytes());
        bmp[22..26].copy_from_slice(&40000i32.to_le_bytes());
        assert!(
            check("Splines/x.bmp", &bmp)
                .unwrap_err()
                .contains("40000x40000")
        );
        // what is not looked into goes as it is
        assert!(check("Vehicles/B/readme.md", b"anything").is_ok());
    }

    /// Every file of a stock bus's folder (`OMSI_ROOT`) passes: pictures, sounds, meshes
    /// and texts.
    #[test]
    fn a_stock_bus_passes_whole() {
        let Some(root) = std::env::var_os("OMSI_ROOT").map(std::path::PathBuf::from) else {
            return;
        };
        let mut todo = vec![omsi_cfg::resolve_path(&root, "Vehicles/MAN_SD200")];
        let (mut seen, mut bad) = (0, Vec::new());
        while let Some(d) = todo.pop() {
            for p in std::fs::read_dir(&d)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
            {
                if p.is_dir() {
                    todo.push(p);
                    continue;
                }
                // (an archive beside the files - the SD200's SD-Sounds.zip - is none the bus
                // reads; any archive is refused, as it may be a Java program)
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
                    continue;
                }
                let rel = p
                    .strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                seen += 1;
                if let Err(why) = screen(&rel, &std::fs::read(&p).unwrap()) {
                    bad.push(why);
                }
            }
        }
        assert!(seen > 0 && bad.is_empty(), "{seen} files: {bad:?}");
    }

    /// Every file of the content on this machine through what is checked of what comes from
    /// another machine (`screen`): what it would refuse, by reason - to find the files it
    /// takes for something they are not. `QUARANTINE_SCAN`: targets separated by `|`,
    /// each a content root, `<folder>=><prefix>` (a folder read as if it were `<prefix>`,
    /// e.g. `D:\Bus=>Vehicles/Bus`) or a .zip (its files as the game reads one in place).
    /// The report goes to `QUARANTINE_SCAN_OUT`.
    ///   QUARANTINE_SCAN=... cargo test -p omsi-lan --lib scan_content -- --ignored --nocapture
    #[test]
    #[ignore]
    fn scan_content() {
        use std::collections::BTreeMap;
        let targets = std::env::var("QUARANTINE_SCAN").expect("QUARANTINE_SCAN");
        let mut refused: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let (mut files, mut bytes) = (0u64, 0u64);
        let mut look = |rel: &str, data: &[u8], refused: &mut BTreeMap<String, Vec<String>>| {
            if super::super::refuse_path(rel).is_some() {
                return;
            }
            files += 1;
            bytes += data.len() as u64;
            if let Err(w) = screen(rel, data) {
                let reason = w
                    .strip_prefix(&format!("{rel}: "))
                    .unwrap_or(&w)
                    .to_string();
                // (sizes in a reason make every file a reason of its own)
                let key = reason.split(" (").next().unwrap_or(&reason).to_string();
                refused
                    .entry(key)
                    .or_default()
                    .push(format!("{rel}  [{reason}]"));
            }
        };
        for t in targets.split('|').map(str::trim).filter(|t| !t.is_empty()) {
            let (dir, prefix) = match t.split_once("=>") {
                Some((d, p)) => (
                    std::path::PathBuf::from(d),
                    Some(p.trim_end_matches('/').to_string()),
                ),
                None => (std::path::PathBuf::from(t), None),
            };
            if dir
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            {
                let Ok(mut z) = std::fs::File::open(&dir)
                    .map_err(|e| e.to_string())
                    .and_then(|f| zip::ZipArchive::new(f).map_err(|e| e.to_string()))
                else {
                    continue;
                };
                for i in 0..z.len() {
                    let Ok(mut f) = z.by_index(i) else { continue };
                    if f.is_dir() {
                        continue;
                    }
                    let Ok(name) = f.name().map(|n| n.replace('\\', "/")) else {
                        continue;
                    };
                    // (a bus zipped without its Vehicles folder is read as one)
                    let top = name.split('/').next().unwrap_or("").to_ascii_lowercase();
                    let rel = if super::super::FOLDERS.contains(&top.as_str()) {
                        name.clone()
                    } else {
                        format!("Vehicles/{name}")
                    };
                    let mut data = Vec::new();
                    if std::io::Read::read_to_end(&mut f, &mut data).is_ok() {
                        look(&rel, &data, &mut refused);
                    }
                }
                continue;
            }
            fn walk(d: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
                for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                    let p = e.path();
                    // (through junctions too: `metadata` follows them)
                    match std::fs::metadata(&p) {
                        Ok(m) if m.is_dir() => walk(&p, out),
                        Ok(_) => out.push(p),
                        Err(_) => {}
                    }
                }
            }
            let mut all = Vec::new();
            walk(&dir, &mut all);
            for path in all {
                let Ok(sub) = path.strip_prefix(&dir) else {
                    continue;
                };
                let sub = sub.to_string_lossy().replace('\\', "/");
                let rel = match &prefix {
                    Some(p) => format!("{p}/{sub}"),
                    None => sub,
                };
                let top = rel.split('/').next().unwrap_or("").to_ascii_lowercase();
                if !super::super::FOLDERS.contains(&top.as_str()) {
                    continue;
                }
                if let Ok(data) = std::fs::read(&path) {
                    look(&rel, &data, &mut refused);
                }
            }
            eprintln!("{t}: done");
        }
        let mut report = format!(
            "{files} files, {:.1} GB looked at; refused: {}\n",
            bytes as f64 / 1e9,
            refused.values().map(Vec::len).sum::<usize>()
        );
        for (reason, list) in &refused {
            report += &format!("\n## {reason} ({})\n", list.len());
            for l in list {
                report += &format!("  {l}\n");
            }
        }
        let out =
            std::env::var("QUARANTINE_SCAN_OUT").unwrap_or_else(|_| "quarantine-scan.txt".into());
        std::fs::write(&out, &report).unwrap();
        eprintln!("{}", report.lines().next().unwrap_or(""));
    }
}
