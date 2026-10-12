//! RosterFlow's Preferences window (Optibus's): a search box and IN USE / ALL on the left, the
//! four kinds - VEHICLES, DRIVERS, DEPOTS, MISC - and their preferences under them; the one
//! chosen on the right, with what it sets and, for a rule, whether it is a hard rule (never
//! broken by Optimise, an error by hand) or a soft one that costs a penalty. SAVE CHANGES,
//! CANCEL, SAVE & CLOSE at the foot. Optimise and the validation go by them.

use super::super::super::ui::{id_of, Ui};
use super::super::super::Launcher;
use super::super::{act, eur};
use super::rf_panels::{check_row, seg, stepper, switch};
use super::rosterflow::{rf_button, caps, plain, Pal};
use glam::Vec2;
use omsi_launcher_lib::company::rosterflow::{self as rf, DutyType, Limit, Prefs, ReliefTiming, Travel, Tri};
use omsi_launcher_lib::company::Cents;
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};

/// The preferences, by kind: (kind, its icon, its items).
const KINDS: [(&str, &str, &[Item]); 4] = [
    ("Vehicles", "directions_bus", &[Item::Layover, Item::PrePost]),
    ("Drivers", "person", &[Item::Signing, Item::Relief, Item::Travel, Item::Breaks, Item::Continuous, Item::Limits, Item::Pieces, Item::Changeover, Item::Split, Item::Types]),
    ("Depots", "local_parking", &[Item::Depot]),
    ("Misc", "apps", &[Item::Costs, Item::Algorithm]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Item {
    Layover,
    PrePost,
    Signing,
    Relief,
    Travel,
    Breaks,
    Continuous,
    Limits,
    Pieces,
    Changeover,
    Split,
    Types,
    Depot,
    Costs,
    Algorithm,
}

impl Item {
    fn name(self) -> &'static str {
        match self {
            Item::Layover => "Layover",
            Item::PrePost => "Pre/Post-Trip",
            Item::Signing => "Driver Signing",
            Item::Relief => "Relief Timing",
            Item::Travel => "Duty Travel",
            Item::Breaks => "Duty Breaks",
            Item::Continuous => "Continuous Break",
            Item::Limits => "Time Limitations",
            Item::Pieces => "Piece Length",
            Item::Changeover => "Driver Changeover Vehicle",
            Item::Split => "Split Break Definition",
            Item::Types => "Duty Types",
            Item::Depot => "Depot Setup",
            Item::Costs => "Costs",
            Item::Algorithm => "Algorithm Parameters",
        }
    }

    fn info(self) -> &'static str {
        match self {
            Item::Layover => "The least time a bus stands between two trips at one place.",
            Item::PrePost => "The bus's check before it leaves the depot and after it is back.",
            Item::Signing => "The paid minutes a driver signs on before work and off after it, at the duty's ends and around a split.",
            Item::Relief => "When the driver changes within the bus's layover at a relief point (a terminus or the depot).",
            Item::Travel => "How a driver gets to a relief point at another place, and how long it takes.",
            Item::Breaks => "A duty working longer than this needs a meal break of at least this long.",
            Item::Continuous => "The longest a driver works without a break of at least this long.",
            Item::Limits => "The spread of a duty (sign-on to sign-off) at most.",
            Item::Pieces => "Pieces of work at least this long: short pieces are costly.",
            Item::Changeover => "How many times a driver may change buses in a duty.",
            Item::Split => "A break between two pieces this long or longer is a split: unpaid.",
            Item::Types => "The kinds of duty the company works. A duty no allowed type fits is Other, an issue. The first that fits is its type.",
            Item::Depot => "How many buses the depot holds: buses out at once at most.",
            Item::Costs => "What the crew and the vehicles cost: Optimise works the cost down.",
            Item::Algorithm => "What Optimise makes: the vehicles' blocks, the duties, or both.",
        }
    }

    /// In use: a rule that is on, or a setting.
    fn in_use(self, p: &Prefs) -> bool {
        match self {
            Item::Layover => p.layover.on,
            Item::Breaks => p.meal.on,
            Item::Continuous => p.continuous.on,
            Item::Limits => p.max_spread.on,
            Item::Pieces => p.min_piece.on,
            Item::Changeover => p.changeovers.on,
            Item::Depot => p.depot_capacity.on,
            Item::Types => !p.duty_types.is_empty(),
            Item::Costs => p.wage.is_some() || p.driver_fixed.is_some() || p.vehicle_fixed.is_some() || p.per_km.is_some(),
            _ => true,
        }
    }
}

