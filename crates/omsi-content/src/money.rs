//! `Money/<cur>/<cur>.cfg` (unit `mc_money`).

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Currency {
    pub path: PathBuf,
    pub name: String,
    pub decimals: i32,
    /// (mesh file, value)
    pub coins: Vec<(String, f32)>,
    pub bills: Vec<(String, f32)>,
}

impl Currency {
    pub fn load(path: &Path) -> Result<Currency, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut c = Currency {
            path: f.path.clone(),
            ..Default::default()
        };
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "currency" => {
                    c.name = r.str().to_string();
                    c.decimals = r.i32();
                }
                "coin" => c.coins.push((r.str().to_string(), r.f32())),
                "bill" => c.bills.push((r.str().to_string(), r.f32())),
                _ => {}
            }
        }
        Ok(c)
    }
}
