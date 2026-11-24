//! Money on the cash desk: coins a passenger pays with (`[ticket_sale_money_point]`) and the
//! change the driver hands out with `GiveChangeCoin` (`[ticket_sale_change_point]`).
//!
//! Which coins, and where on the point each one lies, is decided here; omsi-app's `money`
//! draws them (it makes the coins' meshes and instances, kept in `meshes` and `placed`).

use glam::{Mat4, Vec3};
use hashbrown::HashMap;
use omsi_content::Currency;
use std::path::{Path, PathBuf};

pub struct Money {
    pub currency: Option<Currency>,
    pub dir: PathBuf,
    /// The renderer's mesh and materials of each coin (made by the app the first time the
    /// coin is put down).
    pub meshes: HashMap<usize, (usize, Vec<usize>)>,
    /// (instance, place in the bus frame - or in its parent mesh's -, coin index, is
    /// change, the `[mesh_ident]` of the mesh it lies on)
    pub placed: Vec<(usize, Mat4, usize, bool, Option<String>)>,
    pub hidden: Vec<usize>,
    rng: u64,
}

impl Money {
    pub fn new(root: &Path, money_system: &str) -> Money {
        let path = omsi_cfg::resolve_path(root, money_system);
        let currency = Currency::load(&path).map_err(|e| log::warn!("money system {}: {e}", path.display())).ok();
        if let Some(c) = &currency {
            log::info!("money system {}: {} coins, {} bills", c.name, c.coins.len(), c.bills.len());
        }
        Money { currency, dir: path.parent().map(|p| p.to_path_buf()).unwrap_or_default(), meshes: HashMap::new(), placed: Vec::new(), hidden: Vec::new(), rng: 0x5151_7777 }
    }

