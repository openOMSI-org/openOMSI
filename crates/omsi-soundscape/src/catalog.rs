//! Which scenery objects the ambience may make heard, and as what.
//!
//! Only what the map's authors said an object is: the `[groups]` an object is filed under in
//! the editor ("Church", "Education", "Stations", "Ambient Sounds" …) and its
//! `[friendlyname]` ("Petrol Station Type 2 "BP"", "Kindergarten 1", "Falkensee Fitter's
//! Shop"). A file's name alone decides nothing: `Taubenhaus.sco` is an old cottage and
//! `DDR_Industrial_01.sco` a small office building, and neither coos nor hums. An object that
//! carries a `[sound]` of its own (a transformer hut, a level crossing) keeps it and gets
//! nothing from us.

use omsi_scenery::sco::SceneryObject;

/// The kinds of [`omsi_geometry::SoundSpot`] (their `kind` number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum Spot {
    /// An object with a `[sound]` of its own: nothing of ours plays on top of it.
    OwnSound = 1,
    /// OMSI's ambient sound objects (`Sceneryobjects\Generic\sound_*.sco`): the map's own
    /// birds of the wood, of the field, its rooster - ours of the same kind make way.
    OmsiForest,
    OmsiField,
    OmsiRooster,
    ChurchProtestant,
    ChurchCatholic,
    /// A church whose denomination the map does not say.
    Church,
    School,
    Kindergarten,
    PetrolStation,
    Supermarket,
    SnackBar,
    Pool,
    SportsGround,
    Tennis,
    Farm,
    Cows,
    BusDepot,
    Workshop,
    RoadWorks,
    Substation,
    SodiumLamp,
    Station,
    /// A Berlin S-Bahn station: its door signal too.
    SBahnStation,
    PedestrianSignal,
}

impl Spot {
    pub const ALL: [Spot; 25] = [
        Spot::OwnSound,
        Spot::OmsiForest,
        Spot::OmsiField,
        Spot::OmsiRooster,
        Spot::ChurchProtestant,
        Spot::ChurchCatholic,
        Spot::Church,
        Spot::School,
        Spot::Kindergarten,
        Spot::PetrolStation,
        Spot::Supermarket,
        Spot::SnackBar,
        Spot::Pool,
        Spot::SportsGround,
        Spot::Tennis,
        Spot::Farm,
        Spot::Cows,
        Spot::BusDepot,
        Spot::Workshop,
        Spot::RoadWorks,
        Spot::Substation,
        Spot::SodiumLamp,
        Spot::Station,
        Spot::SBahnStation,
        Spot::PedestrianSignal,
    ];

    pub fn from_u16(k: u16) -> Option<Spot> {
        Spot::ALL.iter().copied().find(|s| *s as u16 == k)
    }

    pub fn name(self) -> &'static str {
        match self {
            Spot::OwnSound => "own sound",
            Spot::OmsiForest => "map's forest sound",
            Spot::OmsiField => "map's field sound",
            Spot::OmsiRooster => "map's rooster",
            Spot::ChurchProtestant => "Protestant church",
            Spot::ChurchCatholic => "Catholic church",
            Spot::Church => "church",
            Spot::School => "school",
            Spot::Kindergarten => "kindergarten",
            Spot::PetrolStation => "petrol station",
            Spot::Supermarket => "supermarket",
            Spot::SnackBar => "snack bar",
            Spot::Pool => "swimming pool",
            Spot::SportsGround => "sports ground",
            Spot::Tennis => "tennis courts",
            Spot::Farm => "farm building",
            Spot::Cows => "cows",
            Spot::BusDepot => "bus depot",
            Spot::Workshop => "workshop",
            Spot::RoadWorks => "road works",
            Spot::Substation => "substation",
            Spot::SodiumLamp => "sodium lamp",
            Spot::Station => "railway station",
            Spot::SBahnStation => "S-Bahn station",
            Spot::PedestrianSignal => "pedestrian signal",
        }
    }
}

/// What an object is to the ambience, if anything.
pub fn classify(sco: &SceneryObject) -> Option<Spot> {
    let stem = sco
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let groups: Vec<String> = sco
        .groups
        .iter()
        .map(|g| g.trim().to_ascii_lowercase())
        .collect();
    let name = sco.friendly_name.to_ascii_lowercase();
    classify_by(&stem, &groups, &name, sco.sound.is_some())
}

