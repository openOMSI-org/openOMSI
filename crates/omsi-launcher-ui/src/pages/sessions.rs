//! The Sessions page: the running games.

use super::*;

// --- sessions ---------------------------------------------------------------------------------

fn ago(t: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(t);
    let s = now.saturating_sub(t);
    if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min", s / 60)
    } else {
        format!("{} h {} min", s / 3600, (s / 60) % 60)
    }
}

pub fn sessions(l: &mut Launcher, area: Rect) {
    let body = area;
    let list = l.state.instances.clone();
    if list.is_empty() {
        let r = Rect::new(body.x, body.y, body.w.min(720.0), 120.0);
        l.ui.card(r);
        l.ui.icon("sports_esports", Vec2::new(r.x + 40.0, r.center().y), 36.0, TEXT_FAINT);
        l.ui.paragraph("No game is running. Start a duty on the Drive page; to drive with friends, turn on hosting on the Multiplayer page and give them the code shown here.", Vec2::new(r.x + 76.0, r.y + 30.0), r.w - 100.0, 13.5, Weight::Regular, TEXT_DIM);
        return;
    }
    let names: std::collections::HashMap<String, String> = l.state.vehicles.iter().map(|v| (v.file.clone(), v.name.clone())).collect();
    let short_bus = |b: &str| names.get(b).cloned().unwrap_or_else(|| b.rsplit('/').next().unwrap_or("").trim_end_matches(".bus").to_string());
    let mut y = body.y - l.ui.scroll.get(&id_of("sessions")).copied().unwrap_or(0.0);
    // (the page's whole width, as the other pages: at 900 a wide window had the cards in its
    // left half and the buttons in the middle of nowhere)
    let view = Rect::new(body.x, body.y, body.w, body.h);
    let mut actions: Vec<(u32, &str)> = Vec::new();
    let mut copy: Option<String> = None;
    l.ui.push_clip(view, 0.0);
    for i in &list {
        let lan = i.lan_status.clone().unwrap_or(Value::Null);
        let role = lan.get("role").and_then(|x| x.as_str()).unwrap_or("");
        let players: Vec<Value> = lan.get("players").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let chat: Vec<String> = lan.get("chat").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect()).unwrap_or_default();
        let warnings: Vec<String> = lan.get("warnings").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect()).unwrap_or_default();
        let log_open = l.state.open_logs.contains(&i.pid);
        let log_lines = l.state.logs.get(&i.pid).cloned().unwrap_or_default();
        let mut h = 96.0;
        if role == "host" {
            h += 96.0;
        } else if !role.is_empty() {
            h += 24.0;
        }
        h += if players.is_empty() { 0.0 } else { 26.0 + players.len() as f32 * 20.0 };
        h += if chat.is_empty() { 0.0 } else { 26.0 + chat.len().min(6) as f32 * 18.0 };
        h += warnings.len() as f32 * 20.0;
        if log_open {
            h += 220.0;
        }
        let r = Rect::new(view.x, y, view.w, h);
        l.ui.card(r);
        let running = i.running;
        let c = Vec2::new(r.x + 24.0, r.y + 28.0);
        if running {
            let pulse = (l.ui.time * 3.0).sin() * 0.5 + 0.5;
            l.ui.p().circle(c, 6.0 + 3.0 * pulse, OK.alpha(0.25));
        }
        l.ui.p().circle(c, 6.0, if running { OK } else { TEXT_FAINT });
        let duty = i.line.as_ref().map(|ln| match &i.tour {
            Some(t) => format!(" | {}", omsi_ui::tr("line %{line} / %{tour}").replace("%{line}", ln).replace("%{tour}", t)),
            None => format!(" | {}", omsi_ui::tr("line %{line}").replace("%{line}", ln)),
        }).unwrap_or_default();
        l.ui.text_in(&format!("{} | {}{duty}", short_map(&i.map), short_bus(&i.bus)), Rect::new(r.x + 42.0, r.y + 16.0, r.w - 260.0, 24.0), 16.0, Weight::Black, TEXT, Align::Left);
        let status = if running {
            if l.state.stopping.contains(&i.pid) || i.stopping.is_some() { omsi_ui::tr("stopping - saving the run...").into_owned() } else { omsi_ui::tr("running for %{time}").replace("%{time}", &ago(i.started)) }
        } else if i.exit_code == Some(0) {
            omsi_ui::tr("ended").into_owned()
        } else if i.killed {
            omsi_ui::tr("ended (killed - it did not end by itself, the run is not saved)").into_owned()
        } else {
            match i.exit_code {
                Some(c) => omsi_ui::tr("ended (exit code %{code})").replace("%{code}", &c.to_string()),
                None => omsi_ui::tr("ended").into_owned(),
            }
        };
        l.ui.text_in(&format!("{status} | {}", omsi_ui::tr("driver %{name}").replace("%{name}", &i.profile)), Rect::new(r.x + 42.0, r.y + 42.0, r.w - 60.0, 18.0), 12.0, Weight::Regular, TEXT_DIM, Align::Left);
        l.ui.text_in(&i.last_line, Rect::new(r.x + 42.0, r.y + 62.0, r.w - 60.0, 18.0), 11.5, Weight::Regular, TEXT_FAINT, Align::Left);
        // buttons
        let bw = 110.0;
        if running {
            let stopping = l.state.stopping.contains(&i.pid);
            if l.ui.button(&format!("stop-{}", i.pid), Rect::new(r.right() - 18.0 - bw, r.y + 14.0, bw, 34.0), if stopping { "Stopping..." } else { "Stop" }, Some("close"), ButtonKind::Danger) && !stopping {
                actions.push((i.pid, "stop"));
            }
        }
        if l.ui.button(&format!("log-{}", i.pid), Rect::new(r.right() - 18.0 - bw - if running { bw + 8.0 } else { 0.0 }, r.y + 14.0, bw, 34.0), if log_open { "Hide log" } else { "Show log" }, Some("receipt_long"), ButtonKind::Normal) {
            actions.push((i.pid, "log"));
        }
        let mut yy = r.y + 86.0;
        if role == "host" {
            let code = lan.get("code").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let box_r = Rect::new(r.x + 20.0, yy, r.w - 40.0, 84.0);
            l.ui.p().rounded(box_r, 10.0, accent().alpha(0.10));
            l.ui.p().rounded_border(box_r, 10.0, 1.0, accent().alpha(0.5));
            l.ui.text_in("SESSION CODE", Rect::new(box_r.x + 16.0, box_r.y + 8.0, 200.0, 16.0), 10.5, Weight::Black, accent(), Align::Left);
            l.ui.text_in(&code, Rect::new(box_r.x + 16.0, box_r.y + 26.0, box_r.w - 170.0, 30.0), 20.0, Weight::Condensed, TEXT, Align::Left);
            l.ui.text_in("Your friends paste it into Multiplayer → Connect by Code.", Rect::new(box_r.x + 16.0, box_r.y + 58.0, box_r.w - 170.0, 18.0), 11.5, Weight::Regular, TEXT_DIM, Align::Left);
            if l.ui.button(&format!("copy-{}", i.pid), Rect::new(box_r.right() - 146.0, box_r.y + 24.0, 130.0, 36.0), "Copy code", Some("content_copy"), ButtonKind::Primary) {
                copy = Some(code.clone());
            }
            yy += 96.0;
        } else if role == "client" {
            let connected = lan.get("connected").and_then(|x| x.as_bool()).unwrap_or(false);
            let text = if let Some(rej) = lan.get("rejected").and_then(|x| x.as_str()) {
                omsi_ui::tr("not connected: %{reason}").replace("%{reason}", rej)
            } else if connected {
                omsi_ui::tr("connected to %{host}").replace("%{host}", lan.get("host_name").and_then(|x| x.as_str()).unwrap_or(""))
            } else {
                omsi_ui::tr("connecting...").into_owned()
            };
            l.ui.text_in(&omsi_ui::tr("Multiplayer: %{state}").replace("%{state}", &text), Rect::new(r.x + 42.0, yy, r.w - 60.0, 20.0), 12.5, Weight::Medium, if connected { OK } else { WARN }, Align::Left);
            yy += 24.0;
        }
        if !players.is_empty() {
            l.ui.text_in("PLAYERS", Rect::new(r.x + 42.0, yy, 200.0, 18.0), 10.5, Weight::Black, TEXT_FAINT, Align::Left);
            yy += 22.0;
            for p in &players {
                let s = |k: &str| p.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                let pax = p.get("passengers").and_then(|x| x.as_i64()).unwrap_or(0);
                let dest = if s("destination").is_empty() { String::new() } else { format!(" | {} → {}", s("line"), s("destination")) };
                l.ui.text_in(&format!("{} | {}{dest}{} | {}", s("name"), short_bus(&s("bus")), if pax > 0 { format!(" | {}", omsi_ui::tr("%{n} passengers").replace("%{n}", &pax.to_string())) } else { String::new() }, s("where")), Rect::new(r.x + 42.0, yy, r.w - 60.0, 18.0), 12.5, Weight::Medium, TEXT_SOFT, Align::Left);
                yy += 20.0;
            }
        }
        if !chat.is_empty() {
            l.ui.text_in("CHAT  (V in the game to write)", Rect::new(r.x + 42.0, yy + 4.0, 300.0, 18.0), 10.5, Weight::Black, TEXT_FAINT, Align::Left);
            yy += 26.0;
            for c in chat.iter().rev().take(6).rev() {
                l.ui.text_in(c, Rect::new(r.x + 42.0, yy, r.w - 60.0, 18.0), 12.0, Weight::Regular, TEXT_SOFT, Align::Left);
                yy += 18.0;
            }
        }
        for w in &warnings {
            l.ui.icon("warning", Vec2::new(r.x + 50.0, yy + 9.0), 15.0, WARN);
            l.ui.text_in(w, Rect::new(r.x + 64.0, yy, r.w - 80.0, 18.0), 12.0, Weight::Medium, WARN, Align::Left);
            yy += 20.0;
        }
        if log_open {
            let lr = Rect::new(r.x + 20.0, yy + 6.0, r.w - 40.0, 204.0);
            l.ui.p().rounded(lr, 8.0, Color::rgba(6, 8, 10, 0.9));
            let text: Vec<String> = log_lines.iter().rev().take(11).rev().cloned().collect();
            for (k, line) in text.iter().enumerate() {
                l.ui.text_in(line, Rect::new(lr.x + 10.0, lr.y + 6.0 + k as f32 * 18.0, lr.w - 20.0, 18.0), 11.0, Weight::Regular, if line.contains("ERROR") { DANGER } else if line.contains("WARN") { WARN } else { TEXT_DIM }, Align::Left);
            }
        }
        y += h + GAP;
    }
    l.ui.pop_clip();
    // scrolling the list
    let content = y + l.ui.scroll.get(&id_of("sessions")).copied().unwrap_or(0.0) - body.y;
    if l.ui.hover(view) && l.ui.input.wheel.y.abs() > 0.0 {
        let s = l.ui.scroll.entry(id_of("sessions")).or_insert(0.0);
        *s = (*s - l.ui.input.wheel.y * 40.0).clamp(0.0, (content - view.h).max(0.0));
    }
    for (pid, what) in actions {
        match what {
            "stop" => l.state.stop(pid),
            _ => {
                if !l.state.open_logs.remove(&pid) {
                    l.state.open_logs.insert(pid);
                    l.state.log_tail(pid);
                }
            }
        }
    }
    if let Some(c) = copy {
        l.ui.clipboard_out = Some(c);
        l.state.set_status("Session code copied.", false);
    }
    let _ = hhmm;
}
