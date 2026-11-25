//! `.otp` ticket packs and `.cti` ticket texture items.

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ticket {
    pub name: String,
    pub name_english: String,
    pub max_stations: i32,
    pub age_min: i32,
    pub age_max: i32,
    pub value: f32,
    pub display_string: String,
    pub day_ticket: bool,
    pub probability: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TicketPack {
    pub path: PathBuf,
    pub stamper_prop: f32,
    pub ticketbuy_prop: f32,
    pub chattiness: f32,
    pub whinge_prop: f32,
    pub voice_path: Option<String>,
    pub tickets: Vec<Ticket>,
}

impl TicketPack {
    pub fn load(path: &Path) -> Result<TicketPack, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut p = TicketPack {
            path: f.path.clone(),
            ..Default::default()
        };
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "ticketpack" => {
                    p.stamper_prop = r.f32();
                    p.ticketbuy_prop = r.f32();
                    p.chattiness = r.f32();
                    p.whinge_prop = r.f32();
                }
                "voicepath" => p.voice_path = Some(r.str().to_string()),
                "ticket" | "ticket_2" => {
                    let mut t = Ticket {
                        name: r.str().to_string(),
                        name_english: r.str().to_string(),
                        max_stations: r.i32(),
                        age_min: r.i32(),
                        age_max: r.i32(),
                        value: r.f32(),
                        display_string: r.str().to_string(),
                        day_ticket: false,
                        probability: 1.0,
                    };
                    if k == "ticket_2" {
                        t.day_ticket = r.bool();
                        t.probability = r.f32();
                    }
                    p.tickets.push(t);
                }
                _ => {}
            }
        }
        Ok(p)
    }
}

/// `.cti`: `[item] name variable texture` and `[setvar] name value`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TicketItems {
    pub items: Vec<(String, String, String)>,
    pub set_vars: Vec<(String, f32)>,
}

impl TicketItems {
    pub fn load(path: &Path) -> Result<TicketItems, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut t = TicketItems::default();
        let mut r = f.reader();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "item" => t.items.push((
                    r.str().to_string(),
                    r.str().to_string(),
                    r.str().to_string(),
                )),
                "setvar" => t.set_vars.push((r.str().to_string(), r.f32())),
                _ => {}
            }
        }
        Ok(t)
    }
}
