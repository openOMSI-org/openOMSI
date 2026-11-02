//! App command-line adapter for the reusable rail driver.

pub(crate) use omsi_driver::rail_drive::*;

pub(crate) fn args_rail(args: &crate::Args) -> bool {
    args.bus
        .as_deref()
        .map(|b| omsi_cfg::resolve_path(&args.root, b))
        .and_then(|p| omsi_vehicle::Vehicle::load(&p).ok())
        .is_some_and(|d| is_rail(&d))
}
