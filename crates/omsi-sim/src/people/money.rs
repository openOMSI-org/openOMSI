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
        let currency = Currency::load(&path)
            .map_err(|e| log::warn!("money system {}: {e}", path.display()))
            .ok();
        if let Some(c) = &currency {
            log::info!(
                "money system {}: {} coins, {} bills",
                c.name,
                c.coins.len(),
                c.bills.len()
            );
        }
        Money {
            currency,
            dir: path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
            meshes: HashMap::new(),
            placed: Vec::new(),
            hidden: Vec::new(),
            rng: 0x5151_7777,
        }
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
        let Some(c) = &self.currency else {
            return Vec::new();
        };
        let n = c.coins.len();
        let mut all: Vec<(usize, f32)> = c
            .coins
            .iter()
            .enumerate()
            .map(|(i, (_, v))| (i, *v))
            .chain(c.bills.iter().enumerate().map(|(k, (_, v))| (n + k, *v)))
            .filter(|(_, v)| *v > 0.0)
            .collect();
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

    /// What a passenger puts on the desk for `price` (sub_7e8254): pieces of the money
    /// system - every `[coin]` and `[bill]` alike, each as likely (`Random(count)`, 0x7e82c5)
    /// - drawn until they cover the fare, then the pieces not needed taken back again
    /// ([`Self::take_back`]). A coin in the payment plays `ev_ticketsale_givemoney`
    /// (0x7e84f8; the caller's).
    pub fn omsi_coins_for(&mut self, price: f32) -> Vec<usize> {
        if !price.is_finite() || price <= 0.0 {
            return Vec::new();
        }
        // (the money system's pieces: coins first, then the bills - the draw is uniform, so
        // the order of the file, which Omsi.exe keeps, makes no difference)
        let pieces: Vec<(usize, f32)> = match self.currency.as_ref() {
            Some(c) => {
                let n = c.coins.len();
                c.coins
                    .iter()
                    .enumerate()
                    .map(|(i, (_, v))| (i, *v))
                    .chain(c.bills.iter().enumerate().map(|(k, (_, v))| (n + k, *v)))
                    .collect()
            }
            None => return Vec::new(),
        };
        // (a money system without a piece worth anything: Omsi.exe never covered the fare)
        if !pieces.iter().any(|(_, v)| v.is_finite() && *v > 0.0) {
            return Vec::new();
        }
        let tolerance = self.currency.as_ref().map_or(0.005, |c| c.tolerance());
        let mut out: Vec<(usize, f32)> = Vec::new();
        let mut sum = 0.0_f32;
        while sum < price && out.len() < 200 {
            let k = ((self.rand_f() * pieces.len() as f32) as usize).min(pieces.len() - 1);
            sum += pieces[k].1;
            out.push(pieces[k]);
        }
        Self::take_back(price, tolerance, &mut out);
        out.into_iter().map(|(id, _)| id).collect()
    }

    /// sub_7e8254's second loop (0x7e8489): the first piece of `out` the rest covers the fare
    /// without (to `tolerance`, half the currency's unit) is taken back - the last piece put
    /// in its place, as Omsi.exe shortens its list - and the list is gone through again from
    /// the start, until no piece can be spared.
    fn take_back(price: f32, tolerance: f32, out: &mut Vec<(usize, f32)>) {
        let mut sum: f32 = out.iter().map(|(_, v)| *v).sum();
        'again: loop {
            for j in 0..out.len() {
                if price - tolerance <= sum - out[j].1 {
                    sum -= out[j].1;
                    out.swap_remove(j);
                    continue 'again;
                }
            }
            break;
        }
    }

    /// The value of the smallest coin.
    pub fn smallest_value(&self) -> f32 {
        self.currency
            .as_ref()
            .and_then(|c| {
                c.coins
                    .iter()
                    .map(|(_, v)| *v)
                    .filter(|v| *v > 0.0)
                    .reduce(f32::min)
            })
            .unwrap_or(0.01)
    }

    /// How many coins lie on the change tray.
    pub fn change_count(&self) -> usize {
        self.placed.iter().filter(|p| p.3).count()
    }

    pub fn value_of(&self, coins: &[usize]) -> f32 {
        let Some(c) = &self.currency else { return 0.0 };
        let n = c.coins.len();
        coins
            .iter()
            .filter_map(|&i| {
                if i < n {
                    c.coins.get(i)
                } else {
                    c.bills.get(i - n)
                }
            })
            .map(|(_, v)| *v)
            .sum()
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
        // (what the passenger still waits for: more than the tolerance the change is
        // counted to, half the currency's unit - not half the smallest coin, or a few cents
        // short of a 5-cent coin were never handed out while the passenger waited for them)
        let tolerance = self.currency.as_ref().map_or(0.005, |c| c.tolerance());
        if left > tolerance {
            self.exact_coins_for(left)
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coins_lie_flat_on_the_point_not_stacked() {
        let point = Vec3::new(0.4, 5.0, 1.1);
        let mut rng = Money {
            currency: None,
            dir: PathBuf::new(),
            meshes: HashMap::new(),
            placed: Vec::new(),
            hidden: Vec::new(),
            rng: 7,
        };
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
        let mut m = Money {
            currency: None,
            dir: PathBuf::new(),
            meshes: HashMap::new(),
            placed: Vec::new(),
            hidden: Vec::new(),
            rng: 7,
        };
        m.placed.push((10, Mat4::IDENTITY, 0, false, None));
        m.placed.push((11, Mat4::IDENTITY, 1, true, None));
        m.placed.push((12, Mat4::IDENTITY, 2, true, None));
        m.take_coin(1);
        assert_eq!(
            m.placed.iter().map(|p| p.0).collect::<Vec<_>>(),
            vec![10, 12]
        );
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

    /// The rest of the change is handed out down to the tolerance the passenger counts it
    /// to, half the currency's unit: 2 cents still owed with 5 cents the smallest coin are
    /// owed, not forgiven.
    #[test]
    fn the_rest_of_the_change_is_counted_to_half_the_currency_unit() {
        let mut money = money_with_currency(&[1.0, 0.05], &[]);
        money.currency.as_mut().unwrap().decimals = 2;
        assert!(!money.change_left_coins(0.02).is_empty());
        assert!(money.change_left_coins(0.004).is_empty());
    }

    /// Omsi.exe draws from every piece of the money system, the bills as likely as each
    /// coin (sub_7e8254): a small fare is mostly paid with a note, and a fare above the
    /// largest coin now and then with coins alone.
    #[test]
    fn the_payment_draws_coins_and_bills_alike() {
        let mut money = money_with_currency(
            &[0.05, 0.10, 0.20, 0.50, 1.0, 2.0],
            &[5.0, 10.0, 20.0, 50.0, 100.0],
        );
        money.currency.as_mut().unwrap().decimals = 2;
        let coins = 6;
        let (mut small_with_bill, mut large_coins_only) = (0, 0);
        for _ in 0..400 {
            let small = money.omsi_coins_for(1.70);
            assert!(money.value_of(&small) >= 1.70 - 0.005);
            if small.iter().any(|&id| id >= coins) {
                small_with_bill += 1;
                // (the note alone: every other piece is spared)
                assert_eq!(small.len(), 1, "{small:?}");
            }
            let large = money.omsi_coins_for(2.50);
            assert!(money.value_of(&large) >= 2.50 - 0.005);
            if large.iter().all(|&id| id < coins) {
                large_coins_only += 1;
            }
        }
        // (a note is drawn before the coins cover 1.70 about 4 times in 5; 2.50 is paid
        // in coins alone about once in 10)
        assert!((260..=380).contains(&small_with_bill), "{small_with_bill}");
        assert!((15..=80).contains(&large_coins_only), "{large_coins_only}");
    }

    /// The pieces not needed are taken back as Omsi.exe shortens its list: the last piece
    /// moves into the place of the one taken, and the search starts over.
    #[test]
    fn spare_pieces_are_taken_back_in_omsi_order() {
        // 0.50, then two 1.00 pieces for a fare of 1: the 0.50 goes, the second 1.00 moves
        // to the front and goes next - the first 1.00 is paid
        let mut out = vec![(0, 0.5), (1, 1.0), (2, 1.0)];
        Money::take_back(1.0, 0.005, &mut out);
        assert_eq!(out, vec![(1, 1.0)]);
        // nothing to spare: all stay
        let mut out = vec![(0, 0.5), (1, 0.2)];
        Money::take_back(0.7, 0.005, &mut out);
        assert_eq!(out.len(), 2);
        // short by less than half the unit: still enough
        let mut out = vec![(0, 0.698), (1, 1.0)];
        Money::take_back(0.7, 0.005, &mut out);
        assert_eq!(out, vec![(1, 1.0)]);
        let mut out = vec![(0, 0.698), (1, 0.01)];
        Money::take_back(0.7, 0.005, &mut out);
        assert_eq!(out, vec![(0, 0.698)]);
    }
}
