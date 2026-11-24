//! `.owt` weather presets (unit `mc_weather`).

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Weather {
    pub path: PathBuf,
    pub name: String,
    pub description: String,
    /// visibility (m), fog density
    pub fog: (f32, f32),
    pub wind: (f32, f32),
    /// temperature °C, absolute humidity (g/m³: `Weather_AbsHum`). A `.owt` file gives the
    /// dew point in its place, see [`absolute_humidity`].
    pub temp: (f32, f32),
    pub pressure: f32,
    pub clouds: (String, f32),
    /// How much of the sky the clouds cover, 0..1; `None` = the cloud type in `clouds.0`
    /// decides. A weather set by hand (`custom:`) sets it as one value from a clear sky to a
    /// closed one, rather than naming the nearest of the five types.
    pub cloud_cover: Option<f32>,
    pub precip: Vec<f32>,
    pub ground_wet: [f32; 3],
    pub snow: bool,
    pub snow_on_road: bool,
}

impl Weather {
    pub fn load(path: &Path) -> Result<Weather, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }
    pub fn parse(f: &CfgFile) -> Weather {
        let mut w = Weather {
            path: f.path.clone(),
            pressure: 1013.0,
            ..Default::default()
        };
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "name" => w.name = r.str().to_string(),
                "description" => w.description = r.until("[end]").join("\n"),
                "fog" => w.fog = (r.f32(), r.f32()),
                "wind" => w.wind = (r.f32(), r.f32()),
                "temp" => {
                    let t = r.f32();
                    w.temp = (t, absolute_humidity(t, r.f32()));
                }
                "press" => w.pressure = r.f32(),
                "clouds" => w.clouds = (r.str().to_string(), r.f32()),
                "precip" => w.precip = (0..5).map(|_| r.f32()).collect(),
                "groundwet" => w.ground_wet = r.f32s::<3>(),
                "snow" => w.snow = true,
                "snowonroad" => w.snow_on_road = true,
                _ => {}
            }
        }
        w
    }
}

/// The absolute humidity (g/m³) of air at `temp` °C with the dew point `dew` °C, as OMSI 2
/// makes `Weather_AbsHum` of a weather's `[temp]`: the second value there is the dew point
/// (Bodennebel 9/9 and Überfrierende Nässe -6/-6 are saturated, Starker Schneefall -6/-7.96),
/// not the humidity itself. Taken as it stood, a winter weather gave the scripts a negative
/// humidity - no breath of exhaust steam in the frost - and every weather a wrong one for the
/// heating's misting of the panes. The saturation pressure is Magnus's in base 10, over ice
/// below 0 °C (chosen by the temperature, not the dew point, as in the original).
pub fn absolute_humidity(temp: f32, dew: f32) -> f32 {
    let (a, b) = if temp < 0.0 {
        (7.6, 240.7)
    } else {
        (7.5, 237.3)
    };
    6.1078 * 216.69 * 10f32.powf(a * dew / (b + dew)) / (273.15 + temp)
}

