//! The workshop's stores (the rules are `omsi_launcher_lib::company::parts`'s), as City Bus
//! Manager's spare parts shop: a card a part - its price (the research's discount shown against
//! the list price), its stock and a slider for how many to order - and under them the
//! delivery (free at 16:00, or at once for a fee), the sum and Buy. Beside them: the buses
//! waiting for parts, what is on order, the stores' own ordering, and the mechanics.

use super::super::theme::*;
use super::super::ui::{ButtonKind, Ui};
use super::super::Launcher;
use super::{act, day_label, eur, figure, kit, num, section};
use glam::Vec2;
use omsi_launcher_lib::company::clock;
use omsi_launcher_lib::company::parts::{self as pt, Part};
use omsi_launcher_lib::company::{dates, research, Company};
use omsi_ui::paint::Align;
use omsi_ui::{Color, Rect, Weight};
use std::cell::RefCell;

/// What is being ordered: how many of each part (`Part::ALL`'s order), and how it comes.
#[derive(Clone, Default)]
struct Basket {
    company: String,
    counts: [f32; 10],
    /// 1: at once, for the fee.
    express: usize,
}

thread_local! {
    static BASKET: RefCell<Basket> = RefCell::new(Basket::default());
}

/// How many of a part one order takes at most.
const MOST: f32 = 30.0;
const CARD_W: f32 = 168.0;
const CARD_H: f32 = 198.0;
const GAP: f32 = 14.0;

pub fn draw(l: &mut Launcher, area: Rect) {
    let Some(c) = l.company.company.clone() else { return };
    let mut basket = BASKET.with(|b| b.borrow().clone());
    if basket.company != c.id {
        basket = Basket { company: c.id.clone(), ..Default::default() };
    }
    let wide = area.w >= 980.0;
    let side_w = if wide { (area.w * 0.3).clamp(300.0, 420.0) } else { 0.0 };
    let main = Rect::new(area.x, area.y, area.w - if wide { side_w + 16.0 } else { 0.0 }, area.h);
    // the figures
    let fh = kit::FIGURE_H;
    let cols = if main.w >= 760.0 { 4 } else { 2 };
    let fw = (main.w - (cols as f32 - 1.0) * 12.0) / cols as f32;
    let today = c.date.clone();
    let mechanics = pt::mechanics(&c, &today).count();
    let waiting = c.stores.waiting.len();
    let on_order: u32 = c.stores.orders.iter().map(pt::Order::count).sum();
    let worth: i64 = Part::ALL.iter().map(|p| pt::price(&c, *p) * c.stores.count(*p) as i64).sum();
    let figures = [
        ("Mechanics", mechanics.to_string(), omsi_ui::tr("at work today").into_owned(), if mechanics == 0 { WARN } else { TEXT }),
        ("Waiting for parts", waiting.to_string(), omsi_ui::tr("buses in the workshop").into_owned(), if waiting > 0 { WARN } else { TEXT }),
        ("On order", on_order.to_string(), omsi_ui::tr("parts on their way").into_owned(), TEXT),
        ("In store", eur(worth), omsi_ui::tr("worth of spare parts").into_owned(), TEXT),
    ];
    for (k, (label, value, under, ink)) in figures.iter().enumerate() {
        let r = Rect::new(main.x + (k % cols) as f32 * (fw + 12.0), main.y + (k / cols) as f32 * (fh + 12.0), fw, fh);
        figure(&mut l.ui, r, label, value, under, *ink);
    }
    let fy = main.y + figures.len().div_ceil(cols) as f32 * (fh + 12.0) + 4.0;
    let body = Rect::new(main.x, fy, main.w, (main.bottom() - fy).max(0.0));
    // the parts, the delivery, the sum and Buy (and, on a narrow page, the rest under them)
    let mut buy = false;
    let mut auto = None;
    l.ui.scroll_area("company-workshop", body, &mut |ui, v| {
        let w = v.w - 12.0;
        let cols = (((w + GAP) / (CARD_W + GAP)).floor() as usize).clamp(1, 5);
        let cw = (w - (cols as f32 - 1.0) * GAP) / cols as f32;
        for (k, p) in Part::ALL.iter().enumerate() {
            let r = Rect::new(v.x + (k % cols) as f32 * (cw + GAP), v.y + (k / cols) as f32 * (CARD_H + GAP), cw, CARD_H);
            if ui.rect_visible(r) {
                part_card(ui, r, &c, *p, &mut basket.counts[k]);
            }
        }
        let mut y = v.y + Part::ALL.len().div_ceil(cols) as f32 * (CARD_H + GAP) + 6.0;
        y = footer(ui, Rect::new(v.x, y, w, 0.0), &c, &mut basket, &mut buy) + 18.0;
        if !wide {
            y = side(ui, Rect::new(v.x, y, w, 0.0), &c, &mut auto) + 10.0;
        }
        y - v.y
    });
    if wide {
        let r = Rect::new(main.right() + 16.0, area.y, side_w, area.h);
        l.ui.scroll_area("company-workshop-side", r, &mut |ui, v| side(ui, Rect::new(v.x, v.y, v.w - 10.0, 0.0), &c, &mut auto) - v.y);
    }
    if let Some(on) = auto {
        act(l, |c| {
            c.stores.auto = on;
            Ok(())
        });
    }
    if buy {
        order(l, &c, &mut basket);
    }
    BASKET.with(|b| *b.borrow_mut() = basket);
}