/// The window open: the preferences being changed (kept only on save), the item shown, the
/// search, and IN USE or ALL.
pub(in super::super) struct PrefsEdit {
    pub prefs: Prefs,
    pub item: Item,
    pub search: String,
    pub in_use: bool,
    pub scroll: f32,
}

impl PrefsEdit {
    pub(super) fn new(p: &Prefs) -> PrefsEdit {
        PrefsEdit { prefs: p.clone(), item: Item::Signing, search: String::new(), in_use: false, scroll: 0.0 }
    }
}

/// Minutes as "h:mm".
fn hm(m: i32) -> String {
    rf::hm(m)
}

/// A rule's editor: in use, its value, hard or soft, the penalty.
#[allow(clippy::too_many_arguments)]
fn limit(ui: &mut Ui, p: &Pal, name: &str, x: f32, y: &mut f32, w: f32, lim: &mut Limit, value_label: &str, step: i32, max: i32, fmt: &dyn Fn(i32) -> String) {
    if switch(ui, p, &format!("{name}-on"), Rect::new(x, *y, 260.0, 30.0), lim.on, "In use") {
        lim.on = !lim.on;
    }
    *y += 42.0;
    if !lim.on {
        return;
    }
    ui.text_in(value_label, Rect::new(x, *y, 260.0, 30.0), 13.0, Weight::Medium, p.dim, Align::Left);
    if let Some(v) = stepper(ui, p, &format!("{name}-value"), Rect::new(x + 270.0, *y, 150.0, 30.0), lim.value, step, 0, max, fmt) {
        lim.value = v;
    }
    *y += 42.0;
    ui.text_in("Rule", Rect::new(x, *y, 260.0, 30.0), 13.0, Weight::Medium, p.dim, Align::Left);
    if let Some(k) = seg(ui, p, &format!("{name}-hard"), Rect::new(x + 270.0, *y, 300.0, 30.0), if lim.hard { 0 } else { 1 }, &["Hard rule", "Soft rule"]) {
        lim.hard = k == 0;
    }
    *y += 42.0;
    if !lim.hard {
        ui.text_in("Penalty", Rect::new(x, *y, 260.0, 30.0), 13.0, Weight::Medium, p.dim, Align::Left);
        if let Some(v) = stepper(ui, p, &format!("{name}-pen"), Rect::new(x + 270.0, *y, 150.0, 30.0), (lim.penalty / 100) as i32, 5, 0, 10_000, &|e| eur(e as Cents * 100)) {
            lim.penalty = v as Cents * 100;
        }
        ui.text_in("Each time it is broken; Optimise weighs it against the cost.", Rect::new(x + 430.0, *y, w - 430.0, 30.0), 11.5, Weight::Regular, p.faint, Align::Left);
        *y += 42.0;
    } else {
        ui.text_in("Never broken by Optimise; an error when broken by hand.", Rect::new(x, *y - 6.0, w, 22.0), 11.5, Weight::Regular, p.faint, Align::Left);
        *y += 26.0;
    }
    let _ = w;
}

/// A setting in minutes: its words and its stepper.
#[allow(clippy::too_many_arguments)]
fn minutes(ui: &mut Ui, p: &Pal, name: &str, x: f32, y: &mut f32, label: &str, v: &mut i32, step: i32, max: i32) {
    ui.text_in(label, Rect::new(x, *y, 260.0, 30.0), 13.0, Weight::Medium, p.dim, Align::Left);
    if let Some(n) = stepper(ui, p, name, Rect::new(x + 270.0, *y, 150.0, 30.0), *v, step, 0, max, &hm) {
        *v = n;
    }
    *y += 42.0;
}

/// A cost: as the company pays (None), or set.
#[allow(clippy::too_many_arguments)]
fn cost(ui: &mut Ui, p: &Pal, name: &str, x: f32, y: &mut f32, label: &str, v: &mut Option<Cents>, auto: Cents, step: i32, unit: &str) {
    ui.text_in(label, Rect::new(x, *y, 260.0, 30.0), 13.0, Weight::Medium, p.dim, Align::Left);
    let mut set = v.is_some();
    if check_row(ui, p, &format!("{name}-set"), Rect::new(x + 270.0, *y, 110.0, 30.0), set, "Set") {
        set = !set;
        *v = if set { Some(auto) } else { None };
    }
    let shown = v.unwrap_or(auto);
    if set {
        if let Some(n) = stepper(ui, p, name, Rect::new(x + 384.0, *y, 170.0, 30.0), shown as i32, step, 0, 10_000_000, &|c| format!("{}{unit}", eur(c as Cents))) {
            *v = Some(n as Cents);
        }
    } else {
        let words = format!("{}{unit}  ({})", eur(shown), omsi_ui::tr("as the company pays"));
        ui.text_as_is(&words, Vec2::new(x + 384.0, *y + 19.5), 13.0, Weight::Medium, p.text, Align::Left);
    }
    *y += 42.0;
}

