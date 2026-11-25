//! `passengercabin.cfg` (unit `mc_passcabin`).

use omsi_cfg::CfgFile;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PassPos {
    pub pos: [f32; 3],
    pub height: f32,
    pub rot: f32,
    /// The four `[interiorlight]`s (by index, -1 none) that light a person on this seat, as
    /// Omsi.exe keeps them in the seat record (+0x24..+0x27, 0x5ce680): the first seat of
    /// the file, `[passpos]` or `[drivpos]`, has 0 1 2 3, every later one those of the seat
    /// before it, and an `[illumination_interior]` sets those of the seat written last.
    pub illumination: [i32; 4],
    /// Where it stands in Omsi.exe's one list of `[passpos]` and `[drivpos]` (file order):
    /// the seat number scripts ask `GetHumanCountOnSeat` about (0x7d39a4) - with the
    /// driver's place first, as most cabins have it, the first `[passpos]` is seat 1.
    pub file_index: usize,
    /// openOMSI's (#721): a script variable that switches the `[passpos]` on and off - while
    /// it is 0 no passenger takes the place - named on the line straight after its five
    /// values. None (or a name the scripts do not have): always on.
    pub switch_var: Option<String>,
    /// openOMSI's: a script variable the engine sets to 1 while somebody is on the place and
    /// to 0 while nobody is, named on the line after that.
    pub taken_var: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entry {
    pub path_point: i32,
    pub no_ticket_sale: bool,
    pub with_button: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Point3 {
    pub path_point: i32,
    pub pos: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VarPoint {
    pub pos: [f32; 3],
    pub var: [f32; 2],
    pub parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PassengerCabin {
    pub entries: Vec<Entry>,
    pub exits: Vec<i32>,
    pub link_to_next_veh: Option<i32>,
    pub link_to_prev_veh: Option<i32>,
    pub stampers: Vec<Point3>,
    pub ticket_sales: Vec<Point3>,
    pub money_points: Vec<VarPoint>,
    pub change_points: Vec<VarPoint>,
    pub pass_positions: Vec<PassPos>,
    pub driver_positions: Vec<PassPos>,
}

impl PassengerCabin {
    pub fn load(path: &Path) -> Result<PassengerCabin, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        Ok(Self::parse(&f))
    }

    pub fn parse(f: &CfgFile) -> PassengerCabin {
        let mut c = PassengerCabin::default();
        let mut r = f.reader().disabled_blocks();
        // the seat written last (driver's or not, and which): Omsi.exe keeps both kinds in
        // one list, in the order of the file
        let mut last: Option<(bool, usize)> = None;
        while let Some(e) = r.next_entry(&["{noticketsale}", "{withbutton}"]) {
            let k = match e {
                // a line of its own anywhere between the blocks, as Omsi.exe reads them
                // (0x5ce952 - 0x5ce9bc): it marks the entry read last. Every stock cabin has
                // a blank line between the second entry's path point and its
                // `{noticketsale}`, and read only straight after the path point, the flag
                // was lost there - and so was a `{withbutton}` written the same way (#1156)
                omsi_cfg::Entry::Token(t) => {
                    if let Some(e) = c.entries.last_mut() {
                        if t == "{noticketsale}" {
                            e.no_ticket_sale = true;
                        } else {
                            e.with_button = true;
                        }
                    }
                    continue;
                }
                omsi_cfg::Entry::Keyword(k) => k,
            };
            match k.as_str() {
                "entry" => c.entries.push(Entry {
                    path_point: r.i32(),
                    ..Default::default()
                }),
                "exit" => c.exits.push(r.i32()),
                "linktonextveh" => c.link_to_next_veh = Some(r.i32()),
                "linktoprevveh" => c.link_to_prev_veh = Some(r.i32()),
                "stamper" => c.stampers.push(Point3 {
                    path_point: r.i32(),
                    pos: r.f32s::<3>(),
                }),
                "ticket_sale" => c.ticket_sales.push(Point3 {
                    path_point: r.i32(),
                    pos: r.f32s::<3>(),
                }),
                "ticket_sale_money_point"
                | "ticket_sale_money_point_2"
                | "ticket_sale_change_point"
                | "ticket_sale_change_point_2" => {
                    let pos = r.f32s::<3>();
                    let var = r.f32s::<2>();
                    let parent = if k.ends_with("_2") {
                        Some(r.str().to_string())
                    } else {
                        None
                    };
                    let p = VarPoint { pos, var, parent };
                    if k.contains("money") {
                        c.money_points.push(p);
                    } else {
                        c.change_points.push(p);
                    }
                }
                "passpos" | "drivpos" => {
                    let pos = r.f32s::<3>();
                    let height = r.f32();
                    let rot = r.f32();
                    let illumination = match last {
                        Some((true, i)) => c.driver_positions[i].illumination,
                        Some((false, i)) => c.pass_positions[i].illumination,
                        None => [0, 1, 2, 3],
                    };
                    let file_index = c.pass_positions.len() + c.driver_positions.len();
                    // (openOMSI's two variables of a `[passpos]`, #721: lines that follow at
                    // once - a blank line, the next block or a `{...}` flag ends them, so every
                    // OMSI 2 file reads as before, and Omsi.exe passes over the lines)
                    let mut more = || {
                        let l = r.lines().get(r.pos())?.trim();
                        if k != "passpos"
                            || l.is_empty()
                            || l.starts_with('[')
                            || l.starts_with('{')
                        {
                            return None;
                        }
                        r.line();
                        Some(l.to_string())
                    };
                    let switch_var = more();
                    let taken_var = switch_var.as_ref().and_then(|_| more());
                    let p = PassPos {
                        pos,
                        height,
                        rot,
                        illumination,
                        file_index,
                        switch_var,
                        taken_var,
                    };
                    if k == "passpos" {
                        c.pass_positions.push(p);
                        last = Some((false, c.pass_positions.len() - 1));
                    } else {
                        c.driver_positions.push(p);
                        last = Some((true, c.driver_positions.len() - 1));
                    }
                }
                "illumination_interior" => {
                    // (exactly four lines, each a signed byte in the record)
                    let l = [r.i32(), r.i32(), r.i32(), r.i32()];
                    let seat = match last {
                        Some((true, i)) => Some(&mut c.driver_positions[i]),
                        Some((false, i)) => Some(&mut c.pass_positions[i]),
                        None => None,
                    };
                    if let Some(seat) = seat {
                        seat.illumination = l;
                    }
                }
                _ => {}
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("../../../tools/test-support/original_root.rs");

    #[test]
    fn seat_lamps_follow_the_seat_before() {
        let text = "[drivpos]\n-0.8\n4.5\n1.0\n0.5\n0\n\n[illumination_interior]\n4\n5\n-1\n-1\n\n\
                    [passpos]\n0.5\n2\n1\n0.5\n0\n\n[passpos]\n0.5\n1\n1\n0.5\n0\n\n\
                    [illumination_interior]\n6\n7\n8\n9\n\n[passpos]\n0.5\n0\n1\n0.5\n0\n";
        let c = PassengerCabin::parse(&CfgFile::from_str("passengercabin.cfg", text));
        assert_eq!(c.driver_positions[0].illumination, [4, 5, -1, -1]);
        assert_eq!(c.pass_positions[0].illumination, [4, 5, -1, -1]);
        assert_eq!(c.pass_positions[1].illumination, [6, 7, 8, 9]);
        assert_eq!(c.pass_positions[2].illumination, [6, 7, 8, 9]);
        let c = PassengerCabin::parse(&CfgFile::from_str(
            "passengercabin.cfg",
            "[passpos]\n0\n0\n1\n0.5\n0\n",
        ));
        assert_eq!(c.pass_positions[0].illumination, [0, 1, 2, 3]);
    }

    /// #1156: `{noticketsale}` and `{withbutton}` are lines of their own that mark the entry
    /// read last, wherever they stand before the next one - the stock cabins write a blank
    /// line before them.
    #[test]
    fn entry_flags_mark_the_entry_read_last() {
        let text = "[entry]\r\n0\r\n\r\n[entry]\r\n4\r\n\r\n{noticketsale}\r\n\r\n[exit]\r\n7\r\n\r\n\
                    [entry]\r\n9\r\n{withbutton}\r\n{noticketsale}\r\n\r\n[entry]\r\n11\r\n\r\n[exit]\r\n12\r\n\r\n{withbutton}\r\n";
        let c = PassengerCabin::parse(&CfgFile::from_str("passengercabin.cfg", text));
        let flags: Vec<(i32, bool, bool)> = c
            .entries
            .iter()
            .map(|e| (e.path_point, e.no_ticket_sale, e.with_button))
            .collect();
        assert_eq!(
            flags,
            [
                (0, false, false),
                (4, true, false),
                (9, true, true),
                (11, false, true)
            ]
        );
        assert_eq!(c.exits, [7, 12]);
    }

    #[test]
    #[ignore = "needs the original OMSI 2 install (OMSI_ROOT)"]
    fn stock_sd200_entry_flags() {
        // the stock SD200: its second entry (path point 4) sells no tickets
        let root = original_root();
        let c = PassengerCabin::load(&root.join("Vehicles/MAN_SD200/Model/passengercabin.cfg"))
            .expect("SD200 passengercabin.cfg");
        assert_eq!(
            c.entries
                .iter()
                .map(|e| (e.path_point, e.no_ticket_sale))
                .collect::<Vec<_>>(),
            [(0, false), (4, true)]
        );
    }

    /// #721: a `[passpos]` may name a variable that switches it on and off and one the
    /// engine writes its occupancy into, on the lines straight after its values.
    #[test]
    fn a_place_may_name_its_switch_and_occupancy_variables() {
        let text = "[passpos]\n0.94\n0.04\n0.92\n0.43\n0\nseat_folded_down\nseat_taken\n\n\
                    [passpos]\n0.5\n2\n1\n0.5\n0\nlayout_long\n\n[passpos]\n0.5\n1\n1\n0.5\n0\n\n\
                    [passpos]\n0.5\n0\n1\n0.5\n0\n[entry]\n0\n{noticketsale}\n[drivpos]\n-0.8\n4.5\n1.0\n0.5\n0\nnot_a_place_var\n";
        let c = PassengerCabin::parse(&CfgFile::from_str("passengercabin.cfg", text));
        let vars: Vec<(Option<&str>, Option<&str>)> = c
            .pass_positions
            .iter()
            .map(|p| (p.switch_var.as_deref(), p.taken_var.as_deref()))
            .collect();
        assert_eq!(
            vars,
            [
                (Some("seat_folded_down"), Some("seat_taken")),
                (Some("layout_long"), None),
                (None, None),
                (None, None)
            ]
        );
        assert_eq!(c.pass_positions[0].pos, [0.94, 0.04, 0.92]);
        assert_eq!(c.entries[0].path_point, 0);
        assert!(c.entries[0].no_ticket_sale);
        assert_eq!(c.driver_positions[0].switch_var, None);
    }

    #[test]
    fn seats_are_numbered_with_the_drivers_place() {
        let text = "[drivpos]\n-0.8\n4.5\n1.0\n0.5\n0\n\n[passpos]\n0.5\n2\n1\n0.5\n0\n\n\
                    [passpos]\n0.5\n1\n1\n0.5\n0\n\n[drivpos]\n0.8\n4.5\n1.0\n0.5\n0\n\n[passpos]\n0.5\n0\n1\n0.5\n0\n";
        let c = PassengerCabin::parse(&CfgFile::from_str("passengercabin.cfg", text));
        let seats: Vec<usize> = c.pass_positions.iter().map(|p| p.file_index).collect();
        assert_eq!(seats, [1, 2, 4]);
        assert_eq!(
            c.driver_positions
                .iter()
                .map(|p| p.file_index)
                .collect::<Vec<_>>(),
            [0, 3]
        );
    }
}