/// A part's card: its name and picture, its price, the slider of how many to order, its stock
/// and what the ones chosen cost.
fn part_card(ui: &mut Ui, r: Rect, c: &Company, p: Part, count: &mut f32) {
    ui.p().rounded(r, SHEET_RADIUS, FIELD);
    ui.p().rounded_border(r, SHEET_RADIUS, 1.0, if *count > 0.0 { accent().alpha(0.7) } else { Color::rgba(110, 160, 230, 0.45) });
    ui.text_in(p.label(), Rect::new(r.x + 10.0, r.y + 10.0, r.w - 20.0, 26.0), 17.0, Weight::Bold, TEXT, Align::Center);
    let ic = Vec2::new(r.center().x, r.y + 62.0);
    ui.p().circle(ic, 24.0, accent_2().alpha(0.16));
    ui.icon(p.icon(), ic, 28.0, accent_2());
    // the price: the list price struck through when the research takes some off
    let (list, price) = (pt::list_price(c, p), pt::price(c, p));
    let now = eur(price);
    let pw = ui.width(&now, 16.0, Weight::Bold);
    if price < list {
        let was = eur(list);
        let ww = ui.width(&was, 11.5, Weight::Regular);
        let x = r.center().x - (ww + 8.0 + pw) * 0.5;
        let y = r.y + 98.0;
        ui.text_in(&was, Rect::new(x, y, ww + 2.0, 22.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
        ui.p().line(Vec2::new(x - 1.0, y + 11.0), Vec2::new(x + ww + 1.0, y + 11.0), 1.0, TEXT_DIM);
        ui.text_in(&now, Rect::new(x + ww + 8.0, y, pw + 4.0, 22.0), 16.0, Weight::Bold, OK, Align::Left);
    } else {
        ui.text_in(&now, Rect::new(r.x + 10.0, r.y + 98.0, r.w - 20.0, 22.0), 16.0, Weight::Bold, TEXT, Align::Center);
    }
    ui.slider(&format!("company-workshop-{}", p.label()), Rect::new(r.x + 12.0, r.y + 126.0, r.w - 24.0, 30.0), count, 0.0, MOST, 1.0, "", &|x| format!("{x:.0}"));
    // the stock, and what the ones chosen cost
    let stock = c.stores.count(p);
    let y = r.y + 164.0;
    ui.icon("inventory_2", Vec2::new(r.x + 24.0, y + 11.0), 16.0, if stock == 0 { DANGER.lighten(0.2) } else { TEXT_SOFT });
    ui.text_in(&stock.to_string(), Rect::new(r.x + 38.0, y, 60.0, 22.0), kit::ROWS + 1.0, Weight::Bold, if stock == 0 { DANGER.lighten(0.2) } else { TEXT }, Align::Left);
    ui.tooltip(Rect::new(r.x + 12.0, y, 80.0, 22.0), "In store");
    let line = price * count.round() as i64;
    ui.text_in(&eur(line), Rect::new(r.x + r.w * 0.4, y, r.w * 0.6 - 14.0, 22.0), kit::ROWS + 1.0, Weight::Bold, if line > 0 { TEXT } else { TEXT_DIM }, Align::Right);
}

/// When an order's 16:00 is, in words.
fn when(c: &Company, due: i64) -> String {
    let day = clock::date_of(due);
    match dates::between(&c.date, &day) {
        0 => omsi_ui::tr("today at 16:00").into_owned(),
        1 => omsi_ui::tr("tomorrow at 16:00").into_owned(),
        _ => format!("{} {}", day_label(&day), clock::hhmm(due)),
    }
}

/// The delivery, the sum and Buy. Returns its bottom.
fn footer(ui: &mut Ui, r: Rect, c: &Company, basket: &mut Basket, buy: &mut bool) -> f32 {
    let list: Vec<(Part, u32)> = Part::ALL.iter().zip(basket.counts.iter()).map(|(p, n)| (*p, n.round() as u32)).collect();
    let express = basket.express == 1;
    let sum = pt::order_cost(c, &list, express);
    let n: u32 = list.iter().map(|x| x.1).sum();
    let w = r.w.min(560.0);
    let x = r.x + (r.w - w) * 0.5;
    let free = omsi_ui::tr("Delivery at 16:00: free").into_owned();
    let now = omsi_ui::tr("At once: %{amount}").replace("%{amount}", &eur(pt::express_fee(c)));
    ui.segmented("company-workshop-delivery", Rect::new(x, r.y, w, 42.0), &mut basket.express, &[free.as_str(), now.as_str()]);
    let note = if basket.express == 1 { omsi_ui::tr("In the stores as soon as it is bought.").into_owned() } else { omsi_ui::tr("Ordered now, it comes %{when}.").replace("%{when}", &when(c, pt::free_delivery(c))) };
    ui.text_in(&note, Rect::new(x, r.y + 48.0, w, 20.0), kit::NOTE, Weight::Regular, TEXT_SOFT, Align::Center);
    let total = omsi_ui::tr("Sum: %{amount}").replace("%{amount}", &eur(if n == 0 { 0 } else { sum }));
    ui.text_in(&total, Rect::new(x, r.y + 76.0, w, 34.0), 24.0, Weight::Bold, if sum > c.cash && n > 0 { DANGER.lighten(0.2) } else { TEXT }, Align::Center);
    let b = Rect::new(r.center().x - 110.0, r.y + 120.0, 220.0, 46.0);
    if ui.button("company-workshop-buy", b, "Buy", Some("inventory_2"), if n > 0 { ButtonKind::Primary } else { ButtonKind::Normal }) {
        *buy = true;
    }
    if n > 0 {
        let clear = Rect::new(b.right() + 12.0, b.y + 3.0, 40.0, 40.0);
        if ui.icon_button("company-workshop-clear", clear.center(), 18.0, "close", "Choose nothing") {
            basket.counts = [0.0; 10];
        }
    }
    b.bottom()
}

/// The rest of the stores: the buses waiting for parts, what is on order, the stores' own
/// ordering, the mechanics. Returns its bottom.
fn side(ui: &mut Ui, r: Rect, c: &Company, auto: &mut Option<bool>) -> f32 {
    let mut y = r.y;
    // the buses waiting for parts
    let rows = c.stores.waiting.len().max(1);
    let h = 58.0 + rows as f32 * 44.0;
    let inner = section(ui, Rect::new(r.x, y, r.w, h), "Waiting for parts");
    if c.stores.waiting.is_empty() {
        ui.text_in("No bus waits for a part.", Rect::new(inner.x, inner.y, inner.w, 24.0), kit::ROWS, Weight::Regular, TEXT_SOFT, Align::Left);
    }
    for (k, w) in c.stores.waiting.iter().enumerate() {
        let ry = inner.y + k as f32 * 44.0;
        ui.icon("directions_bus", Vec2::new(inner.x + 10.0, ry + 11.0), 16.0, TEXT_SOFT);
        let pw = 26.0 + kit::tag(ui, Vec2::new(inner.x + 24.0, ry), &w.number, TEXT_SOFT) + 10.0;
        let parts: Vec<String> = w.parts.iter().map(|p| omsi_ui::tr(p.label()).into_owned()).collect();
        ui.text_in(&parts.join(", "), Rect::new(inner.x + pw, ry, inner.w - pw, 22.0), kit::ROWS, Weight::Bold, WARN, Align::Left);
        let since = omsi_ui::tr("waiting since %{date}").replace("%{date}", &day_label(&w.since));
        ui.text_in(&since, Rect::new(inner.x + pw, ry + 20.0, inner.w - pw, 18.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
    }
    y += h + 12.0;
    // on order
    let rows = c.stores.orders.len().max(1);
    let h = 58.0 + rows as f32 * 44.0;
    let inner = section(ui, Rect::new(r.x, y, r.w, h), "On order");
    if c.stores.orders.is_empty() {
        ui.text_in("Nothing is on order.", Rect::new(inner.x, inner.y, inner.w, 24.0), kit::ROWS, Weight::Regular, TEXT_SOFT, Align::Left);
    }
    for (k, o) in c.stores.orders.iter().enumerate() {
        let ry = inner.y + k as f32 * 44.0;
        ui.icon("schedule", Vec2::new(inner.x + 10.0, ry + 11.0), 16.0, accent_2());
        let what = omsi_ui::tr("%{n} parts, %{when}").replace("%{n}", &o.count().to_string()).replace("%{when}", &when(c, o.due));
        ui.text_in(&what, Rect::new(inner.x + 28.0, ry, inner.w - 28.0, 22.0), kit::ROWS, Weight::Bold, TEXT, Align::Left);
        let parts: Vec<String> = o.parts.iter().map(|(p, n)| format!("{n} × {}", omsi_ui::tr(p.label()))).collect();
        let by = if o.auto { omsi_ui::tr("ordered by the stores").into_owned() } else { eur(o.cost) };
        ui.text_in(&format!("{}  ·  {by}", parts.join(", ")), Rect::new(inner.x + 28.0, ry + 20.0, inner.w - 28.0, 18.0), kit::NOTE, Weight::Regular, TEXT_DIM, Align::Left);
    }
    y += h + 12.0;
    // the stores' own ordering
    let text = "What falls under half of what the stores keep - and what a waiting bus needs - is ordered for the free delivery of the next day.";
    let ph = ui.paragraph_height(text, r.w - 36.0, kit::NOTE, Weight::Regular);
    let h = 58.0 + 36.0 + ph + 6.0;
    let inner = section(ui, Rect::new(r.x, y, r.w, h), "Ordering by itself");
    let mut on = c.stores.auto;
    if ui.toggle("company-workshop-auto", Rect::new(inner.x, inner.y, inner.w, 30.0), &mut on, "The stores order what runs short") {
        *auto = Some(on);
    }
    ui.paragraph(text, Vec2::new(inner.x, inner.y + 36.0), inner.w, kit::NOTE, Weight::Regular, TEXT_SOFT);
    y += h + 12.0;
    // the mechanics
    let today = c.date.clone();
    let n = pt::mechanics(c, &today).count();
    let line = if n == 0 {
        omsi_ui::tr("No mechanics at work: an outside garage repairs the buses that break down, in its own time. Hire mechanics on the Staff page.").into_owned()
    } else {
        omsi_ui::tr("At work today: %{n}, %{hours} hours of repairs a day.").replace("%{n}", &n.to_string()).replace("%{hours}", &num(pt::workshop_hours(c, &today), 0))
    };
    let faster = 1.0 - research::repair_factor(c);
    let more = if faster > 0.0 { omsi_ui::tr("The research makes the repairs %{n} % quicker.").replace("%{n}", &num(faster * 100.0, 0)) } else { String::new() };
    let text = if more.is_empty() { line } else { format!("{line} {more}") };
    let ph = ui.paragraph_height(&text, r.w - 36.0, kit::NOTE, Weight::Regular);
    let h = 58.0 + ph + 4.0;
    let inner = section(ui, Rect::new(r.x, y, r.w, h), "Mechanics");
    ui.paragraph(&text, Vec2::new(inner.x, inner.y), inner.w, kit::NOTE, Weight::Regular, if n == 0 { WARN } else { TEXT_SOFT });
    y + h
}

/// Buy what was chosen: what is in the way said first, else ordered and the choice cleared.
fn order(l: &mut Launcher, c: &Company, basket: &mut Basket) {
    let list: Vec<(Part, u32)> = Part::ALL.iter().zip(basket.counts.iter()).map(|(p, n)| (*p, n.round() as u32)).filter(|x| x.1 > 0).collect();
    let express = basket.express == 1;
    let n: u32 = list.iter().map(|x| x.1).sum();
    if n > 0 {
        let cost = pt::order_cost(c, &list, express);
        if cost > c.cash {
            kit::show(l, kit::no_cash(c, cost));
            return;
        }
    }
    let Some(cost) = act(l, |c| pt::order(c, &list, express)) else { return };
    let text = if express {
        omsi_ui::tr("Bought: %{n} spare parts for %{amount}, in the stores now.").replace("%{n}", &n.to_string()).replace("%{amount}", &eur(cost))
    } else {
        let due = l.company.company.as_ref().and_then(|c| c.stores.orders.last()).map(|o| o.due).unwrap_or(0);
        omsi_ui::tr("Ordered: %{n} spare parts for %{amount}; they come %{when}.").replace("%{n}", &n.to_string()).replace("%{amount}", &eur(cost)).replace("%{when}", &when(c, due))
    };
    l.state.set_status(text, false);
    basket.counts = [0.0; 10];
}
