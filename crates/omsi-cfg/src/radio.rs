use std::path::PathBuf;

pub const DEFAULT_STATIONS: &[(&str, &str)] = &[
    ("radioeins", "https://dispatcher.rndfnk.com/rbb/radioeins/live/mp3/mid"),
    ("Berlin 88.8", "https://dispatcher.rndfnk.com/rbb/rbb888/live/mp3/mid"),
    ("Deutschlandfunk", "https://st01.sslstream.dlf.de/dlf/01/128/mp3/stream.mp3"),
    ("SWR3", "https://liveradio.swr.de/sw282p3/swr3/play.mp3"),
    ("Европа Плюс", "https://ep256.hostingradio.ru:8052/europaplus256.mp3"),
    ("Русское Радио", "https://rusradio.hostingradio.ru/rusradio128.mp3"),
    ("Ретро FM", "https://retro.hostingradio.ru:8043/retro256.mp3"),
    ("Наше Радио", "https://nashe1.hostingradio.ru:80/nashe-128.mp3"),
    ("Radio Paradise", "https://stream.radioparadise.com/mp3-128"),
];

/// `radio.cfg` as it is written the first time: what it is for, the volume, the default list.
pub fn default_text() -> String {
    let mut text = String::from(
        "# Internet radio for the buses' radios: one station per line, `name = address`\n\
         # (an MP3, AAC or Ogg stream, or an .m3u/.pls playlist). A radio's station\n\
         # button n plays the n-th station, a cassette player the first; Shift+R in the\n\
         # game steps through the list. volume = 0..1.\n\
         volume = 0.7\n",
    );
    for (n, u) in DEFAULT_STATIONS {
        text.push_str(&format!("{n} = {u}\n"));
    }
    text
}

/// The station lines of a radio.cfg as they stand: (name, all behind the `=` - the address
/// and any frequencies after it).
pub fn station_lines(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(|l| l.trim().trim_start_matches('\u{feff}'))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .filter(|(n, v)| !n.eq_ignore_ascii_case("volume") && !v.split('|').next().unwrap_or("").trim().is_empty())
        .collect()
}

/// `text` (a radio.cfg) with `list` for its stations: its comments and volume stay as they
/// are, the stations stand where its first one stood (after the rest when it had none). A
/// station without an address is left out, and a name loses what would end it or make the
/// line a comment (`=`, a line break, a leading `#`).
pub fn with_stations(text: &str, list: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut placed = false;
    let put = |out: &mut String| {
        let mut k = 0;
        for (name, value) in list {
            let value = value.replace(['\r', '\n'], " ");
            if value.split('|').next().unwrap_or("").trim().is_empty() {
                continue;
            }
            let name = name.replace(['=', '\r', '\n'], " ");
            let name = name.trim().trim_start_matches('#').trim();
            k += 1;
            let name = if name.is_empty() || name.eq_ignore_ascii_case("volume") { format!("Station {k}") } else { name.to_string() };
            out.push_str(&format!("{name} = {}\n", value.trim()));
        }
    };
    for line in text.lines() {
        if !station_lines(line).is_empty() {
            if !placed {
                put(&mut out);
                placed = true;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !placed {
        put(&mut out);
    }
    out
}

/// The player's own stations (`radio.cfg`; the default list while there is none), as the
/// launcher's Sound settings show them to edit (#857).
pub fn own_stations() -> Vec<(String, String)> {
    station_lines(&config_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_else(default_text))
}

/// Keep `list` as the player's own stations in `radio.cfg` (see `with_stations`): the game
/// tunes in to them when it starts.
pub fn save_stations(list: &[(String, String)]) -> std::io::Result<()> {
    let Some(p) = config_path() else { return Err(std::io::Error::other("no home folder")) };
    let text = match std::fs::read_to_string(&p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => default_text(),
        Err(e) => return Err(e),
    };
    std::fs::create_dir_all(p.parent().unwrap_or(&p))?;
    let new = p.with_extension("cfg.new");
    std::fs::write(&new, with_stations(&text, list))?;
    std::fs::rename(&new, &p)
}

pub fn config_path() -> Option<PathBuf> {
    Some(crate::user_data::base_dir()?.join("radio.cfg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unnamed_stations_are_numbered_as_saved_and_none_is_taken_for_the_volume() {
        let list = [("".to_string(), "".to_string()), ("".to_string(), "https://a.example/a.mp3".to_string()), ("Volume".to_string(), "https://b.example/b.mp3".to_string())];
        assert_eq!(with_stations("volume = 0.4\n", &list), "volume = 0.4\nStation 1 = https://a.example/a.mp3\nStation 2 = https://b.example/b.mp3\n");
    }
}