/// OMSI 2's current weather (`[currWeather_ICAO]`): a weather made from an airport's METAR
/// report - visibility (m, CAVOK and statute miles too), wind, temperature with the
/// humidity from the dew point, QNH, the cloud cover (the lowest layer that covers most)
/// as one of OMSI's cloud types, and rain, drizzle or snow with their strength.
pub fn from_metar(station: &str, text: &str) -> Weather {
    let mut w = Weather {
        name: format!("METAR {station}"),
        description: text.trim().to_string(),
        fog: (50_000.0, 1.0),
        temp: (15.0, 8.0),
        pressure: 1013.0,
        clouds: ("-1".into(), 0.0),
        precip: vec![0.0, 32.0, 0.0, 0.0, 0.0],
        ..Default::default()
    };
    let mut cover_rank = 0;
    let mut precip: Option<(f32, f32)> = None;
    for tok in text.split_whitespace() {
        let t = tok.trim_end_matches('=');
        // what follows is a forecast or remarks, not the weather now
        if matches!(t, "TEMPO" | "BECMG" | "NOSIG" | "RMK" | "PROB30" | "PROB40") {
            break;
        }
        if t == "CAVOK" || t == "NSC" || t == "SKC" || t == "CLR" {
            if t == "CAVOK" {
                w.fog.0 = 50_000.0;
            }
            continue;
        }
        // wind: dddff(Gff)KT / MPS
        if (t.ends_with("KT") || t.ends_with("MPS")) && t.len() >= 7 {
            let dir = t[..3].parse::<f32>().unwrap_or(0.0);
            let spd: f32 = t[3..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0.0);
            w.wind = (dir, if t.ends_with("KT") { spd * 0.514 } else { spd });
            continue;
        }
        // visibility in metres, or statute miles
        if t.len() == 4 && t.chars().all(|c| c.is_ascii_digit()) {
            let v: f32 = t.parse().unwrap_or(9999.0);
            w.fog.0 = if v >= 9999.0 { 50_000.0 } else { v.max(50.0) };
            continue;
        }
        if let Some(sm) = t.strip_suffix("SM") {
            if let Ok(v) = sm.parse::<f32>() {
                w.fog.0 = if v >= 10.0 {
                    50_000.0
                } else {
                    (v * 1609.0).max(50.0)
                };
            }
            continue;
        }
        // temperature / dew point (M = minus)
        if let Some((a, b)) = t.split_once('/') {
            let num = |s: &str| -> Option<f32> {
                let (neg, s) = s.strip_prefix('M').map(|s| (true, s)).unwrap_or((false, s));
                s.parse::<f32>().ok().map(|v| if neg { -v } else { v })
            };
            if let (Some(tc), Some(td)) = (num(a), num(b)) {
                if a.len() <= 3 && b.len() <= 3 {
                    w.temp = (tc, absolute_humidity(tc, td));
                }
            }
            continue;
        }
        if let Some(q) = t.strip_prefix('Q').and_then(|q| q.parse::<f32>().ok()) {
            w.pressure = q;
            continue;
        }
        if let Some(a) = t
            .strip_prefix('A')
            .filter(|a| a.len() == 4)
            .and_then(|a| a.parse::<f32>().ok())
        {
            w.pressure = a / 100.0 * 33.8639;
            continue;
        }
        // clouds: the densest layer (FEW < SCT < BKN < OVC), its base in feet
        for (code, rank) in [("FEW", 1), ("SCT", 2), ("BKN", 3), ("OVC", 4), ("VV", 4)] {
            if let Some(h) = t.strip_prefix(code) {
                let base = h
                    .chars()
                    .take(3)
                    .collect::<String>()
                    .parse::<f32>()
                    .map(|x| x * 30.48)
                    .unwrap_or(600.0);
                if rank > cover_rank {
                    cover_rank = rank;
                    let kind = match rank {
                        1 => "Cumulus 1",
                        2 => "Cumulus 2",
                        3 => "Cumulus 3",
                        _ => "Overcast 1",
                    };
                    w.clouds = (kind.to_string(), base.max(50.0));
                }
            }
        }
        // precipitation: -/+ strength, RA DZ SN SG PL GR, with showers and thunder
        let (strength, rest) = match t.chars().next() {
            Some('-') => (70.0, &t[1..]),
            Some('+') => (230.0, &t[1..]),
            _ => (150.0, t),
        };
        let rest = rest
            .trim_start_matches("VC")
            .trim_start_matches("SH")
            .trim_start_matches("TS")
            .trim_start_matches("FZ");
        let kind = if rest.starts_with("SN") || rest.starts_with("SG") || rest.starts_with("PL") {
            Some(2.0)
        } else if rest.starts_with("RA")
            || rest.starts_with("DZ")
            || rest.starts_with("GR")
            || rest.starts_with("GS")
        {
            Some(1.0)
        } else {
            None
        };
        if let Some(k) = kind {
            let s = if rest.starts_with("DZ") {
                strength * 0.5
            } else {
                strength
            };
            if precip.map(|p| s > p.1).unwrap_or(true) {
                precip = Some((k, s));
            }
        }
        if rest == "FG" && w.fog.0 > 1000.0 {
            w.fog.0 = 800.0;
        }
    }
    if let Some((k, s)) = precip {
        w.precip = vec![k, s, 0.0, 0.0, 0.0];
        if k == 2.0 {
            w.snow = w.temp.0 < 2.0;
        }
        w.ground_wet = [if k == 1.0 { 80.0 } else { 20.0 }, 255.0, 115.0];
    }
    w
}

#[cfg(test)]
mod humidity_tests {
    use super::absolute_humidity;

    #[test]
    fn the_second_temp_value_is_the_dew_point() {
        // saturated air holds about 8.8 g/m³ at 9 °C, about 3.2 at -6 °C
        assert!((absolute_humidity(9.0, 9.0) - 8.8).abs() < 0.1);
        assert!((absolute_humidity(-6.0, -6.0) - 3.2).abs() < 0.1);
        // a dew point below zero is still some humidity, never less than none
        let snow = absolute_humidity(-6.0, -7.96);
        assert!(snow > 2.0 && snow < 3.0, "{snow}");
        let cfg = omsi_cfg::CfgFile::from_str("x.owt", "[temp]\r\n-6\r\n-7.96\r\n");
        let w = super::Weather::parse(&cfg);
        assert_eq!(w.temp, (-6.0, snow));
    }
}

#[cfg(test)]
mod metar_tests {
    #[test]
    fn berlin_rain() {
        let w = super::from_metar(
            "EDDT",
            "EDDT 241250Z 23008KT 4000 -RA BKN012 OVC030 14/12 Q0997",
        );
        assert_eq!(w.wind.0, 230.0);
        assert!((w.wind.1 - 4.1).abs() < 0.1);
        assert_eq!(w.fog.0, 4000.0);
        assert_eq!(w.clouds.0, "Overcast 1");
        assert_eq!(w.precip[0], 1.0);
        assert_eq!(w.pressure, 997.0);
        assert_eq!(w.temp.0, 14.0);
        let c = super::from_metar(
            "EDDT",
            "EDDT 241250Z VRB02KT CAVOK M03/M09 Q1030 TEMPO 4000 -RA BKN009",
        );
        assert_eq!(c.clouds.0, "-1");
        assert_eq!(c.temp.0, -3.0);
        let s = super::from_metar(
            "EDDT",
            "EDDT 241250Z 30012G25KT 1200 +SN VV008 M02/M03 Q1005",
        );
        assert!(s.snow && s.precip[0] == 2.0 && s.fog.0 == 1200.0);
    }
}
