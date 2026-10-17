//! The paper timetable in the driver's cab.
//!
//! Stock vehicle models show this through `[matl_freetex] file_schedule`. OMSI supplies the
//! bitmap named by that string; openOMSI makes it from the player's current duty and keeps
//! it in its own cache so the original installation remains read-only.

use omsi_sim::timetable_run::PlayerDuty;
use ab_glyph::{Font as _, FontVec, PxScale, ScaleFont};
use anyhow::{anyhow, Context, Result};
use omsi_content::font::{Font, FontAtlas, FontChar, TextAlign};
use omsi_sim::VehicleInstance;
use omsi_texture::Image;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// Omsi.exe (0x7e72a0) writes the paper as one text with DrawTextW into the rectangle from
// (60, 90) to the bitmap's right edge, in its typewriter font (Courier New) bold at 16 pt.
const PAPER_X: u32 = 60;
const PAPER_TOP: u32 = 90;
/// 16 pt at 96 DPI: the font's em in pixels (`-MulDiv(16, 96, 72)`).
const FONT_EM: f32 = 21.0;
/// A label is cut, or filled with dots, to this many characters.
const LABEL_CHARS: usize = 25;
/// Rows of the first column; the next 24 make the second, the rest are left out.
const ROWS_PER_COLUMN: usize = 24;
const TEXT_COLOR: [u8; 3] = [17, 15, 14];

#[derive(Debug, Clone, PartialEq, Eq)]
struct PaperRow {
    name: String,
    time: String,
}

/// Update `file_schedule` to a cached drawing of the current trip. `language`: the game's
/// language (`DEU`, `ENG`...), asked once, the first time a paper is drawn. The renderer already
/// handles the model's `[matl_freetex]` slot, so switching this string updates the paper.
pub fn update_vehicle(
    vehicle: &mut VehicleInstance,
    duty: &PlayerDuty,
    fonts: &mut omsi_sim::texttex::FontLibrary,
    language: impl FnOnce() -> String,
) -> Result<()> {
    // A model without the string has no paper to show: nothing to draw, nothing to look
    // for on the disk (this ran, file check included, on every frame of such a bus).
    let Some(var) = vehicle.ty.program.str_var("file_schedule") else {
        return Ok(());
    };
    let var = var as usize;
    // The paper changes with the trip only: while what it is drawn from and the name the
    // bus shows are as they were last time, the text is not set again every frame.
    static LAST: std::sync::Mutex<Option<(u64, String)>> = std::sync::Mutex::new(None);
    let key = input_key(duty);
    if let Ok(last) = LAST.lock() {
        if let Some((k, name)) = last.as_ref() {
            if *k == key && vehicle.state.str_vars.get(var) == Some(name) {
                return Ok(());
            }
        }
    }
    let (arr, dep) = tt_labels(language);
    let (title, rows) = paper_content(&duty.line, &duty.tour, &duty.trips, duty.trip_index, (arr, dep));
    let lines = paper_lines(&title, &rows);
    let signature = content_signature(&lines);
    let path = cache_dir()?.join(format!("schedule-v7-{signature:016x}.png"));
    let filename = path.to_string_lossy().into_owned();

    if vehicle.state.str_vars.get(var) == Some(&filename) {
        remember(&LAST, key, filename);
        return Ok(());
    }

    if !path.is_file() {
        let Some(font) = schedule_font(fonts, &lines) else {
            set_filename(vehicle, "");
            return Err(anyhow!(
                "no OMSI bitmap font is available for the driver's timetable"
            ));
        };
        let mut image = paper_base();
        draw_schedule(&mut image, &font, &lines);
        save_png(&image, &path)?;
    }
    set_filename(vehicle, &filename);
    remember(&LAST, key, filename);
    Ok(())
}

fn remember(last: &std::sync::Mutex<Option<(u64, String)>>, key: u64, filename: String) {
    if let Ok(mut last) = last.lock() {
        *last = Some((key, filename));
    }
}

/// Everything `paper_content` reads of the duty, hashed (the labels are fixed for the run).
fn input_key(duty: &PlayerDuty) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    duty.line.hash(&mut hasher);
    duty.tour.hash(&mut hasher);
    duty.trip_index.hash(&mut hasher);
    if let Some(trip) = duty.trips.get(duty.trip_index) {
        trip.line.hash(&mut hasher);
        trip.terminus.hash(&mut hasher);
        for stop in &trip.stops {
            stop.name.hash(&mut hasher);
            stop.stops.hash(&mut hasher);
            stop.arr.to_bits().hash(&mut hasher);
            stop.dep.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn cache_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("cannot locate the openOMSI user data folder"))?;
    let dir = home.join(".openomsi").join("cache").join("schedules");
    // (the bus is handed the sheet by its full path, outside the content folders)
    omsi_texture::trust_generated_dir(&dir);
    Ok(dir)
}

