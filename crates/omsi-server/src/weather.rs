//! Server-only asynchronous METAR weather updates.
//!
//! A slow weather service never blocks the simulation tick.

use omsi_content::weather::Weather;
use omsi_net::LanSession;
use std::sync::mpsc::Receiver;
use std::time::Instant;

pub fn tick_metar(
    l: &mut LanSession,
    icao: Option<&String>,
    rx: &mut Option<Receiver<Option<Weather>>>,
    due: &mut Instant,
    weather_name: &mut String,
) {
    if let Some(icao) = icao {
        if rx.is_some() {
            let got = rx.as_ref().map(|rx| rx.try_recv());
            match got {
                Some(Ok(report)) => {
                    *rx = None;
                    // (a failed download is tried again in a minute)
                    let wait = if report.is_some() { 600 } else { 60 };
                    *due = std::time::Instant::now() + std::time::Duration::from_secs(wait);
                    if let Some(w) = report {
                        if let Some(wire) = crate::metar::report_wire(&w) {
                            if wire != l.weather() {
                                log::info!(
                                    "server: weather now the METAR report of {icao}: {wire}"
                                );
                                l.set_weather(&wire);
                            }
                            *weather_name = w.name.clone();
                        }
                    }
                }
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    *rx = None;
                    *due = std::time::Instant::now() + std::time::Duration::from_secs(60);
                }
                _ => {}
            }
        } else if std::time::Instant::now() >= *due {
            let (tx, receiver) = std::sync::mpsc::channel();
            *rx = Some(receiver);
            let icao = icao.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::metar::try_metar(&icao));
            });
        }
    }
}

/// Server-owned state for asynchronous METAR requests, never a UI dependency.
pub struct NativeWeather {
    station: Option<String>,
    due: Instant,
    receiver: Option<Receiver<Option<Weather>>>,
    name: String,
}

impl NativeWeather {
    pub fn new(station: Option<String>, initial: &str) -> Self {
        Self {
            station,
            due: Instant::now(),
            receiver: None,
            name: initial.to_owned(),
        }
    }

    pub fn tick(&mut self, session: &mut LanSession) {
        tick_metar(
            session,
            self.station.as_ref(),
            &mut self.receiver,
            &mut self.due,
            &mut self.name,
        );
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn station(&self) -> Option<&str> {
        self.station.as_deref()
    }
}
