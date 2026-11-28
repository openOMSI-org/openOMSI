//! METAR retrieval and wire encoding for the dedicated host, no scenery/texture dependencies.

/// The weather of an airport's METAR report (aviationweather.gov), or a clear day when it
/// cannot be had (no network, an unknown station).
pub fn fetch_metar(icao: &str) -> omsi_content::weather::Weather {
    match try_metar(icao) {
        Some(w) => w,
        None => {
            log::warn!(
                "current weather at {icao}: no METAR report could be had; a clear day instead"
            );
            omsi_content::weather::from_metar(icao, "CAVOK")
        }
    }
}

/// The weather of an airport's METAR report, None when it cannot be had.
pub fn try_metar(icao: &str) -> Option<omsi_content::weather::Weather> {
    // (Tegel, OMSI's Berlin default, closed in 2020: Berlin's airport now reports)
    let icao = match icao.to_ascii_uppercase().as_str() {
        "EDDT" | "EDDI" | "" => "EDDB".to_string(),
        other => other.to_string(),
    };
    let url = format!("https://aviationweather.gov/api/data/metar?ids={icao}&format=raw");
    let text = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .map(|t| t.lines().next().unwrap_or("").trim().to_string())
        .filter(|t| !t.is_empty());
    let t = text?;
    log::info!("current weather at {icao}: {t}");
    let mut w = omsi_content::weather::from_metar(&icao, &t);
    w.path = std::path::PathBuf::from(format!("metar:{icao}"));
    Some(w)
}

/// The start of a weather text that carries a METAR report's values (not a file, not a
/// download): what a host or server with the METAR sync tells the players, who make the
/// weather from it themselves and need no sync of their own.
pub const REPORT: &str = "metar-report:";

/// `w` (made from a METAR report) as the text the session tells the players: the station and
/// the report up to its forecast, which `from_metar` does not read either.
pub fn report_wire(w: &omsi_content::weather::Weather) -> Option<String> {
    let path = w.path.to_string_lossy();
    let icao = path.strip_prefix("metar:")?;
    let mut raw: Vec<&str> = Vec::new();
    for t in w.description.split_whitespace() {
        if matches!(
            t.trim_end_matches('='),
            "TEMPO" | "BECMG" | "NOSIG" | "RMK" | "PROB30" | "PROB40"
        ) {
            break;
        }
        raw.push(t);
    }
    let raw = raw.join(" ");
    if icao.is_empty() || raw.is_empty() {
        return None;
    }
    // (the network's weather field holds 260 characters)
    Some(format!("{REPORT}{icao} {raw}").chars().take(250).collect())
}