/// [`classify`] on its pieces: the file's stem, the groups and the friendly name (all lower
/// case), and whether the object has a `[sound]` of its own.
pub fn classify_by(stem: &str, groups: &[String], name: &str, own_sound: bool) -> Option<Spot> {
    let group = |g: &str| {
        groups
            .iter()
            .any(|x| x == g || x.starts_with(&format!("{g} ")))
    };
    let says = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if group("ambient sounds") {
        return Some(if stem.contains("forest") || stem.contains("wald") {
            Spot::OmsiForest
        } else if stem.contains("field") || stem.contains("feld") {
            Spot::OmsiField
        } else if stem.contains("cock") || stem.contains("hahn") || stem.contains("rooster") {
            Spot::OmsiRooster
        } else {
            Spot::OwnSound
        });
    }
    if own_sound {
        return Some(Spot::OwnSound);
    }
    // (a sign or a road named after a church is not one)
    let signs = groups.iter().any(|g| {
        g.contains("sign")
            || g.contains("zeichen")
            || g.contains("road objects")
            || g.contains("markings")
    });
    let named_church = !signs
        && (name.starts_with("church")
            || name.starts_with("st ") && name.contains("church")
            || says(&[
                "cathedral",
                " church",
                "chapel",
                "kirche",
                "kapelle",
                "münster",
                "dom ",
            ]));
    if group("church") || group("churches") || group("kirche") || group("kirchen") || named_church {
        let catholic = says(&["kath", "cathol", "roman"]);
        let protestant = says(&["ev.", "evang", "protest", "luther"]) || name.starts_with("ev ");
        return Some(if catholic {
            Spot::ChurchCatholic
        } else if protestant {
            Spot::ChurchProtestant
        } else {
            Spot::Church
        });
    }
    if group("stations")
        || group("bahnhof")
        || group("bahnhöfe")
        || (!signs && says(&["train station", "railway station", "s-bahnhof", "u-bahnhof"]))
    {
        return Some(if says(&["s-bhf", "s-bahn"]) || stem.starts_with("s-bhf") {
            Spot::SBahnStation
        } else {
            Spot::Station
        });
    }
    if group("education") || group("schools") {
        // (filed under "Education" too: the sports halls, the pool, the sports ground)
        if says(&["kindergarten", "kita", "nursery"]) {
            return Some(Spot::Kindergarten);
        }
        if says(&[
            "swimming pool",
            "swimming-pool",
            "freibad",
            "schwimmbad",
            "lido",
            "open air pool",
        ]) {
            return Some(Spot::Pool);
        }
        if says(&["tennis"]) {
            return Some(Spot::Tennis);
        }
        if says(&[
            "sports ground",
            "sportsground",
            "sportplatz",
            "football",
            "stadium",
            "stadion",
        ]) {
            return Some(Spot::SportsGround);
        }
        if says(&["school", "schule", "schulzentrum", "gymnasium,"])
            && !says(&["gymnasium, concrete", "gym hall", "sporthalle", "turnhalle"])
        {
            return Some(Spot::School);
        }
        return None;
    }
    if says(&[
        "petrol station",
        "gas station",
        "filling station",
        "tankstelle",
    ]) {
        return Some(Spot::PetrolStation);
    }
    if says(&["supermarket", "supermarkt", "kaufhalle", "kaufland"]) {
        return Some(Spot::Supermarket);
    }
    if says(&[
        "snack bar",
        "imbiss",
        "ice cream parlour",
        "ice-cream parlour",
        "ice cream parlor",
        "eisdiele",
        "chip shop",
        "takeaway",
        "take-away",
    ]) {
        return Some(Spot::SnackBar);
    }
    if says(&["bus depot", "betriebshof", "omnibushof", "bus garage"]) {
        return Some(Spot::BusDepot);
    }
    if says(&[
        "fitter's shop",
        "fitters shop",
        "schlosserei",
        "metalwork",
        "blacksmith",
        "schmiede",
    ]) {
        return Some(Spot::Workshop);
    }
    if says(&["substation", "transformer", "umspannwerk", "trafostation"]) {
        return Some(Spot::Substation);
    }
    if group("animals") && says(&["cow", "kuh", "kühe", "cattle", "rinder"]) {
        return Some(Spot::Cows);
    }
    if (group("agricultural") || group("agriculture") || group("landwirtschaft"))
        && says(&["barn", "scheune", "stall", "farm", "hof"])
    {
        return Some(Spot::Farm);
    }
    if group("road works") || group("baustelle") {
        return Some(Spot::RoadWorks);
    }
    if group("lights") && (stem.contains("sodium") || says(&["sodium", "natrium"])) {
        return Some(Spot::SodiumLamp);
    }
    // (a British signalled crossing: the pelican, puffin and toucan crossings beep for the
    // blind while the green man shows)
    if says(&["pelican crossing", "puffin crossing", "toucan crossing"]) {
        return Some(Spot::PedestrianSignal);
    }
    if group("traffic lights")
        && (stem.starts_with("ampel_mensch") || says(&["traffic light humans", "pedestrian"]))
    {
        return Some(Spot::PedestrianSignal);
    }
    None
}

