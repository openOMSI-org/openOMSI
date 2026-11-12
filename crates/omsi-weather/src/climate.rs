use std::path::{Path, PathBuf};

use omsi_cfg::{CfgFile, CfgReader};

/// Monthly climate statistics per location, specified in the map's `openomsi_climate.cfg`
#[derive(Debug, Clone)]
pub struct ClimateStats {
    /// Mean temperatures (°C)
    pub temp: [f32; 12],
    /// Mean precipitations (mm)
    pub precip: [f32; 12],
}

impl ClimateStats {
    pub fn for_map(map: &str, root: &Path) -> Option<ClimateStats> {
        Path::new(map).parent().map(|dir| {
            let rel = dir.strip_prefix(root).unwrap_or(&dir);
            const CLIMATE_CFG: &'static str = "openomsi_climate.cfg";
            let climate_path = omsi_cfg::find_in_roots(&rel.join(CLIMATE_CFG).to_string_lossy())
                .map(|x| x.1)
                .unwrap_or_else(|| dir.join(CLIMATE_CFG));
            let f = CfgFile::read(climate_path).ok()?;
            Self::from_cfg(&f)
        })?
    }

    fn read_monthly_values(r: &mut CfgReader) -> Option<Vec<f32>> {
        r.rest_of_block()
            .iter()
            .map(|x| x.trim())
            .filter(|x| !x.is_empty())
            .map(|x| x.parse::<f32>())
            .collect::<Result<Vec<f32>, _>>()
            .map_err(|e| log::warn!("openomsi_climate.cfg: float parsing error: {e}"))
            .ok()
    }

    pub fn from_cfg(f: &CfgFile) -> Option<ClimateStats> {
        let mut r = f.reader();
        let mut temp: Option<Vec<f32>> = None;
        let mut precip: Option<Vec<f32>> = None;
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "temperature" => {
                    temp = Self::read_monthly_values(&mut r);
                }
                "precipitation" => {
                    precip = Self::read_monthly_values(&mut r);
                }
                _ => {
                    log::warn!("openomsi_climate.cfg: unknown section [{k}] occurred");
                }
            }
        }
        if let Some(t) = temp {
            let p = precip.unwrap_or_else(|| vec![0.0f32; 12]);
            if t.iter().all(|x| (-100.0..=100.0).contains(x))
                && p.iter().all(|x| (0.0..=5000.0).contains(x))
            {
                let t = t.as_array().or_else(|| { log::warn!("openomsi_climate.cfg: temperature: {} elements provided, 12 expected; falling back to default", t.len()); None })?;
                let p = p.as_array().or_else(||  { log::warn!("openomsi_climate.cfg: precipitation: {} elements provided, 12 expected; falling back to default", p.len()); None })?;
                Some(ClimateStats {
                    temp: *t,
                    precip: *p,
                })
            } else {
                log::warn!(
                    "openomsi_climate.cfg: some values non-finite or outside the range (-100 to 100 °C, 0 to 5000 mm)"
                );
                None
            }
        } else {
            log::warn!("openomsi_climate.cfg: incomplete configuration, falling back to default");
            None
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn custom_climate_is_read_properly() {
        use std::path::PathBuf;

        use omsi_cfg::CfgFile;

        use crate::climate::ClimateStats;

        let f1 = CfgFile {
            path: PathBuf::from("maps/test/openomsi_climate.cfg"),
            lines: [
                "[temperature]",
                "1",
                "2",
                "3",
                "4",
                "5",
                "6",
                "7",
                "8",
                "9",
                "10",
                "11",
                "12",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect(),
        };
        let s1 = ClimateStats::from_cfg(&f1);
        assert!(s1.is_some());
        assert_eq!(
            s1.unwrap().temp,
            [
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0
            ]
        );

        let f2 = CfgFile {
            path: PathBuf::from("maps/test/openomsi_climate.cfg"),
            lines: [
                "[temperature]",
                "1",
                "2",
                "3",
                "4",
                "5",
                "6",
                "",
                "7",
                "8",
                "9",
                "",
                "10",
                "11",
                "12",
                "",
                "",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect(),
        };
        let s2 = ClimateStats::from_cfg(&f2);
        assert!(s2.is_some());
        assert_eq!(
            s2.unwrap().temp,
            [
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0
            ]
        );

        let f3 = CfgFile {
            path: PathBuf::from("maps/test/openomsi_climate.cfg"),
            lines: [
                "[temperature]",
                "1",
                "2",
                "3",
                "4",
                "5",
                "6",
                "7",
                "8",
                "",
                "[precipitation]",
                "25",
                "30",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect(),
        };
        let s3 = ClimateStats::from_cfg(&f3);
        assert!(s3.is_none());

        let f4 = CfgFile {
            path: PathBuf::from("maps/test/openomsi_climate.cfg"),
            lines: [
                "[temperatures]",
                "1",
                "2",
                "3",
                "4",
                "5",
                "6",
                "7",
                "8",
                "9",
                "10",
                "11",
                "12",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect(),
        };
        let s4 = ClimateStats::from_cfg(&f4);
        assert!(s4.is_none());

        let f5 = CfgFile {
            path: PathBuf::from("maps/test/openomsi_climate.cfg"),
            lines: [
                "[temperature]",
                "1",
                "2",
                "3",
                "4",
                "5",
                "6",
                "7",
                "8",
                "9",
                "10",
                "11",
                "NaN",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect(),
        };
        let s5 = ClimateStats::from_cfg(&f5);
        assert!(s5.is_none());

        let f6 = CfgFile {
            path: PathBuf::from("maps/test/openomsi_climate.cfg"),
            lines: [
                "[temperature]",
                "1",
                "2",
                "3",
                "4",
                "5",
                "6",
                "7",
                "8",
                "9",
                "10",
                "11",
                "-Inf",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect(),
        };
        let s6 = ClimateStats::from_cfg(&f6);
        assert!(s6.is_none());
    }
}
