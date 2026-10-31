//! A candidate room owns its transport until the running game accepts the handover.

use omsi_net::{LanSession, Pose, WorldInfo};
use std::time::{Duration, Instant};

pub struct PreparedJoin {
    pub session: LanSession,
    target: String,
    vehicle_offers: Option<std::sync::mpsc::Receiver<bool>>,
    pub vehicle_allowed: Option<bool>,
    client: Option<(omsi_net::ws::WsClient, String)>,
}

impl PreparedJoin {
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Establish a new room without changing the active room's gateway or offers.
    pub fn connect(
        target: &str,
        name: &str,
        world: WorldInfo,
        planned: &Pose,
    ) -> Result<Self, String> {
        Self::connect_with_wait(target, name, world, planned, Duration::from_secs(10))
    }

    /// Keep the hosted room alive when preparing a handover to another room.
    pub fn connect_for_handover(
        target: &str,
        name: &str,
        world: WorldInfo,
        planned: &Pose,
        hosted_session: Option<u64>,
    ) -> Result<Self, String> {
        const SELF_JOIN: &str = "You cannot join the room you are hosting";
        if omsi_net::SessionCode::decode(target.trim())
            .ok()
            .is_some_and(|code| hosted_session == Some(code.session))
        {
            return Err(SELF_JOIN.into());
        }
        let mut candidate = Self::connect(target, name, world, planned)?;
        // Addresses and gateways need not include a code; check the negotiated identity too.
        if hosted_session == Some(candidate.session.session) {
            return Err(SELF_JOIN.into());
        }
        candidate.ask_vehicle_offers(&planned.bus);
        Ok(candidate)
    }

    fn ask_vehicle_offers(&mut self, bus: &str) {
        if bus.is_empty() {
            return;
        }
        let target = self
            .client
            .as_ref()
            .map(|(_, url)| url.clone())
            .or_else(|| self.session.host.map(|host| host.to_string()))
            .unwrap_or_else(|| self.target.clone());
        let bus = bus.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .name("candidate buses".into())
            .spawn(move || {
                if let Some(allowed) = crate::gateway::candidate_bus_allowed(&target, &bus) {
                    let _ = tx.send(allowed);
                }
            })
            .is_ok()
        {
            self.vehicle_offers = Some(rx);
        }
    }

    /// Candidate offers never replace the active room's vehicle list.
    pub fn poll_vehicle_offers(&mut self) {
        let Some(rx) = self.vehicle_offers.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(allowed) => self.vehicle_allowed = Some(allowed),
            Err(std::sync::mpsc::TryRecvError::Empty) => self.vehicle_offers = Some(rx),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
        }
    }

    fn connect_with_wait(
        target: &str,
        name: &str,
        world: WorldInfo,
        planned: &Pose,
        wait: Duration,
    ) -> Result<Self, String> {
        let target = target.trim();
        if target.is_empty() || target.len() > 2048 || target.chars().any(char::is_control) {
            return Err("Enter a valid connection code or server address".into());
        }
        let target = omsi_net::official::resolve_target(target)?;
        crate::bans::announce_local_hardware_id();
        let mut candidate = Self::open(&target, name, world.clone())?;
        match candidate.wait(planned, wait) {
            Ok(()) => Ok(candidate),
            Err(error)
                if candidate.session.turned_away.is_none()
                    && candidate.session.welcome.is_none() =>
            {
                let tunnel = omsi_net::SessionCode::decode(&target)
                    .ok()
                    .and_then(|code| omsi_net::bridge::lookup_tunnel(code.session));
                let Some(tunnel) = tunnel else {
                    return Err(error);
                };
                let mut fallback = Self::open(&tunnel, name, world)?;
                fallback.target = target;
                fallback.wait(planned, wait)?;
                Ok(fallback)
            }
            Err(error) => Err(error),
        }
    }

    fn open(target: &str, name: &str, world: WorldInfo) -> Result<Self, String> {
        let (direct, client) = match omsi_net::ws::ws_url(target) {
            Some(url) => {
                let client = omsi_net::ws::WsClient::connect(&url)?;
                (client.local.to_string(), Some((client, url)))
            }
            None => (target.to_string(), None),
        };
        let session = LanSession::join(&direct, name, world, Duration::from_secs(3))?;
        Ok(Self {
            session,
            target: target.to_string(),
            vehicle_offers: None,
            vehicle_allowed: None,
            client,
        })
    }

    fn wait(&mut self, planned: &Pose, wait: Duration) -> Result<(), String> {
        let started = Instant::now();
        while started.elapsed() < wait {
            self.session.tick(0.02, planned);
            if let Some(why) = self
                .session
                .turned_away
                .as_ref()
                .or(self.session.rejected.as_ref())
            {
                return Err(why.clone());
            }
            if self.session.connected && self.session.welcome.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("The room did not answer; the current game has not been changed".into())
    }

    /// Commit only after the game has accepted the room and any required map change.
    pub fn accept(self) -> LanSession {
        crate::gateway::replace_join_transport(self.client);
        let offers = crate::gateway::new_offers_session();
        crate::gateway::ask_server_offers(&self.target, offers);
        self.session
    }
}

#[cfg(test)]
mod tests;