/// Whether a tree type is a conifer: filed under "Indeciduous" (OMSI's group for the
/// evergreens), or else a fir, pine, spruce by its name or texture.
pub fn is_conifer(sco: &SceneryObject, texture: &str) -> bool {
    let groups: Vec<String> = sco
        .groups
        .iter()
        .map(|g| g.trim().to_ascii_lowercase())
        .collect();
    if groups
        .iter()
        .any(|g| g == "indeciduous" || g == "conifers" || g == "nadelbäume" || g == "nadelbaeume")
    {
        return true;
    }
    if groups
        .iter()
        .any(|g| g == "deciduous" || g == "laubbäume" || g == "laubbaeume")
    {
        return false;
    }
    let text = format!(
        "{} {} {}",
        sco.path.to_string_lossy(),
        sco.friendly_name,
        texture
    )
    .to_ascii_lowercase();
    [
        "fir", "pine", "spruce", "conifer", "tanne", "fichte", "kiefer", "nadel", "thuja",
        "cypress", "zypresse", "yew", "eibe",
    ]
    .iter()
    .any(|w| text.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_ascii_lowercase()).collect()
    }

    #[test]
    fn the_stock_objects_as_their_authors_filed_them() {
        let c = |stem: &str, groups: &[&str], name: &str| {
            classify_by(stem, &g(groups), &name.to_ascii_lowercase(), false)
        };
        assert_eq!(
            c(
                "kirche_ev_stnikolai",
                &["Buildings", "Church"],
                "ev. St. Nikolai Spandau"
            ),
            Some(Spot::ChurchProtestant)
        );
        assert_eq!(
            c(
                "kirche_kath_stmarien",
                &["Buildings", "Church"],
                "kath. St. Marien Spandau"
            ),
            Some(Spot::ChurchCatholic)
        );
        assert_eq!(
            c(
                "ddr_ev_kirche_seegefeld",
                &["Buildings", "Church DDR"],
                "Ev. Kirche Seegefeld"
            ),
            Some(Spot::ChurchProtestant)
        );
        assert_eq!(
            c("kirche_ev_01", &["Buildings", "Church"], "Small ev. 01"),
            Some(Spot::ChurchProtestant)
        );
        assert_eq!(
            c("bw_school1", &["Buildings", "Education"], "School 1"),
            Some(Spot::School)
        );
        assert_eq!(
            c(
                "schulzentrum",
                &["Buildings", "Education"],
                "School Center Spektefeld"
            ),
            Some(Spot::School)
        );
        assert_eq!(
            c(
                "schul_turn_01",
                &["Buildings", "Education"],
                "Gymnasium, Concrete"
            ),
            None,
            "a sports hall"
        );
        assert_eq!(
            c("bw_kinder1", &["Buildings", "Education"], "Kindergarten 1"),
            Some(Spot::Kindergarten)
        );
        assert_eq!(
            c(
                "kombibad",
                &["Buildings", "Education"],
                "Indoor/Open Air Swimming Pool"
            ),
            Some(Spot::Pool)
        );
        assert_eq!(
            c(
                "sportsground_big",
                &["Buildings", "Education"],
                "Sports Ground big"
            ),
            Some(Spot::SportsGround)
        );
        assert_eq!(
            c(
                "tennis-center",
                &["Buildings", "Education"],
                "Tennis Center"
            ),
            Some(Spot::Tennis)
        );
        assert_eq!(
            c(
                "tank_2_bp",
                &["Buildings", "Automobile"],
                "Petrol Station Type 2 \"BP\""
            ),
            Some(Spot::PetrolStation)
        );
        assert_eq!(
            c(
                "bw_supermarkt_1",
                &["Buildings", "Commercial"],
                "Supermarket 1 - Reichelt"
            ),
            Some(Spot::Supermarket)
        );
        assert_eq!(
            c(
                "ddr_falkensee_kaufhalle",
                &["Buildings", "Commercial DDR"],
                "Falkensee Kaufhalle"
            ),
            Some(Spot::Supermarket)
        );
        assert_eq!(
            c(
                "ddr_imbissbude",
                &["Buildings", "Commercial DDR"],
                "DDR Snack Bar"
            ),
            Some(Spot::SnackBar)
        );
        assert_eq!(
            c(
                "ddr_falkensee_eisdiele",
                &["Buildings", "Commercial DDR"],
                "Falkensee Ice Cream Parlour"
            ),
            Some(Spot::SnackBar)
        );
        assert_eq!(
            c(
                "omnibushof_s_1",
                &["Buildings", "BVG"],
                "Spandau Bus Depot Pt 1"
            ),
            Some(Spot::BusDepot)
        );
        assert_eq!(
            c(
                "ddr_falkensee_schlosserei",
                &["Buildings", "Industrial DDR"],
                "Falkensee Fitter's Shop"
            ),
            Some(Spot::Workshop)
        );
        assert_eq!(
            c(
                "s-bhf_spd-west",
                &["Railroad", "Stations"],
                "S-Bhf. Spandau-West"
            ),
            Some(Spot::SBahnStation)
        );
        assert_eq!(
            c(
                "u-bhf ruhleben",
                &["Railroad", "Stations"],
                "U-Bhf. Ruhleben"
            ),
            Some(Spot::Station)
        );
        assert_eq!(c("cows", &["Animals"], "Group of 5 cows"), Some(Spot::Cows));
        assert_eq!(
            c("ddr_barn_07", &["Buildings", "Agricultural DDR"], "Barn 07"),
            Some(Spot::Farm)
        );
        assert_eq!(
            c(
                "sodiumlight_l_break_l",
                &["German Street Side", "Lights"],
                "Large Sodium on large break pole"
            ),
            Some(Spot::SodiumLamp)
        );
        assert_eq!(
            c(
                "ampel_mensch_1",
                &["German Street Side", "Traffic Lights"],
                "Traffic Light Humans"
            ),
            Some(Spot::PedestrianSignal)
        );
        // what the names would have got wrong
        assert_eq!(
            c("taubenhaus", &["Buildings", "Residential"], "Old Cottage"),
            None
        );
        assert_eq!(
            c(
                "ddr_industrial_01",
                &["Buildings", "Industrial DDR"],
                "Small office building"
            ),
            None
        );
        assert_eq!(
            c(
                "bw_workshop_01",
                &["Buildings", "Industrial"],
                "Workshop 01"
            ),
            None,
            "a generic block, placed 135 times"
        );
        assert_eq!(
            c(
                "bw_wkrank_a",
                &["Buildings", "Administration"],
                "Waldkrankenhaus-A"
            ),
            None
        );
        // a British map (Cotterell): filed under its author's own groups
        assert_eq!(
            c(
                "st+helen+skipwith",
                &["UKDT", "Sketchup Conversions", "G3FX"],
                "Church - St Helen Skipwith"
            ),
            Some(Spot::Church)
        );
        assert_eq!(
            c(
                "pelicancrossing",
                &["Cotterell", "Junctions"],
                "Pelican Crossing (Paths Only)"
            ),
            Some(Spot::PedestrianSignal)
        );
        assert_eq!(
            c(
                "train station a",
                &["Cotterell", "Junctions"],
                "Train Station A"
            ),
            Some(Spot::Station)
        );
        assert_eq!(
            c(
                "cotterellbusstnsign",
                &["UKDT", "Cotterell"],
                "Bus Station Sign"
            ),
            None
        );
        assert_eq!(
            c(
                "p544_zebra_crossing",
                &["Road-hog123", "Road Objects", "Road Signs", "Warning"],
                "Zebra Crossing Ahead"
            ),
            None
        );
        assert_eq!(
            c(
                "church_rd_sign",
                &["Road Objects", "Road Signs"],
                "Church Road"
            ),
            None
        );
    }

    #[test]
    fn the_maps_own_sounds_win() {
        let own = |stem: &str| classify_by(stem, &g(&["Ambient Sounds"]), "", true);
        assert_eq!(own("sound_forest"), Some(Spot::OmsiForest));
        assert_eq!(own("sound_field"), Some(Spot::OmsiField));
        assert_eq!(own("sound_cock"), Some(Spot::OmsiRooster));
        // a transformer hut with its own hum: nothing of ours
        assert_eq!(
            classify_by(
                "trafohaus",
                &g(&["German Street Side", "Miscellaneous"]),
                "transformator hut",
                true
            ),
            Some(Spot::OwnSound)
        );
    }

    #[test]
    fn every_kind_has_a_number_of_its_own() {
        for s in Spot::ALL {
            assert_eq!(Spot::from_u16(s as u16), Some(s));
        }
    }
}
