//! The livery of the vehicle the player drives: its file, its paint, its fleet number and
//! its plate. The AI's buses of the same file leave them alone (#3477): a bus of one's own
//! repaint standing at the next stop with one's own plate reads as a copy of one's own bus.

use std::path::PathBuf;

/// What a vehicle shows to tell it from the others of its type: the file (as the vehicle
/// lists and the ailists name it, under the content root), the paint scheme's name (the
/// model's own paint when it has none of the list) and the fleet number and plate the
/// scripts' `number` and `ident` strings hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerLivery {
    pub bus: PathBuf,
    pub paint: String,
    pub number: String,
    pub ident: String,
}

impl PlayerLivery {
    /// What `v` shows.
    pub fn of(v: &crate::VehicleInstance) -> PlayerLivery {
        PlayerLivery {
            bus: v.ty.def.path.clone(),
            paint: v
                .host
                .paint_scheme
                .flatten()
                .and_then(|i| v.ty.paint_schemes.get(i))
                .map(|s| s.name.clone())
                .unwrap_or_else(|| v.ty.def.default_paint.clone()),
            number: v.number(),
            ident: v.ident(),
        }
    }

    /// Is `ty` the player's own vehicle file?
    pub fn is_bus_of(&self, ty: &crate::VehicleType) -> bool {
        self.bus == ty.def.path
    }

    /// Does `name` (a paint scheme's name, or a `[aigroup_depot_typgroup_2]` line's repaint)
    /// name the player's paint? An unknown paint (no name at all) is nobody's.
    pub fn is_paint(&self, name: &str) -> bool {
        let mine = self.paint.trim();
        !mine.is_empty() && mine.eq_ignore_ascii_case(name.trim())
    }

    /// Does `number` or `plate` name the player's own fleet number or plate?
    pub fn is_number_or_plate(&self, number: &str, plate: &str) -> bool {
        let (mine, plate_mine) = (self.number.trim(), self.ident.trim());
        (!mine.is_empty() && mine.eq_ignore_ascii_case(number.trim()))
            || (!plate_mine.is_empty() && plate_mine.eq_ignore_ascii_case(plate.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn livery() -> PlayerLivery {
        PlayerLivery {
            bus: PathBuf::from("Vehicles/MAN_SD200/sd200.bus"),
            paint: "BVG Gelb".into(),
            number: "1234".into(),
            ident: "B-V 1234".into(),
        }
    }

    #[test]
    fn a_livery_names_its_own_paint_number_and_plate() {
        let l = livery();
        assert!(l.is_paint("BVG Gelb") && l.is_paint(" bvg gelb "));
        assert!(!l.is_paint("BVG Rot") && !l.is_paint(""));
        assert!(l.is_number_or_plate("1234", "B-V 1234"));
        assert!(l.is_number_or_plate(" 1234 ", ""));
        assert!(l.is_number_or_plate("", "b-v 1234"));
        assert!(!l.is_number_or_plate("5678", "B-V 5678"));
    }

    #[test]
    fn an_unknown_paint_or_number_is_nobodys() {
        let l = PlayerLivery {
            paint: "  ".into(),
            number: String::new(),
            ident: String::new(),
            ..livery()
        };
        assert!(!l.is_paint("BVG Gelb"));
        assert!(!l.is_number_or_plate("1234", "B-V 1234"));
    }
}
