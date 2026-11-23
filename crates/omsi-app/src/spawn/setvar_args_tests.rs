//! `--setvar name=value[,name=value…]` as the bus's own variables: the launcher's onboard
//! computer (an IBIS unit, a ticket printer) reaches the vehicle as one (#4077).
use super::Args;
use super::requested_vars;
use clap::Parser;

#[test]
fn a_setvar_list_is_split_into_the_bus_variables() {
    let args = Args::parse_from([
        "openomsi",
        "--bus",
        "Vehicles/x.bus",
        "--setvar",
        "vis_ibis_type=1, v_drzwi=0.5",
    ]);
    assert_eq!(
        requested_vars(&args),
        vec![
            ("vis_ibis_type".to_string(), 1.0),
            ("v_drzwi".to_string(), 0.5)
        ]
    );
}

#[test]
fn a_start_without_setvars_asks_for_none() {
    let args = Args::parse_from(["openomsi", "--bus", "Vehicles/x.bus"]);
    assert!(requested_vars(&args).is_empty());
    // (a word without its value is no variable, and neither is a name with nothing before the =)
    let args = Args::parse_from([
        "openomsi",
        "--bus",
        "Vehicles/x.bus",
        "--setvar",
        "novalue,=1",
    ]);
    assert!(requested_vars(&args).is_empty());
}
