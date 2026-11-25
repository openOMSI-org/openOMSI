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

    /// The smallest amount the currency counts in: 10 to the minus its `[currency]`
    /// decimals (Omsi.exe's TCurrency keeps it at +0x10, 0x5cca00; the decimals are a byte
    /// there). 0.01 for two decimals, whatever its smallest coin is worth.
    pub fn unit(&self) -> f32 {
        10f32.powi(-((self.decimals as u8) as i32))
    }

    /// How far a payment or the change may be off and still count as right: half of
    /// [`Currency::unit`] (the passenger's money 0x7e8254, the driver's change 0x7e8900).
    pub fn tolerance(&self) -> f32 {
        self.unit() / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Omsi.exe counts change to half the currency's unit, not half its smallest coin: with
    /// 5 cents the smallest coin, 2 cents short is still short.
    #[test]
    fn the_change_is_counted_to_half_the_currency_unit() {
        let c = Currency {
            decimals: 2,
            coins: vec![("5c.x".into(), 0.05), ("1.x".into(), 1.0)],
            ..Currency::default()
        };
        assert!((c.unit() - 0.01).abs() < 1e-7);
        assert!((c.tolerance() - 0.005).abs() < 1e-7);
        assert!(c.tolerance() < 0.02);
        let whole = Currency {
            decimals: 0,
            ..Currency::default()
        };
        assert_eq!(whole.unit(), 1.0);
        assert_eq!(whole.tolerance(), 0.5);
    }
}