/// The window over the studio (`r`).
pub(super) fn window(l: &mut Launcher, r: Rect, p: &Pal) {
    let Some(mut e) = l.company.planning.rf.prefs.take() else { return };
    let company = l.company.company.clone();
    let ui = &mut l.ui;
    // (it takes the mouse and the wheel: nothing under it)
    ui.p().rect(r, p.bg);
    l.company.planning.rf.over.push(r);
    let top = Rect::new(r.x, r.y, r.w, 46.0);
    ui.p().rect(top, p.bar);
    ui.icon("tune", Vec2::new(r.x + 24.0, top.center().y), 22.0, p.accent);
    ui.text_in("Preferences", Rect::new(r.x + 44.0, top.y, 300.0, top.h), 17.0, Weight::Bold, p.text, Align::Left);
    let defaults = rf_button(ui, p, "rf-prefs-defaults", Rect::new(r.right() - 190.0, top.y + 8.0, 176.0, 30.0), "Load defaults", false, true);
    // the sidebar: search, IN USE / ALL, the kinds and their items
    let side = Rect::new(r.x + 18.0, top.bottom() + 16.0, 320.0, r.h - top.h - 90.0);
    plain(ui, |ui| ui.text_input("rf-prefs-search", Rect::new(side.x, side.y, side.w, 36.0), &mut e.search, "Search", Some("search")));
    if let Some(k) = seg(ui, p, "rf-prefs-inuse", Rect::new(side.x, side.y + 46.0, side.w, 28.0), if e.in_use { 0 } else { 1 }, &["In use", "All"]) {
        e.in_use = k == 0;
    }
    let mut y = side.y + 90.0;
    let q = e.search.trim().to_lowercase();
    ui.push_clip(Rect::new(side.x - 4.0, y, side.w + 8.0, side.bottom() - y), 0.0);
    for (kind, icon, items) in KINDS.iter() {
        let shown: Vec<Item> = items.iter().copied().filter(|it| (!e.in_use || it.in_use(&e.prefs)) && (q.is_empty() || omsi_ui::tr(it.name()).to_lowercase().contains(&q) || it.name().to_lowercase().contains(&q))).collect();
        if shown.is_empty() && !q.is_empty() {
            continue;
        }
        ui.icon(icon, Vec2::new(side.x + 12.0, y + 14.0), 20.0, p.dim);
        let t = omsi_ui::tr(kind).to_uppercase();
        ui.text_as_is(&t, Vec2::new(side.x + 32.0, y + 20.5), 16.0, Weight::Bold, p.accent, Align::Left);
        y += 30.0;
        for it in shown {
            let rr = Rect::new(side.x + 24.0, y, side.w - 24.0, 24.0);
            let on = e.item == it;
            let (h, _, click) = ui.interact(id_of(&format!("rf-prefs-item-{it:?}")), rr);
            if on {
                ui.p().rect(rr, p.tool);
            } else if h {
                ui.p().rect(rr, p.text.alpha(0.05));
            }
            ui.text_in(it.name(), Rect::new(rr.x + 10.0, rr.y, rr.w - 20.0, rr.h), 13.5, if it.in_use(&e.prefs) { Weight::Bold } else { Weight::Regular }, if on || h { p.text } else { p.text.alpha(0.82) }, Align::Left);
            if click {
                e.item = it;
                e.scroll = 0.0;
            }
            y += 26.0;
        }
        y += 6.0;
    }
    ui.pop_clip();
    // the item: its title, what it is, its settings
    let pane = Rect::new(side.right() + 36.0, side.y, r.right() - side.right() - 60.0, side.h);
    let t = omsi_ui::tr(e.item.name()).to_uppercase();
    ui.text_as_is(&t, Vec2::new(pane.x, pane.y + 18.0), 16.0, Weight::Bold, p.text, Align::Left);
    let ih = ui.paragraph(e.item.info(), Vec2::new(pane.x, pane.y + 30.0), pane.w.min(760.0), 13.0, Weight::Regular, p.dim);
    let body = Rect::new(pane.x, pane.y + 44.0 + ih, pane.w, pane.h - 44.0 - ih);
    ui.push_clip(body, 0.0);
    if body.contains(ui.input.mouse) && ui.input.wheel.y.abs() > 0.0 {
        e.scroll = (e.scroll - ui.input.wheel.y * 40.0).max(0.0);
        ui.input.wheel = Vec2::ZERO;
    }
    let mut y = body.y + 6.0 - e.scroll;
    let x = body.x;
    let w = body.w;
    let pr = &mut e.prefs;
    let (hour, fixed, day, per_km) = company.as_ref().map(rf::rates).unwrap_or((2500.0, 7500, 3000, 50.0));
    match e.item {
        Item::Layover => limit(ui, p, "rf-p-layover", x, &mut y, w, &mut pr.layover, "Least layover", 1, 120, &hm),
        Item::PrePost => {
            minutes(ui, p, "rf-p-pre", x, &mut y, "Pre-trip check", &mut pr.pre_trip, 1, 60);
            minutes(ui, p, "rf-p-post", x, &mut y, "Post-trip check", &mut pr.post_trip, 1, 60);
        }
        Item::Signing => {
            minutes(ui, p, "rf-p-son", x, &mut y, "Sign-on at the duty's start", &mut pr.sign_on, 1, 60);
            minutes(ui, p, "rf-p-soff", x, &mut y, "Sign-off at the duty's end", &mut pr.sign_off, 1, 60);
            minutes(ui, p, "rf-p-son-split", x, &mut y, "Sign-on after a split", &mut pr.sign_on_split, 1, 60);
            minutes(ui, p, "rf-p-soff-split", x, &mut y, "Sign-off before a split", &mut pr.sign_off_split, 1, 60);
        }
        Item::Relief => {
            for (k, t) in ReliefTiming::ALL.iter().enumerate() {
                if check_row(ui, p, &format!("rf-p-relief-{k}"), Rect::new(x, y, 460.0, 28.0), pr.relief == *t, ReliefTiming::LABELS[k]) {
                    pr.relief = *t;
                }
                y += 32.0;
            }
            y += 10.0;
            if pr.relief == ReliefTiming::BeforeDeparture {
                minutes(ui, p, "rf-p-before", x, &mut y, "Minutes before the departure", &mut pr.relief_before, 1, 60);
            }
            ui.paragraph("Relief points are the line's termini and the depot.", Vec2::new(x, y), w.min(700.0), 12.0, Weight::Regular, p.faint);
        }
        Item::Travel => {
            ui.text_in("Way by default", Rect::new(x, y, 260.0, 30.0), 13.0, Weight::Medium, p.dim, Align::Left);
            let at = Travel::ALL.iter().position(|m| *m == pr.travel).unwrap_or(0);
            if let Some(k) = seg(ui, p, "rf-p-travel", Rect::new(x + 270.0, y, 420.0, 30.0), at, &Travel::LABELS) {
                pr.travel = Travel::ALL[k];
            }
            y += 46.0;
            minutes(ui, p, "rf-p-walk", x, &mut y, "Walk (W)", &mut pr.walk, 1, 120);
            minutes(ui, p, "rf-p-public", x, &mut y, "Public transport (P)", &mut pr.public, 1, 120);
            minutes(ui, p, "rf-p-car", x, &mut y, "Staff car (R)", &mut pr.car, 1, 120);
        }
        Item::Breaks => {
            minutes(ui, p, "rf-p-meal-after", x, &mut y, "A duty working longer than", &mut pr.meal_after, 15, 900);
            limit(ui, p, "rf-p-meal", x, &mut y, w, &mut pr.meal, "needs a meal break of", 5, 180, &hm);
        }
        Item::Continuous => {
            limit(ui, p, "rf-p-cont", x, &mut y, w, &mut pr.continuous, "Longest work without a break", 15, 720, &hm);
            minutes(ui, p, "rf-p-break-min", x, &mut y, "A break is at least", &mut pr.break_min, 5, 120);
            if switch(ui, p, "rf-p-layover-breaks", Rect::new(x, y, 420.0, 30.0), pr.layover_breaks, "A layover with the bus counts as a break") {
                pr.layover_breaks = !pr.layover_breaks;
            }
        }
        Item::Limits => limit(ui, p, "rf-p-spread", x, &mut y, w, &mut pr.max_spread, "Spread at most", 15, 1440, &hm),
        Item::Pieces => limit(ui, p, "rf-p-piece", x, &mut y, w, &mut pr.min_piece, "Pieces at least", 5, 600, &hm),
        Item::Changeover => limit(ui, p, "rf-p-changes", x, &mut y, w, &mut pr.changeovers, "Changes of bus at most", 1, 10, &|n| n.to_string()),
        Item::Split => minutes(ui, p, "rf-p-split", x, &mut y, "A split is a break of at least", &mut pr.split_min, 5, 600),
        Item::Types => duty_types(ui, p, x, &mut y, w, &mut pr.duty_types),
        Item::Depot => limit(ui, p, "rf-p-depot", x, &mut y, w, &mut pr.depot_capacity, "Buses out at once at most", 1, 500, &|n| n.to_string()),
        Item::Costs => {
            cost(ui, p, "rf-p-wage", x, &mut y, "A driver's hour", &mut pr.wage, hour.round() as Cents, 50, "");
            cost(ui, p, "rf-p-fixed", x, &mut y, "Each duty besides its hours", &mut pr.driver_fixed, fixed, 100, "");
            cost(ui, p, "rf-p-bus", x, &mut y, "Each bus out a day", &mut pr.vehicle_fixed, day, 100, "");
            cost(ui, p, "rf-p-km", x, &mut y, "Each kilometre", &mut pr.per_km, per_km.round() as Cents, 1, "");
        }
        Item::Algorithm => {
            if switch(ui, p, "rf-p-opt-veh", Rect::new(x, y, 420.0, 30.0), pr.optimise_vehicles, "Optimise vehicles (the blocks)") {
                pr.optimise_vehicles = !pr.optimise_vehicles;
            }
            y += 42.0;
            if switch(ui, p, "rf-p-opt-duty", Rect::new(x, y, 420.0, 30.0), pr.optimise_duties, "Optimise duties (cut and paired)") {
                pr.optimise_duties = !pr.optimise_duties;
            }
        }
    }
    ui.pop_clip();
    // the foot
    let foot = Rect::new(r.x, r.bottom() - 58.0, r.w, 58.0);
    ui.p().rect(Rect::new(foot.x, foot.y, foot.w, 1.0), p.line);
    let save = {
        let br = Rect::new(r.x + 18.0, foot.y + 13.0, 170.0, 32.0);
        let (h, _, click) = ui.interact(id_of("rf-prefs-save"), br);
        ui.p().rounded_border(br, 2.0, 1.4, if h { p.text } else { p.accent });
        let t = omsi_ui::tr("Save changes").to_uppercase();
        ui.text_as_is(&t, Vec2::new(br.center().x, br.center().y + 4.5), 12.5, Weight::Bold, p.accent, Align::Center);
        click
    };
    let cancel = rf_button(ui, p, "rf-prefs-cancel", Rect::new(r.right() - 330.0, foot.y + 13.0, 150.0, 32.0), "Cancel", false, true);
    let close = rf_button(ui, p, "rf-prefs-close", Rect::new(r.right() - 170.0, foot.y + 13.0, 152.0, 32.0), "Save & close", true, true);
    if defaults {
        e.prefs = Prefs::default();
    }
    if save || close {
        let prefs = e.prefs.clone();
        if act(l, |c| {
            c.planning.flow.prefs = prefs;
            Ok(())
        })
        .is_some()
        {
            l.state.set_status(omsi_ui::tr("Preferences saved: the issues and Optimise follow them.").into_owned(), false);
        }
    }
    if !(cancel || close) {
        l.company.planning.rf.prefs = Some(e);
    }
    let _ = Color::WHITE;
}

