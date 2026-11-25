//! `.odr` driver profiles (unit `mc_driver`).

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Driver {
    pub path: PathBuf,
    pub name: String,
    pub sex: String,
    pub birth_date: i32,
    pub employ_date: i32,
    /// Stops served and, of those, the ones left too early and the ones reached too late.
    /// The file keeps the last two the other way round (see `save`).
    pub bus_stops: [i32; 3],
    pub hektom: f64,
    pub crashes: [i32; 4],
    pub tickets: [f64; 2],
    pub rating: [f64; 5],
    /// Hectometres driven in each bus (`[perbusinfo]`, one block per bus): the bus as its
    /// `[friendlyname]` names it, manufacturer and type with a space between.
    pub per_bus: Vec<(String, i64)>,
}

impl Driver {
    /// Write the personnel file the way OMSI does (UTF-16 LE with a BOM, CR LF).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut t = String::from(
            "-----------------------\r\nDriver File\r\n-----------------------\r\n\r\n",
        );
        t.push_str("Created with openOMSI\r\n\r\n");
        t.push_str(&format!(
            "[ident]\r\n{}\r\n{}\r\n{}\r\n{}\r\n\r\n",
            self.name,
            if self.sex.is_empty() { "M" } else { &self.sex },
            self.birth_date,
            self.employ_date
        ));
        // served, late, early: the order of Omsi.exe's TDriver record (cnt_busstop_all,
        // cnt_busstop_late, cnt_busstop_early), which tools such as Busbetrieb-Simulator read
        t.push_str(&format!(
            "[busstops]\r\n{}\r\n{}\r\n{}\r\n\r\n",
            self.bus_stops[0], self.bus_stops[2], self.bus_stops[1]
        ));
        t.push_str(&format!("[hektom]\r\n{:.0}\r\n\r\n", self.hektom));
        t.push_str(&format!(
            "[crashs]\r\n{}\r\n{}\r\n{}\r\n{}\r\n\r\n",
            self.crashes[0], self.crashes[1], self.crashes[2], self.crashes[3]
        ));
        t.push_str(&format!(
            "[tickets]\r\n{:.0}\r\n{:.6}\r\n\r\n",
            self.tickets[0], self.tickets[1]
        ));
        t.push_str("[rating]\r\n");
        for v in &self.rating {
            t.push_str(&format!("{v:.6}\r\n"));
        }
        // (as Omsi.exe's SaveAllDriver 0x746aec: a block per bus driven, none for a bus at 0)
        for (bus, hm) in self.per_bus.iter().filter(|(_, hm)| *hm != 0) {
            t.push_str(&format!("\r\n[perbusinfo]\r\n{bus}\r\n{hm}\r\n"));
        }
        let mut bytes = vec![0xFF, 0xFE];
        for u in t.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        std::fs::write(path, bytes)
    }

    pub fn load(path: &Path) -> Result<Driver, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut d = Driver {
            path: f.path.clone(),
            ..Default::default()
        };
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "ident" => {
                    d.name = r.str().to_string();
                    d.sex = r.str().to_string();
                    d.birth_date = r.i32();
                    d.employ_date = r.i32();
                }
                "busstops" => {
                    let [all, late, early] = [r.i32(), r.i32(), r.i32()];
                    d.bus_stops = [all, early, late];
                }
                "hektom" => d.hektom = r.f64(),
                "crashs" => d.crashes = [r.i32(), r.i32(), r.i32(), r.i32()],
                "tickets" => d.tickets = r.f64s::<2>(),
                "rating" => d.rating = r.f64s::<5>(),
                "perbusinfo" => {
                    let bus = r.str().to_string();
                    let hm = r.f64().round() as i64;
                    d.add_bus_hektom(&bus, hm);
                }
                _ => {}
            }
        }
        d.check_ratings();
        Ok(d)
    }

    /// Add `hm` hectometres to the bus `bus` (manufacturer and type, see `per_bus`).
    pub fn add_bus_hektom(&mut self, bus: &str, hm: i64) {
        if bus.trim().is_empty() {
            return;
        }
        match self.per_bus.iter_mut().find(|(b, _)| b == bus) {
            Some((_, v)) => *v += hm,
            None => self.per_bus.push((bus.to_string(), hm)),
        }
    }

    /// The `[rating]` block is OMSI's driver record from +0x40 on (the original,
    /// the original): the driving penalty P (0..1), passengers who stepped in
    /// without a complaint, tickets asked for, points for selling them (2 for the right
    /// change, 1 for the wrong), passengers who stepped in. Values that cannot be those (an
    /// older openOMSI file kept averages there) are set back to zero.
    fn check_ratings(&mut self) {
        let [p, content, asked, points, stepped] = self.rating;
        let ok = (0.0..=1.0).contains(&p)
            && content >= 0.0
            && content <= stepped
            && asked >= 0.0
            && points >= 0.0
            && points <= 2.0 * asked + 1e-9
            && self.rating.iter().all(|v| v.is_finite());
        if !ok {
            log::info!(
                "personnel file {}: ratings {:?} are not Omsi.exe's counters; started afresh",
                self.path.display(),
                self.rating
            );
            self.rating = [0.0; 5];
        }
    }

    /// Driving as the personnel dialog shows it: 100 (1 − P) per cent.
    pub fn driving_percent(&self) -> f64 {
        100.0 * (1.0 - self.rating[0].clamp(0.0, 1.0))
    }

    /// Passenger comfort: the share who stepped in without a complaint (None before anybody).
    pub fn comfort_percent(&self) -> Option<f64> {
        (self.rating[4] > 0.0).then(|| 100.0 * self.rating[1] / self.rating[4])
    }

    /// Ticket selling: points over twice the tickets asked for (None before any).
    pub fn ticket_percent(&self) -> Option<f64> {
        (self.rating[2] > 0.0).then(|| 100.0 * self.rating[3] / (2.0 * self.rating[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busstops_are_served_late_early_in_the_file() {
        let path =
            std::env::temp_dir().join(format!("omsi_driver_busstops_{}.odr", std::process::id()));
        let d = Driver {
            name: "Test".into(),
            bus_stops: [10, 1, 3],
            ..Default::default()
        };
        d.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let text = String::from_utf16(&units).unwrap();
        assert!(text.contains("[busstops]\r\n10\r\n3\r\n1\r\n"), "{text}");
        let back = Driver::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back.bus_stops, [10, 1, 3]);
    }

    /// Each bus driven keeps its own hectometres, in a `[perbusinfo]` block of its own (#2475).
    #[test]
    fn every_bus_keeps_its_own_kilometres() {
        let path =
            std::env::temp_dir().join(format!("omsi_driver_perbus_{}.odr", std::process::id()));
        let mut d = Driver {
            name: "Test".into(),
            ..Default::default()
        };
        d.add_bus_hektom("MAN SD202 - D86", 12);
        d.add_bus_hektom("MAN NG 272", 3);
        d.add_bus_hektom("MAN SD202 - D86", 5);
        d.add_bus_hektom(" ", 9);
        d.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let text = String::from_utf16(&units).unwrap();
        assert!(
            text.contains("[perbusinfo]\r\nMAN SD202 - D86\r\n17\r\n"),
            "{text}"
        );
        assert!(
            text.contains("[perbusinfo]\r\nMAN NG 272\r\n3\r\n"),
            "{text}"
        );
        let back = Driver::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            back.per_bus,
            vec![
                ("MAN SD202 - D86".to_string(), 17),
                ("MAN NG 272".to_string(), 3)
            ]
        );
    }
}
