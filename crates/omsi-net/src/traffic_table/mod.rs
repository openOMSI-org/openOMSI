//! The traffic table: every AI vehicle of the host's world as one row of data under an id of
//! its own, and how it reaches the clients.
//!
//! * [`TrafficTable`] (the host): the simulation writes it after each of its steps. Rows
//!   keep their ids for as long as the vehicle lives; a freed id rests for
//!   [`REUSE_DELAY`](ids::REUSE_DELAY) s and comes back with the next generation.
//! * [`ClientSync`] (the host, one per client): which rows a client is told of (those within
//!   its range, the nearer ones more often), in what order and in how many bytes, as
//!   changes of the rows it acknowledged (`wire`).
//! * [`ClientTable`] (a client): the rows it was told of with their last poses, and where
//!   each is between them (`ClientTable::sample`).
//!
//! `docs/TRAFFIC_TABLE.md` has the format and the reasons for it. Nothing here knows of the
//! game: the host's game writes the table from its traffic simulation (`omsi-app`'s
//! `traffic_table`), a client's game makes its vehicles from the rows.

pub mod client;
pub mod ids;
pub mod loopback;
pub mod row;
pub mod server;
pub mod table;
pub mod wire;

#[cfg(test)]
mod tests;

pub use client::{ClientTable, Event, Pose};
pub use ids::{REUSE_DELAY, TrafficId};
pub use row::{Kind, Look, Meta, Row};
pub use server::ClientSync;
pub use table::TrafficTable;