/// The duty types, each an instance: its name, its windows, split or not, allowed, penalty;
/// added and deleted.
fn duty_types(ui: &mut Ui, p: &Pal, x: f32, y: &mut f32, w: f32, types: &mut Vec<DutyType>) {
    let mut remove = None;
    let mut up = None;
    for (n, t) in types.iter_mut().enumerate() {
        let card = Rect::new(x, *y, w.min(900.0), 196.0);
        ui.p().rounded(card, 4.0, p.sheet);
        ui.text_as_is(&format!("{}.", n + 1), Vec2::new(card.x + 14.0, card.y + 26.0), 14.0, Weight::Bold, p.dim, Align::Left);
        let mut name = omsi_ui::tr(&t.name).into_owned();
        if plain(ui, |ui| ui.text_input(&format!("rf-type-name-{n}"), Rect::new(card.x + 40.0, card.y + 8.0, 220.0, 32.0), &mut name, "Name", None)) {
            t.name = name;
        }
        if switch(ui, p, &format!("rf-type-allowed-{n}"), Rect::new(card.x + 280.0, card.y + 9.0, 140.0, 30.0), t.allowed, "Allowed") {
            t.allowed = !t.allowed;
        }
        let ur = Rect::new(card.right() - 70.0, card.y + 10.0, 28.0, 28.0);
        let (uh, _, uclick) = ui.interact(id_of(&format!("rf-type-up-{n}")), ur);
        ui.icon("arrow_upward", ur.center(), 17.0, if uh { p.text } else { p.dim });
        ui.tooltip(ur, "Earlier in the order: the first type a duty fits is its type");
        let dr = Rect::new(card.right() - 38.0, card.y + 10.0, 28.0, 28.0);
        let (dh, _, dclick) = ui.interact(id_of(&format!("rf-type-del-{n}")), dr);
        ui.icon("delete", dr.center(), 17.0, if dh { p.err } else { p.dim });
        if uclick && n > 0 {
            up = Some(n);
        }
        if dclick {
            remove = Some(n);
        }
        let row = |k: usize| card.y + 50.0 + k as f32 * 36.0;
        let col2 = card.x + card.w * 0.5;
        caps(ui, Rect::new(card.x + 16.0, row(0), 120.0, 30.0), "Start", p.dim, Align::Left);
        if let Some(v) = stepper(ui, p, &format!("rf-type-sf-{n}"), Rect::new(card.x + 120.0, row(0), 110.0, 28.0), t.start_from, 15, 0, 1439, &rf::clock) {
            t.start_from = v;
        }
        if let Some(v) = stepper(ui, p, &format!("rf-type-st-{n}"), Rect::new(card.x + 240.0, row(0), 110.0, 28.0), t.start_to, 15, 0, 1439, &rf::clock) {
            t.start_to = v;
        }
        caps(ui, Rect::new(col2, row(0), 120.0, 30.0), "End by", p.dim, Align::Left);
        if let Some(v) = stepper(ui, p, &format!("rf-type-end-{n}"), Rect::new(col2 + 110.0, row(0), 110.0, 28.0), t.end_by, 15, 0, 2879, &rf::clock) {
            t.end_by = v;
        }
        caps(ui, Rect::new(card.x + 16.0, row(1), 120.0, 30.0), "Length at most", p.dim, Align::Left);
        if let Some(v) = stepper(ui, p, &format!("rf-type-len-{n}"), Rect::new(card.x + 120.0, row(1), 110.0, 28.0), t.max_length, 15, 60, 1440, &rf::hm) {
            t.max_length = v;
        }
        caps(ui, Rect::new(col2, row(1), 120.0, 30.0), "Split", p.dim, Align::Left);
        let at = Tri::ALL.iter().position(|s| *s == t.split).unwrap_or(2);
        if let Some(k) = seg(ui, p, &format!("rf-type-split-{n}"), Rect::new(col2 + 110.0, row(1), 220.0, 28.0), at, &Tri::LABELS) {
            t.split = Tri::ALL[k];
        }
        caps(ui, Rect::new(card.x + 16.0, row(2), 120.0, 30.0), "Penalty", p.dim, Align::Left);
        if let Some(v) = stepper(ui, p, &format!("rf-type-pen-{n}"), Rect::new(card.x + 120.0, row(2), 130.0, 28.0), (t.penalty / 100) as i32, 5, 0, 10_000, &|e| eur(e as Cents * 100)) {
            t.penalty = v as Cents * 100;
        }
        *y += 206.0;
    }
    if let Some(n) = remove {
        types.remove(n);
    }
    if let Some(n) = up {
        types.swap(n - 1, n);
    }
    let add = Rect::new(x, *y, 180.0, 32.0);
    let (h, _, click) = ui.interact(id_of("rf-type-add"), add);
    ui.p().rounded_border(add, 2.0, 1.0, if h { p.text } else { p.dim });
    ui.icon("add", Vec2::new(add.x + 16.0, add.center().y), 16.0, p.text);
    ui.text_in("Add preference", Rect::new(add.x + 30.0, add.y, add.w - 34.0, add.h), 13.0, Weight::Medium, p.text, Align::Left);
    if click {
        types.push(DutyType { name: omsi_ui::tr("New type").into_owned(), ..Default::default() });
    }
    *y += 44.0;
}