fn set_filename(vehicle: &mut VehicleInstance, value: &str) {
    if let Some(i) = vehicle.ty.program.str_var("file_schedule") {
        vehicle.state.str_vars[i as usize] = value.to_string();
    } else {
        log::debug!(
            "{} has no file_schedule string variable",
            vehicle.ty.def.type_name
        );
    }
}

fn schedule_font(
    fonts: &mut omsi_sim::texttex::FontLibrary,
    lines: &[String],
) -> Option<std::sync::Arc<FontAtlas>> {
    typewriter_font(lines)
        .map(std::sync::Arc::new)
        .or_else(|| {
            ["19_HHAschedule_font", "DIN Narrow", "DIN_Narrow", "DIN"]
                .into_iter()
                .find_map(|name| fonts.load(name))
        })
}

/// OMSI prints the schedule in a bold typewriter face, rather than a bus display's
/// proportional bitmap font. Rasterize an installed equivalent into fixed-width cells.
fn typewriter_font(lines: &[String]) -> Option<FontAtlas> {
    static FONT: OnceLock<Option<FontVec>> = OnceLock::new();
    let font = FONT
        .get_or_init(|| {
            #[cfg(target_os = "windows")]
            let paths = [std::env::var_os("WINDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("C:/Windows"))
                .join("Fonts/courbd.ttf")];
            #[cfg(target_os = "macos")]
            let paths = [PathBuf::from(
                "/System/Library/Fonts/Supplemental/Courier New Bold.ttf",
            )];
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            let paths = linux_typewriter_fonts();
            paths
                .iter()
                .find_map(|path| FontVec::try_from_vec(std::fs::read(path).ok()?).ok())
        })
        .as_ref()?;
    // GDI sizes a font by its em; ab_glyph by ascent - descent
    let units = font.units_per_em().unwrap_or(2048.0);
    let px = PxScale::from(FONT_EM * font.height_unscaled() / units);
    let scaled = font.as_scaled(px);
    // a line is Courier New's height as GDI makes it, the ascent and the descent each
    // rounded to whole pixels (bold at 16 pt: 17.5 + 6.3 = 17 + 6 = 23 pixels), the baseline
    // at the ascent. The sum rounded made it 24, and the rows ran down past the paper's
    // lines, one pixel more with every row (#629). The cells are Courier New's too, whatever
    // face draws the letters: a Linux without it took DejaVu Sans Mono's taller lines and
    // the rows ran off the paper (#1878).
    let k = FONT_EM / COURIER_UNITS;
    let line_height = gdi_line_height(COURIER_ASCENT * k, -COURIER_DESCENT * k);
    let cell_width = (COURIER_ADVANCE * k).round().max(1.0) as u32;
    let baseline = (COURIER_ASCENT * k).round();
    let mut characters: Vec<char> = (32u8..=126).map(char::from).collect();
    for line in lines {
        characters.extend(line.chars());
    }
    characters.sort_unstable();
    characters.dedup();
    characters.retain(|&ch| scaled.glyph_id(ch).0 != 0);
    let width = cell_width * characters.len() as u32;
    let mut alpha = vec![0; (width * line_height * 4) as usize];
    let mut chars = Vec::with_capacity(characters.len());
    for (index, ch) in characters.into_iter().enumerate() {
        let x0 = index as u32 * cell_width;
        chars.push(FontChar {
            ch,
            x0: x0 as i32,
            x1: (x0 + cell_width) as i32,
            y: 0,
        });
        let glyph = scaled
            .glyph_id(ch)
            .with_scale_and_position(px, ab_glyph::point(0.0, baseline));
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|px, py, coverage| {
                let x = px as i32 + bounds.min.x as i32;
                let y = py as i32 + bounds.min.y as i32;
                if x >= 0 && x < cell_width as i32 && y >= 0 && y < line_height as i32 {
                    let offset = ((y as u32 * width + x0 + x as u32) * 4) as usize;
                    let a = (coverage * 255.0).round() as u8;
                    alpha[offset..offset + 4].fill(a);
                }
            });
        }
    }
    Some(FontAtlas::new(
        Font {
            name: "openOMSI timetable".into(),
            height: line_height as i32,
            gap: 0,
            chars,
            ..Default::default()
        },
        width,
        line_height,
        vec![0; alpha.len()],
        alpha,
    ))
}

