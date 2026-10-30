//! A driver's ownership, the pedestrian and the camera are independent states.

mod give;
mod picking;
mod retained;
mod selection;
mod take;

pub(crate) use retained::{exchange_placed_duty, SavedDuty};
use selection::Target;
mod route;

use crate::App;
use omsi_ui::tr;
use std::sync::Arc;

pub(crate) struct Request {
    pub(crate) target: Option<Target>,
    world: Arc<crate::scene::World>,
}

pub(crate) fn road_bus(ty: &omsi_sim::VehicleType) -> bool {
    ty.def
        .path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("bus"))
        && !crate::rail_drive::is_rail(&ty.def)
        && !ty.def.cameras_driver.is_empty()
}

impl App {
    pub(crate) fn give_ai_reason(&self) -> Option<&'static str> {
        if self.net.lan.is_some() {
            return Some("Vehicle handover is only available in single-player");
        }
        if !self.settings.get_up {
            return Some("Enable Ability to get up before handing the bus to AI");
        }
        let Some(p) = self.player.as_ref() else {
            return Some("There is no player bus to hand over");
        };
        if !road_bus(&p.vehicle.ty) {
            return Some("Only driveable road buses can be handed over");
        }
        if p.vehicle.physics.speed.abs() > 0.14 {
            return Some("Stop the bus before handing it to AI");
        }
        if p.startup.is_some() {
            return Some("Wait for the vehicle start-up to finish");
        }
        if self.world.is_none()
            || self.session.traffic.is_none()
            || self.renderer.is_none()
            || self.scene.is_none()
        {
            return Some("The world and traffic must be ready before vehicle handover");
        }
        if self.menus.game_panels.handover.is_some() {
            return Some("A vehicle handover is already pending");
        }
        None
    }

    pub(crate) fn ask_give_ai(&mut self) {
        if let Some(reason) = self.give_ai_reason() {
            self.service_msg = Some((tr(reason).into_owned(), 5.0));
            return;
        }
        if let Some(world) = self.world.clone() {
            self.menus.game_panels.handover = Some(Request {
                target: None,
                world,
            });
        }
    }

    pub(crate) fn handover_click(&mut self, action: &str) {
        let Some(request) = self.menus.game_panels.handover.take() else {
            return;
        };
        if action != "yes" {
            return;
        }
        if !self
            .world
            .as_ref()
            .is_some_and(|w| Arc::ptr_eq(w, &request.world))
        {
            self.service_msg = Some((
                tr("The world changed; vehicle handover was cancelled").into_owned(),
                5.0,
            ));
            return;
        }
        self.join_sim_thread();
        let result = match request.target {
            Some(Target::Ai(id)) => self.take_ai_bus(id),
            Some(Target::Placed(uid)) => self.take_retained_bus(uid),
            None => self.give_bus_to_ai(),
        };
        match result {
            Ok(()) => {}
            Err(reason) => self.service_msg = Some((tr(&reason).into_owned(), 6.0)),
        }
    }
}

pub(crate) fn panel(app: &App, request: &Request, light: bool) -> omsi_plugin::ui::Panel {
    use crate::game_panels::{button, card, grow_row, note, title};
    let (heading, detail) = match request.target {
        Some(Target::Placed(_)) => (
            "Drive this parked bus?",
            "You will return to its driver seat. Your current bus stays where it is.",
        ),
        Some(Target::Ai(_)) => (
            "Take over this AI bus?",
            "You will become its driver. Your previous bus stays where it is.",
        ),
        None if app.session.duty.is_some() => (
            "Hand this bus to AI?",
            "AI will continue the timetable. You will stand safely outside the bus.",
        ),
        None => (
            "Hand this bus to AI?",
            "AI will drive away and leave traffic out of sight. You will be on foot.",
        ),
    };
    let mut children = vec![title(&tr(heading)), note(&tr(detail), light)];
    if let Some(car) = request.target.and_then(|target| match target {
        Target::Ai(id) => app
            .session
            .traffic
            .as_ref()?
            .cars
            .iter()
            .find(|c| c.id == id),
        Target::Placed(_) => None,
    }) {
        children.push(note(
            &format!(
                "{} {} — {}",
                car.vehicle.ty.def.manufacturer,
                car.vehicle.ty.def.type_name,
                car.vehicle.host.tt_line
            ),
            light,
        ));
    }
    if let Some(Target::Placed(uid)) = request.target {
        if let Some(p) = app.session.placed.iter().find(|p| p.uid == uid) {
            children.push(note(
                &format!(
                    "{} {}",
                    p.vehicle.ty.def.manufacturer, p.vehicle.ty.def.type_name
                ),
                light,
            ));
        }
    }
    children.push(grow_row(vec![
        button("yes", &tr("Confirm"), Some("check")),
        button("no", &tr("Cancel"), Some("close")),
    ]));
    card(
        omsi_plugin::ui::Anchor::Center,
        0.0,
        0.0,
        440.0,
        240,
        light,
        children,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn handover_is_unavailable_without_a_player_bus() {
        let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
        let mut app = crate::assemble_app(args, Default::default());
        app.settings.get_up = true;
        assert_eq!(
            app.give_ai_reason(),
            Some("There is no player bus to hand over")
        );
        app.ask_give_ai();
        assert!(app.menus.game_panels.handover.is_none());
    }
}
