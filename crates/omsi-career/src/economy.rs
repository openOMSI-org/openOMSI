//! Persistent O-Bucks wallet. Amounts intentionally follow euro-like fares.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const STARTING_BALANCE: f64 = 25.00;
pub const ON_TIME_REWARD: f64 = 0.15;
pub const EARLY_OR_LATE_FINE: f64 = 5.00;
pub const CRASH_FINE: f64 = 20.00;
pub const REPAIR_COST: f64 = 12.50;
pub const BAD_DRIVING_FINE: f64 = 0.25;
pub const TRAFFIC_OFFENCE_FINE: f64 = 4.00;
pub const WRONG_WAY_FINE: f64 = 4.00;
pub const PRIORITY_OFFENCE_FINE: f64 = 4.00;
pub const MISSED_ALIGHTING_FINE: f64 = 1.50;
pub const MISSED_BOARDING_FINE: f64 = 1.50;
pub const ACCIDENT_EXONERATION: f64 = 8.00;
pub const PASSENGER_ANSWER_REWARD: f64 = 0.50;
pub const PASSENGER_ANSWER_FINE: f64 = 0.25;
pub const PASSENGER_COMPLIMENT_REWARD: f64 = 0.10;
pub const COURTESY_REWARD: f64 = 0.10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoneyChange { pub amount: f64, pub balance: f64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet { pub balance: f64 }

impl Default for Wallet { fn default() -> Self { Self { balance: STARTING_BALANCE } } }

impl Wallet {
    pub fn load(driver: &str) -> Self {
        wallet_path(driver).and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, driver: &str) -> std::io::Result<()> {
        let Some(path) = wallet_path(driver) else { return Ok(()) };
        if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
        std::fs::write(path, serde_json::to_vec_pretty(self).expect("wallet serializes"))
    }

    pub fn change(&mut self, amount: f64) -> MoneyChange {
        let old = self.balance;
        self.balance = ((self.balance + amount).max(0.0) * 100.0).round() / 100.0;
        MoneyChange { amount: self.balance - old, balance: self.balance }
    }
}

fn wallet_path(driver: &str) -> Option<PathBuf> {
    let root = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let safe: String = driver.chars().map(|c| if c.is_ascii_alphanumeric() || "-_".contains(c) { c } else { '_' }).collect();
    Some(PathBuf::from(root).join(".openomsi").join("obucks").join(format!("{safe}.json")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cents_and_floor() {
        let mut w = Wallet { balance: 2.105 };
        assert_eq!(w.change(-1.10).balance, 1.01);
        assert_eq!(w.change(-99.0).balance, 0.0);
    }
}