/// Courier New Bold's metrics in its own units: the em, the ascent, the descent and every
/// letter's advance (a typewriter face: all alike).
const COURIER_UNITS: f32 = 2048.0;
const COURIER_ASCENT: f32 = 1705.0;
const COURIER_DESCENT: f32 = 615.0;
const COURIER_ADVANCE: f32 = 1229.0;

/// Where a Linux keeps a bold typewriter face, best first: Courier New itself (the Microsoft
/// core fonts), then the faces drawn to its measures (Liberation Mono, Cousine, Nimbus Mono),
/// then any bold monospace one. Distributions file them under different folders, and the
/// player's own fonts count too: looked up by file name in the usual font folders.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn linux_typewriter_fonts() -> Vec<PathBuf> {
    const NAMES: [&str; 9] = [
        "courbd.ttf",
        "courier new bold.ttf",
        "courier_new_bold.ttf",
        "liberationmono-bold.ttf",
        "cousine-bold.ttf",
        "nimbusmonops-bold.otf",
        "nimbusmonl-bold.otf",
        "freemonobold.ttf",
        "dejavusansmono-bold.ttf",
    ];
    let mut dirs = vec![PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts")];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join(".fonts"));
    }
    let mut found: Vec<(usize, PathBuf)> = Vec::new();
    let mut stack: Vec<(PathBuf, u32)> = dirs.into_iter().map(|d| (d, 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                if depth < 4 {
                    stack.push((path, depth + 1));
                }
            } else if let Some(rank) = path.file_name().and_then(|n| n.to_str()).and_then(|n| NAMES.iter().position(|w| n.eq_ignore_ascii_case(w))) {
                found.push((rank, path));
            }
        }
    }
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// GDI's `tmHeight` from a font's ascent and (negative) descent in pixels: each is rounded
/// by itself.
fn gdi_line_height(ascent: f32, descent: f32) -> u32 {
    (ascent.round() + (-descent).round()).max(1.0) as u32
}

/// `TT_Arr` and `TT_Dep` of the game's language (`Languages/<LANG>_basic.olf`), as
/// Omsi.exe translates them for the paper: "Arrival " and "Depart." in English.
fn tt_labels(language: impl FnOnce() -> String) -> &'static (String, String) {
    static LABELS: OnceLock<(String, String)> = OnceLock::new();
    LABELS.get_or_init(|| {
        let lang = language();
        let find = |lang: &str| {
            omsi_cfg::content_dirs("Languages").into_iter().find_map(|dir| {
                omsi_content::language::Language::load(&dir.join(format!("{lang}_basic.olf"))).ok()
            })
        };
        let l = find(&lang).or_else(|| find("ENG"));
        let get = |key: &str, default: &str| {
            l.as_ref()
                .and_then(|l| l.strings.get(key))
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| default.to_string())
        };
        (get("TT_Arr", "Arrival "), get("TT_Dep", "Depart. "))
    })
}

/// The rows of the paper as Omsi.exe makes them (0x7e72a0): every stop of the trip with
/// its departure (a station the profile passes is left out), the last one as
/// "<stop> <TT_Arr>" with its arrival, then one more row "<TT_Dep>" with the last stop's
/// departure.
fn paper_content(
    duty_line: &str,
    duty_tour: &str,
    trips: &[omsi_sim::timetable_run::PlannedTrip],
    trip_index: usize,
    (arr_label, dep_label): (&str, &str),
) -> (String, Vec<PaperRow>) {
    let trip = &trips[trip_index];
    let line = if trip.line.trim().is_empty() {
        duty_line.trim()
    } else {
        trip.line.trim()
    };
    let title = format!("{line} - {} - {}", trip.terminus.trim(), duty_tour.trim());

    let mut rows = Vec::new();
    let Some(last) = trip.stops.len().checked_sub(1) else {
        return (title, rows);
    };
    for i in 0..=last + 1 {
        let stop = &trip.stops[i.min(last)];
        if i < last && !stop.stops {
            continue;
        }
        // (the station's name, a blank and its second name - empty for a map's bus stop -
        // and another blank)
        let name = if i > last {
            dep_label.to_string()
        } else if i == last {
            format!("{}  {arr_label}", stop.name.trim())
        } else {
            format!("{}  ", stop.name.trim())
        };
        let time = format_time(if i == last { stop.arr } else { stop.dep });
        rows.push(PaperRow { name, time });
    }
    (title, rows)
}