    pub fn rand_f(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Every coin and banknote of the currency as (index, value): coins first,
    /// then banknotes (index `coins.len() + k`). The available denominations
    /// influence passenger payments even when no plugin changes the payment.
    fn denominations(&self) -> Vec<(usize, f32)> {
        let Some(c) = &self.currency else { return Vec::new() };
        let n = c.coins.len();
        let mut all: Vec<(usize, f32)> = c.coins.iter().enumerate().map(|(i, (_, v))| (i, *v)).chain(c.bills.iter().enumerate().map(|(k, (_, v))| (n + k, *v))).filter(|(_, v)| *v > 0.0).collect();
        all.sort_by(|a, b| b.1.total_cmp(&a.1));
        all
    }

    /// `value` in as few pieces as the currency allows, largest first; None when it cannot
    /// be paid exactly with them (a fraction smaller than the smallest coin).
    fn exact_pieces(all: &[(usize, f32)], value: f32) -> Option<Vec<usize>> {
        let mut out = Vec::new();
        let mut left = value;
        for (i, v) in all {
            while left >= *v - 0.001 && out.len() < 40 {
                out.push(*i);
                left -= v;
            }
        }
        (left.abs() <= 0.001).then_some(out)
    }

    /// The fare exactly (no change due): coins and notes. (Capped at twelve coins and topped
    /// up with the smallest when that ran out, "exact" was not always exact.)
    pub fn exact_coins_for(&mut self, value: f32) -> Vec<usize> {
        let all = self.denominations();
        if let Some(out) = Self::exact_pieces(&all, value) {
            return out;
        }
        // not payable to the last cent: the nearest amount over it
        let mut out = Vec::new();
        let mut left = value;
        for (i, v) in &all {
            while left >= *v - 0.001 && out.len() < 40 {
                out.push(*i);
                left -= v;
            }
        }
        if left > 0.001 {
            if let Some((i, _)) = all.iter().rev().find(|(_, v)| *v >= left) {
                out.push(*i);
            }
        }
        out
    }

    /// What a passenger puts on the desk for `price`: eligible denominations are
    /// drawn at random until they cover the fare, then unnecessary pieces are
    /// taken back (the remainder may cover the fare minus half the smallest coin).
    pub fn omsi_coins_for(&mut self, price: f32) -> Vec<usize> {
        if !price.is_finite() || price <= 0.0 {
            return Vec::new();
        }

        // Extract the currency information without keeping a borrow of `self`
        // alive while random numbers are generated below.
        let (coin_count, largest_coin, bills) = match self.currency.as_ref() {
            Some(currency) => {
                let largest_coin = currency.coins.iter().map(|(_, value)| *value).filter(|value| value.is_finite() && *value > 0.0).max_by(|a, b| a.total_cmp(b));

                let bills: Vec<f32> = currency.bills.iter().map(|(_, value)| *value).filter(|value| value.is_finite() && *value > 0.0).collect();

                (currency.coins.len(), largest_coin, bills)
            }
            None => return Vec::new(),
        };

        let smallest_bill = bills.iter().copied().min_by(|a, b| a.total_cmp(b));
        let largest_bill = bills .iter().copied().max_by(|a, b| a.total_cmp(b));

        // Banknotes become relevant when the fare exceeds the largest coin,
        // or reaches the value of the smallest banknote.
        let may_use_bills = match (largest_coin, smallest_bill) {
            (Some(largest_coin), Some(smallest_bill)) => {
                price > largest_coin || price >= smallest_bill
            }
            (Some(_), None) => false,
            (None, Some(smallest_bill)) => price >= smallest_bill,
            (None, None) => false,
        };

        // Limit banknotes to the smallest denomination that covers the fare.
        // If no banknote covers it, the largest available banknote is the limit.
        let bill_limit = if may_use_bills {
            bills.iter().copied().filter(|value| *value >= price).min_by(|a, b| a.total_cmp(b)).or(largest_bill)
        } else {
            None
        };
        // These are probability weights, not monetary thresholds:
        // banknotes become more likely once the fare reaches the smallest bill.
        let bill_weight: usize = match (bill_limit, smallest_bill) {
            (Some(_), Some(smallest_bill)) if price >= smallest_bill => 6,
            (Some(_), _) => 3,
            _ => 0,
        };
        let all = self.denominations();
        if all.is_empty() {
            return self.exact_coins_for(price);
        }
        // Keep the original denomination IDs: coins first, banknotes afterwards.
        let mut choices: Vec<(usize, f32)> = Vec::new();
        for &(id, value) in &all {
            if !value.is_finite() || value <= 0.0 {
                continue;
            }
            let is_bill = id >= coin_count;
            if is_bill {
                let Some(limit) = bill_limit else {
                    continue;
                };
                // Never offer a banknote above the permitted limit.
                if value > limit || bill_weight == 0 {
                    continue;
                }
                // Repeating an option increases its probability of selection.
                for _ in 0..bill_weight {
                    choices.push((id, value));
                }
            } else {
                choices.push((id, value));
            }
        }

        if choices.is_empty() {
            return self.exact_coins_for(price);
        }

        let half_smallest = self.smallest_value() / 2.0;
        let mut out: Vec<(usize, f32)> = Vec::new();
        let mut sum = 0.0_f32;

        // Select random denominations until the fare is covered.
        while sum < price && out.len() < 200 {
            let k = ((self.rand_f() * choices.len() as f32) as usize).min(choices.len() - 1);

            let (id, value) = choices[k];
            sum += value;
            out.push((id, value));
        }
        // Remove unnecessary pieces while the remaining amount still covers
        // the fare, allowing for half the smallest coin as in the original.
        let mut again = true;

        while again {
            again = false;
            for j in 0..out.len() {
                if price - half_smallest <= sum - out[j].1 {
                    sum -= out[j].1;
                    out.remove(j);
                    again = true;
                    break;
                }
            }
        }
        out.into_iter().map(|(id, _)| id).collect()
    }

    /// The value of the smallest coin (the tolerance of the change is half of it).
    pub fn smallest_value(&self) -> f32 {
        self.currency.as_ref().and_then(|c| c.coins.iter().map(|(_, v)| *v).filter(|v| *v > 0.0).reduce(f32::min)).unwrap_or(0.01)
    }

    /// How many coins lie on the change tray.
    pub fn change_count(&self) -> usize {
        self.placed.iter().filter(|p| p.3).count()
    }

    pub fn value_of(&self, coins: &[usize]) -> f32 {
        let Some(c) = &self.currency else { return 0.0 };
        let n = c.coins.len();
        coins.iter().filter_map(|&i| if i < n { c.coins.get(i) } else { c.bills.get(i - n) }).map(|(_, v)| *v).sum()
    }

    /// One coin's place on a point from three random numbers in 0..1: x and y (forward) in the
    /// variation, the height of the point itself, a turn of 2*pi*r about the vertical.
    pub fn coin_place(point: Vec3, var: [f32; 2], r: [f32; 3]) -> Mat4 {
        let local = point + Vec3::new((r[0] - 0.5) * var[0], (r[1] - 0.5) * var[1], 0.0);
        Mat4::from_translation(local) * Mat4::from_rotation_z(r[2] * std::f32::consts::TAU)
    }

    /// Remove the payment (driver takes it) or the change (passenger takes it).
    pub fn clear(&mut self, change: bool) {
        let (gone, keep): (Vec<_>, Vec<_>) = self.placed.drain(..).partition(|p| p.3 == change);
        self.hidden.extend(gone.into_iter().map(|p| p.0));
        self.placed = keep;
    }

    /// The driver picks coin `k` of `placed` up again (a coin of the change, clicked).
    pub fn take_coin(&mut self, k: usize) {
        if k < self.placed.len() {
            let p = self.placed.remove(k);
            self.hidden.push(p.0);
        }
    }

    pub fn change_value(&self) -> f32 {
        let coins: Vec<usize> = self.placed.iter().filter(|p| p.3).map(|p| p.2).collect();
        self.value_of(&coins)
    }

    pub fn change_left_coins(&mut self, owed: f32) -> Vec<usize> {
        let left = owed - self.change_value();
        if left > self.smallest_value() / 2.0 { self.exact_coins_for(left) } else { Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coins_lie_flat_on_the_point_not_stacked() {
        let point = Vec3::new(0.4, 5.0, 1.1);
        let mut rng = Money { currency: None, dir: PathBuf::new(), meshes: HashMap::new(), placed: Vec::new(), hidden: Vec::new(), rng: 7 };
        let mut top = f32::MIN;
        for _ in 0..80 {
            let r = [rng.rand_f(), rng.rand_f(), rng.rand_f()];
            let m = Money::coin_place(point, [0.1, 0.06], r);
            let p = m.transform_point3(Vec3::ZERO);
            assert!((p.x - point.x).abs() <= 0.05 + 1e-6 && (p.y - point.y).abs() <= 0.03 + 1e-6);
            top = top.max(p.z);
        }
        // 80 coins: all at the point's height (they were 80 * 3 mm = 24 cm high before)
        assert!((top - point.z).abs() < 1e-6, "{top}");
    }

    #[test]
    fn a_coin_of_the_change_is_picked_up_alone() {
        let mut m = Money { currency: None, dir: PathBuf::new(), meshes: HashMap::new(), placed: Vec::new(), hidden: Vec::new(), rng: 7 };
        m.placed.push((10, Mat4::IDENTITY, 0, false, None));
        m.placed.push((11, Mat4::IDENTITY, 1, true, None));
        m.placed.push((12, Mat4::IDENTITY, 2, true, None));
        m.take_coin(1);
        assert_eq!(m.placed.iter().map(|p| p.0).collect::<Vec<_>>(), vec![10, 12]);
        assert_eq!(m.hidden, vec![11]);
        m.take_coin(9);
        assert_eq!(m.placed.len(), 2);
    }

    #[test]
    fn the_change_left_comes_in_coins_on_top_of_the_tray() {
        let mut m = money_with_currency(&[0.1, 0.2, 0.5, 1.0, 2.0], &[5.0]);
        let all = m.change_left_coins(3.7);
        assert!((m.value_of(&all) - 3.7).abs() < 1e-4);
        m.placed.push((1, Mat4::IDENTITY, 3, true, None));
        m.placed.push((2, Mat4::IDENTITY, 4, false, None));
        let left = m.change_left_coins(3.7);
        assert!((m.value_of(&left) - 2.7).abs() < 1e-4);
        m.placed.push((3, Mat4::IDENTITY, 4, true, None));
        assert!(m.change_left_coins(3.0).is_empty());
        assert!(m.change_left_coins(2.9).is_empty());
    }

    fn money_with_currency(coins: &[f32], bills: &[f32]) -> Money {
        Money {
            currency: Some(Currency {
                coins: coins
                    .iter()
                    .enumerate()
                    .map(|(i, value)| (format!("coin-{i}"), *value))
                    .collect(),
                bills: bills
                    .iter()
                    .enumerate()
                    .map(|(i, value)| (format!("bill-{i}"), *value))
                    .collect(),
                ..Currency::default()
            }),
            dir: PathBuf::new(),
            meshes: HashMap::new(),
            placed: Vec::new(),
            hidden: Vec::new(),
            rng: 0x5151_7777,
        }
    }

    /// Repeatedly checks that no passenger payment contains a banknote
    /// above the permitted limit. `None` means banknotes must not be used.
    fn assert_bill_limit(
        money: &mut Money,
        price: f32,
        max_bill: Option<f32>,
    ) {
        let coin_count = money.currency.as_ref().unwrap().coins.len();

        for _ in 0..100 {
            let pieces = money.omsi_coins_for(price);

            for id in pieces {
                if id < coin_count {
                    continue;
                }

                let value = money.currency.as_ref().unwrap().bills[id - coin_count].1;

                match max_bill {
                    Some(limit) => assert!(
                        value <= limit + 0.001,
                        "Fare {price}: selected a {value} banknote, \
                         but the maximum allowed is {limit}"
                    ),
                    None => panic!(
                        "Fare {price}: selected a {value} banknote, \
                         although banknotes should not be used"
                    ),
                }
            }
        }
    }

    #[test]
    fn euro_fare_does_not_use_banknotes_above_the_smallest_note_covering_fare() {
        // Coins: €0.05, €0.10, €0.20, €0.50, €1, €2.
        // Banknotes: €5, €10, €20, €50, €100.
        let mut money = money_with_currency(
            &[0.05, 0.10, 0.20, 0.50, 1.0, 2.0],
            &[5.0, 10.0, 20.0, 50.0, 100.0],
        );

        // For a €6.70 fare, the largest permitted banknote is €10.
        assert_bill_limit(&mut money, 6.70, Some(10.0));
    }

    #[test]
    fn dinar_fare_does_not_use_banknotes_above_the_smallest_note_covering_fare() {
        // Largest coin: 50 dinars.
        // Banknotes: 50, 100, 200, 500, 1000 and 5000 dinars.
        let mut money = money_with_currency(
            &[1.0, 2.0, 5.0, 10.0, 20.0, 50.0],
            &[50.0, 100.0, 200.0, 500.0, 1000.0, 5000.0],
        );

        // For a 65-dinar fare, the largest permitted banknote is 100.
        assert_bill_limit(&mut money, 65.0, Some(100.0));
    }

    #[test]
    fn fare_below_largest_coin_does_not_use_banknotes() {
        // Deliberately use denominations on a non-euro scale to ensure
        // the algorithm does not depend on fixed thresholds such as €5 or €10.
        let mut money = money_with_currency(
            &[1.0, 2.0, 5.0, 10.0, 20.0, 100.0],
            &[500.0, 1000.0, 5000.0],
        );

        // The fare is below the largest coin (100), so no banknotes.
        assert_bill_limit(&mut money, 65.0, None);
    }
}
