//! openOMSI's driver record: the career kept in the driver's personnel file (`career`: the
//! distance, the stops served on time, the tickets, the crashes and the ratings made of
//! them) and the clock that follows this device's (`real_time`). Nothing here knows the
//! game's `App`, the renderer or the network.

pub mod career;
pub mod economy;
pub mod real_time;