fn format_time(seconds: f64) -> String {
    let minute = (seconds / 60.0).round() as i64;
    let minute = minute.rem_euclid(24 * 60);
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// The text Omsi.exe draws: the title, a rule of 67 '=', then the rows - each the label
/// cut or filled with dots to 25 characters, a blank and the time; rows 25 to 48 stand
/// beside the first 24, five blanks apart.
fn paper_lines(title: &str, rows: &[PaperRow]) -> Vec<String> {
    let cell = |row: &PaperRow| {
        let mut label: String = row.name.chars().take(LABEL_CHARS).collect();
        let n = label.chars().count();
        label.extend(std::iter::repeat_n('.', LABEL_CHARS - n));
        format!("{label} {}", row.time)
    };
    let (a, b) = rows.split_at(rows.len().min(ROWS_PER_COLUMN));
    let b = &b[..b.len().min(ROWS_PER_COLUMN)];
    let mut lines = vec![title.to_string(), "=".repeat(67)];
    for (i, row) in a.iter().enumerate() {
        let mut line = cell(row);
        if let Some(other) = b.get(i) {
            line.push_str("     ");
            line.push_str(&cell(other));
        }
        lines.push(line);
    }
    lines
}

fn content_signature(lines: &[String]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    lines.hash(&mut hasher);
    hasher.finish()
}

fn paper_base() -> Image {
    let dirs = omsi_cfg::content_dirs("Texture");
    let refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    omsi_texture::find_texture("Schedule.bmp", &refs)
        .and_then(|path| omsi_texture::decode_file(&path).ok())
        .unwrap_or_else(|| Image {
            width: 1024,
            height: 1024,
            rgba: [230, 227, 216, 255]
                .into_iter()
                .cycle()
                .take(1024 * 1024 * 4)
                .collect(),
            has_alpha: false,
        })
}

/// Draw the lines from (60, 90) down, one font height apart, as DrawTextW does; what
/// runs off the bitmap is cut off.
fn draw_schedule(image: &mut Image, font: &FontAtlas, lines: &[String]) {
    let line_height = font.font.height.max(1) as u32;
    for (i, line) in lines.iter().enumerate() {
        draw_text(image, font, line, PAPER_X, PAPER_TOP + i as u32 * line_height);
    }
}

fn draw_text(image: &mut Image, font: &FontAtlas, text: &str, x: u32, y: u32) {
    if x >= image.width || y >= image.height || text.is_empty() {
        return;
    }
    let source_width = (font.text_width(text).max(0) as u32).max(1);
    let source_height = font.font.height.max(1) as u32;
    let rgba = font.render_aligned(
        text,
        source_width,
        source_height,
        false,
        TEXT_COLOR,
        TextAlign {
            orientation: 1,
            grid: 1,
        },
    );
    let width = source_width.min(image.width - x);
    let height = source_height.min(image.height - y);
    for py in 0..height {
        for px in 0..width {
            let alpha = rgba[((py * source_width + px) * 4 + 3) as usize] as u32;
            if alpha == 0 {
                continue;
            }
            let target = (((y + py) * image.width + x + px) * 4) as usize;
            for channel in 0..3 {
                let ink = TEXT_COLOR[channel] as u32;
                let paper = image.rgba[target + channel] as u32;
                image.rgba[target + channel] = ((ink * alpha + paper * (255 - alpha)) / 255) as u8;
            }
            image.rgba[target + 3] = 255;
        }
    }
}

fn save_png(image: &Image, path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("schedule cache path has no parent"))?;
    std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let buffer = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
        .ok_or_else(|| anyhow!("schedule image has the wrong pixel count"))?;
    buffer
        .save(path)
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_sim::timetable_run::{PlannedStop, PlannedTrip, StopDir};

    const FONT_HEIGHT: u32 = 24;
    use omsi_content::font::FontChar;

    fn stop(id: i64, name: &str, arr: f64, dep: f64) -> PlannedStop {
        PlannedStop {
            object_id: id,
            name: name.into(),
            arr,
            dep,
            position: None,
            dir: StopDir::default(),
            stops: true,
            closed: None,
        }
    }

    fn test_font() -> FontAtlas {
        let chars: Vec<_> = (32u8..=126)
            .enumerate()
            .map(|(index, byte)| FontChar {
                ch: byte as char,
                x0: index as i32 * 4,
                x1: (index as i32 + 1) * 4,
                y: 0,
            })
            .collect();
        let width = chars.len() as u32 * 4;
        let pixels = vec![255; (width * FONT_HEIGHT * 4) as usize];
        FontAtlas::new(
            omsi_content::font::Font {
                height: FONT_HEIGHT as i32,
                gap: 1,
                chars,
                ..Default::default()
            },
            width,
            FONT_HEIGHT,
            pixels.clone(),
            pixels,
        )
    }

    #[test]
    fn a_line_is_as_high_as_gdi_makes_it() {
        // Courier New bold at 16 pt (an em of 21 px): ascent 1705, descent 615 of 2048
        let k = 21.0 / 2048.0;
        assert_eq!(gdi_line_height(1705.0 * k, -615.0 * k), 23);
    }

    #[test]
    fn paper_rows_end_with_arrival_and_departure_of_the_last_stop() {
        let t = |h: f64, m: f64| h * 3600.0 + m * 60.0;
        let mut passed = stop(2, "Feld", t(11.0, 54.0), t(11.0, 54.0));
        passed.stops = false;
        let current = PlannedTrip {
            name: "76_Kk-BH".into(),
            line: "76".into(),
            terminus: "Bauernhof".into(),
            departure: t(11.0, 52.0),
            end: t(11.0, 59.0),
            stops: vec![
                stop(1, "Krankenhaus", t(11.0, 50.0), t(11.0, 52.0)),
                passed,
                stop(3, "Dorf", t(11.0, 55.0), t(11.0, 57.0)),
                stop(4, "Bauernhof", t(11.0, 59.0), t(12.0, 1.0)),
            ],
        };
        let (title, rows) = paper_content("76", "1", &[current], 0, ("Arrival ", "Depart. "));
        assert_eq!(title, "76 - Bauernhof - 1");
        assert_eq!(
            rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["Krankenhaus  ", "Dorf  ", "Bauernhof  Arrival ", "Depart. "]
        );
        assert_eq!(
            rows.iter().map(|r| r.time.as_str()).collect::<Vec<_>>(),
            ["11:52", "11:57", "11:59", "12:01"]
        );
    }

    #[test]
    fn paper_times_wrap_after_midnight() {
        assert_eq!(format_time(24.0 * 3600.0 + 7.0 * 60.0), "00:07");
    }

    #[test]
    fn paper_text_is_two_columns_of_25_character_labels() {
        let rows: Vec<_> = (0..50)
            .map(|i| PaperRow {
                name: if i == 0 { "Gustav-Adolf-Str./Langhansstr.  ".into() } else { format!("Stop {i:02}  ") },
                time: "12:34".into(),
            })
            .collect();
        let lines = paper_lines("76 - Bauernhof - 1", &rows);
        assert_eq!(lines.len(), 2 + 24);
        assert_eq!(lines[0], "76 - Bauernhof - 1");
        assert_eq!(lines[1], "=".repeat(67));
        assert_eq!(
            lines[2],
            "Gustav-Adolf-Str./Langhan 12:34     Stop 24  ................ 12:34"
        );
        assert_eq!(lines[25], "Stop 23  ................ 12:34     Stop 47  ................ 12:34");
        // rows past the second column are left out
        assert!(!lines.iter().any(|l| l.contains("Stop 48")));

        let short = paper_lines("t", &rows[1..3]);
        assert_eq!(short[2], "Stop 01  ................ 12:34");
        assert_eq!(short.len(), 4);
    }

    #[test]
    fn paper_lines_are_drawn_from_60_90_one_font_height_apart() {
        let font = test_font();
        let mut image = Image { width: 1024, height: 1024, rgba: vec![255; 1024 * 1024 * 4], has_alpha: false };
        draw_schedule(&mut image, &font, &["AB".to_string(), "CD".to_string()]);
        let ink = |x: u32, y: u32| image.rgba[((y * image.width + x) * 4) as usize] < 100;
        assert!(ink(PAPER_X, PAPER_TOP));
        assert!(ink(PAPER_X, PAPER_TOP + FONT_HEIGHT));
        assert!(!ink(PAPER_X - 1, PAPER_TOP));
        assert!(!ink(PAPER_X, PAPER_TOP - 1));
    }
}
