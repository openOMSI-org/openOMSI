//! The company's settings: its mark (a logo picture, or its monogram), how much of it the
//! player runs himself (the easy or the advanced mode, changed at any time: going advanced
//! shows every page with nothing lost), its date (Luc: a company date of one's own, later on
//! as well) and, in the advanced mode, how it buys its buses. Moving the date moves the
//! company's clock to that day's midnight and simulates nothing in between; it may not go back
//! before the company's last booking (`clock::move_to`).

use super::super::theme::*;
use super::super::ui::ButtonKind;
use super::super::Launcher;
use super::kit::{self, Foot};
use super::{act, day_label, Dialog};
use glam::Vec2;
use omsi_launcher_lib::company::{self as co, clock as ck, dealer::BuyingMode};
use omsi_ui::paint::Align;
use omsi_ui::{Rect, Weight};

pub(super) fn dialog(l: &mut Launcher) {
    let Some(Dialog::Settings { date }) = &l.company.dialog else { return };
    let date = date.clone();
    let Some(c) = l.company.company.clone() else {
        l.company.dialog = None;
        return;
    };
    let easy = co::autopilot::on(&c);
    let f = kit::frame(l, 760.0, if easy { 722.0 } else { 786.0 }, "settings", &omsi_ui::tr("The company's settings"));
    let inner = f.body;
    let mut y = inner.y;
    // its mark
    kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), "The company's mark");
    y += 26.0;
    super::company_mark(l, Rect::new(inner.x, y, 68.0, 68.0), &c);
    let logo_text = c.logo.as_deref().map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string()).unwrap_or_else(|| omsi_ui::tr("No logo picture: the short name is the mark.").into_owned());
    l.ui.text_in(&logo_text, Rect::new(inner.x + 84.0, y + 2.0, inner.w - 84.0, 22.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    let lw = Foot::width(&l.ui, "Choose a logo picture", Some("photo_camera"));
    if l.ui.button("company-settings-logo", Rect::new(inner.x + 84.0, y + 30.0, lw, 36.0), "Choose a logo picture", Some("photo_camera"), ButtonKind::Normal) {
        if let Some(p) = omsi_launcher_lib::pick_file("Choose a logo picture") {
            let p = p.to_string_lossy().to_string();
            act(l, |c| {
                c.logo = Some(p);
                Ok(())
            });
        }
    }
    if c.logo.is_some() {
        let rw = Foot::width(&l.ui, "Remove the picture", Some("close"));
        if l.ui.button("company-settings-logo-remove", Rect::new(inner.x + 84.0 + lw + 10.0, y + 30.0, rw, 36.0), "Remove the picture", Some("close"), ButtonKind::Normal) {
            act(l, |c| {
                c.logo = None;
                Ok(())
            });
        }
    }
    y += 68.0 + 26.0;
    // how much of it the player runs himself
    kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), "How you run the company");
    y += 26.0;
    let names: Vec<String> = co::Mode::ALL.iter().map(|m| omsi_ui::tr(m.label()).into_owned()).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut k = co::Mode::ALL.iter().position(|m| *m == c.mode).unwrap_or(1);
    if l.ui.segmented("company-settings-mode", Rect::new(inner.x, y, 320.0f32.min(inner.w), 40.0), &mut k, &refs) {
        set_mode(l, co::Mode::ALL[k]);
    }
    let ph = l.ui.paragraph(co::Mode::ALL[k].info(), Vec2::new(inner.x + 340.0, y + 2.0), (inner.w - 340.0).max(120.0), kit::NOTE, Weight::Regular, TEXT_SOFT);
    y += (ph + 2.0).max(40.0) + 28.0;
    // how it buys (the easy mode buys quickly)
    if !easy {
        kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), "Buying buses");
        y += 26.0;
        let modes: Vec<String> = BuyingMode::ALL.iter().map(|m| omsi_ui::tr(m.label()).into_owned()).collect();
        let refs: Vec<&str> = modes.iter().map(String::as_str).collect();
        let mut mode = BuyingMode::ALL.iter().position(|m| *m == c.dealer.mode).unwrap_or(1);
        if l.ui.segmented("company-settings-buying", Rect::new(inner.x, y, 320.0f32.min(inner.w), 40.0), &mut mode, &refs) {
            let m = BuyingMode::ALL[mode];
            act(l, |c| {
                c.dealer.mode = m;
                Ok(())
            });
        }
        let say = if mode == 0 { "A model, a number, the list price: the buses are yours at once." } else { "Haggle with the dealer, agree on extras and sign a contract; new buses are delivered." };
        l.ui.paragraph(say, Vec2::new(inner.x + 340.0, y + 2.0), (inner.w - 340.0).max(120.0), kit::NOTE, Weight::Regular, TEXT_SOFT);
        y += 64.0;
    }
    // its date
    kit::caps(&mut l.ui, Rect::new(inner.x, y, inner.w, 16.0), "The company's date");
    y += 26.0;
    let now = omsi_ui::tr("Today it is %{date}, %{time}.").replace("%{date}", &day_label(&c.date)).replace("%{time}", &ck::hhmm(ck::now(&c)));
    l.ui.text_in(&now, Rect::new(inner.x, y, inner.w, 24.0), kit::BODY, Weight::Medium, TEXT, Align::Left);
    y += 34.0;
    // (whether its time runs with the real clock, also while openOMSI is closed)
    y += super::clock::realtime_switch(l, Rect::new(inner.x, y, inner.w, 0.0));
    let mut d = date;
    l.ui.date_field("company-settings-date", Rect::new(inner.x, y, 240.0, 40.0), &mut d);
    let earliest = ck::earliest_date(&c);
    if let Some(e) = &earliest {
        let t = omsi_ui::tr("Not before %{date}: the company's last booking.").replace("%{date}", &day_label(e));
        l.ui.text_in(&t, Rect::new(inner.x + 256.0, y, inner.w - 256.0, 40.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Left);
    }
    y += 56.0;
    let warn = "Moving the date changes what the company finds there: the dealer's offers and the used market of that day, deliveries and contracts by their own dates, concessions and leases running to theirs. The days between are not simulated: they bring no money and no wear.";
    l.ui.icon("warning", Vec2::new(inner.x + 10.0, y + 11.0), 20.0, WARN);
    l.ui.paragraph(warn, Vec2::new(inner.x + 32.0, y), inner.w - 32.0, kit::BODY, Weight::Regular, TEXT_SOFT);
    let mut foot = Foot::new(&f);
    // (deleting it: asked again, it cannot be undone)
    if foot.left(l, "company-settings-delete", "Delete the company", Some("delete"), ButtonKind::Danger) {
        l.company.dialog = Some(super::Dialog::Confirm { what: super::Confirm::DeleteCompany(c.id.clone()) });
        return;
    }
    let moving = co::dates::parse(&d).is_some() && d != c.date;
    let label = omsi_ui::tr("Move to %{date}").replace("%{date}", &day_label(&d));
    let go = moving && foot.right(l, "company-settings-move", &label, Some("calendar_month"), ButtonKind::Primary);
    if foot.right(l, "company-settings-close", "Close", None, if moving { ButtonKind::Normal } else { ButtonKind::Primary }) || f.close {
        l.company.dialog = None;
        return;
    }
    if go {
        let to = d.clone();
        if act(l, |c| ck::move_to(c, &to)).is_some() {
            l.company.dialog = None;
            l.company.plan = None;
            l.state.set_status(omsi_ui::tr("The company is at %{date} now.").replace("%{date}", &day_label(&to)), false);
            return;
        }
    }
    l.company.dialog = Some(Dialog::Settings { date: d });
}

/// Run the open company in `mode` from now on (`co::autopilot::set_mode`): a page the mode does
/// not show goes back to the start screen, and the change is said.
pub(super) fn set_mode(l: &mut Launcher, mode: co::Mode) {
    if act(l, |c| {
        co::autopilot::set_mode(c, mode);
        Ok(())
    })
    .is_none()
    {
        return;
    }
    if l.company.tab != super::HOME_TAB && !super::pages_of(mode).contains(&l.company.tab) {
        l.company.tab = super::HOME_TAB;
    }
    let text = match mode {
        co::Mode::Easy => "The company runs in the easy mode now: it sees to its routine by itself.",
        co::Mode::Advanced => "The company runs in the advanced mode now: every page and every function is open.",
    };
    l.state.set_status(omsi_ui::tr(text).into_owned(), false);
}
