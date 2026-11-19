//! The panel tables read into [`Panel`]s, `omsi.ui.toast`'s options and `ui.update`'s keys:
//! held to the limits and checked here, so that a plugin learns at once what is wrong.

use super::{
    Align, Anchor, CardSet, ChartStyle, Element, Kind, MAX_DEPTH, MAX_ELEMENTS, MAX_POINTS,
    MAX_TEXT, Panel, Rgba, RowAlign, Stop, StopState, Weight, check_id,
};
use crate::api::Value;
use std::path::{Path, PathBuf};

/// A table's rows of texts, and each cell's own colour (row by row; empty: none has one).
type TableRows = (Vec<Vec<String>>, Vec<Vec<Option<Rgba>>>);

/// Read a panel table. Unknown keys are left alone and elements of an unknown type skipped,
/// so a plugin written for a later game still shows here; a key of the wrong kind is an
/// error that says where it is. `folder`: the plugin's own, where its images are.
pub fn parse_panel(t: &Value, folder: Option<&Path>) -> Result<Panel, String> {
    let r = Reader {
        t,
        path: String::new(),
        folder,
    };
    if !t.is_table() {
        return Err("the panel is a table".into());
    }
    let mut count = 0;
    Ok(Panel {
        anchor: match r.string("anchor")? {
            None => Anchor::TopLeft,
            Some(a) => Anchor::parse(&a).ok_or_else(|| format!("anchor: \"{a}\" is no anchor (top_left, top, top_right, left, center, right, bottom_left, bottom, bottom_right)"))?,
        },
        x: r.number("x", 0.0, -10000.0, 10000.0)?,
        y: r.number("y", 0.0, -10000.0, 10000.0)?,
        width: r.number("width", 300.0, 40.0, 4000.0)?,
        padding: r.number("padding", 12.0, 0.0, 64.0)?,
        gap: r.number("gap", 6.0, 0.0, 64.0)?,
        background: r.color("background")?,
        radius: r.number("radius", 12.0, 0.0, 64.0)?,
        accent: r.color("accent")?,
        visible: r.boolean("visible", true)?,
        clickable: r.boolean("clickable", false)?,
        draggable: r.boolean("draggable", false)?,
        grab: r.boolean("grab", false)?,
        frame: r.boolean("frame", false)?,
        children: r.children("children", 0, &mut count)?,
    })
}

/// What `omsi.ui.toast(text, opts)` asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct ToastSpec {
    pub text: String,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub color: Option<Rgba>,
    pub seconds: f32,
}

/// Read `omsi.ui.toast`'s text and options (`seconds`, `title`, `icon`, `color`) with the
/// same limits and messages as a panel's.
pub fn parse_toast(text: &Value, opts: Option<&Value>) -> Result<ToastSpec, String> {
    let text = match text {
        Value::Str(_) | Value::Int(_) | Value::Num(_) => text.to_text().unwrap_or_default(),
        _ => return Err("the text of a notification is a text".to_string()),
    };
    if text.chars().count() > MAX_TEXT {
        return Err(format!("the text is longer than {MAX_TEXT} characters"));
    }
    let mut spec = ToastSpec {
        text,
        title: None,
        icon: None,
        color: None,
        seconds: 5.0,
    };
    if let Some(t) = opts {
        let r = Reader {
            t,
            path: "opts".into(),
            folder: None,
        };
        spec.title = r.string("title")?;
        spec.icon = r.string("icon")?;
        spec.color = r.color("color")?;
        spec.seconds = r.number("seconds", 5.0, 1.0, 60.0)?;
    }
    Ok(spec)
}

/// A panel written as a plugin writes it - a Lua table constructor, `{ width = 300, ... }` -
/// read in a Lua state of its own: for the game's tests and pictures of panels.
pub fn panel_from_source(src: &str) -> Result<Panel, String> {
    let lua = mlua::Lua::new();
    let t: mlua::Value = lua
        .load(format!("return {src}"))
        .eval()
        .map_err(|e| e.to_string())?;
    parse_panel(&crate::lua::to_value(&t)?, None)
}

/// Change the keys `fields` gives of an element (`ui.update`).
pub(super) fn patch(e: &mut Element, fields: &Value, folder: Option<&Path>) -> Result<(), String> {
    let r = Reader {
        t: fields,
        path: e.id.clone().unwrap_or_default(),
        folder,
    };
    if let Some(c) = r.color("color")? {
        e.color = Some(c);
    }
    let visible = r.boolean("visible", true)?;
    let has = |k: &str| fields.get(k).is_some_and(|v| !v.is_nil());
    match &mut e.kind {
        Kind::Text { text, .. }
        | Kind::Badge { text, .. }
        | Kind::Button { text, .. }
        | Kind::Checkbox { text, .. }
            if has("text") =>
        {
            *text = r.string("text")?.unwrap_or_default();
        }
        Kind::Input { text, .. } if has("text") => *text = r.string("text")?.unwrap_or_default(),
        _ => {}
    }
    match &mut e.kind {
        Kind::Bar {
            value, segments, ..
        } if has("value") || has("segments") => {
            if has("value") {
                *value = r.number("value", 0.0, 0.0, 1.0)?;
            }
            if has("segments") {
                *segments = r.segments("segments")?;
            }
        }
        Kind::Stops { stops, .. } if has("stops") => *stops = r.stops("stops")?,
        Kind::Slider {
            value, min, max, ..
        } if has("value") => *value = r.number("value", *min, min.min(*max), max.max(*min))?,
        Kind::Checkbox { checked, .. } if has("checked") => {
            *checked = r.boolean("checked", false)?
        }
        Kind::Tabs { selected, tabs } if has("selected") => {
            *selected = (r.number("selected", 1.0, 1.0, tabs.len().max(1) as f32)? as usize)
                .saturating_sub(1)
        }
        Kind::Icon { name, .. } if has("name") => *name = r.string("name")?.unwrap_or_default(),
        Kind::Chart { values, .. } if has("values") => *values = r.numbers("values")?,
        Kind::Table {
            rows, inks, aligns, ..
        } if has("rows") || has("aligns") => {
            if has("rows") {
                (*rows, *inks) = r.rows("rows")?;
            }
            if has("aligns") {
                *aligns = r.aligns("aligns")?;
            }
        }
        Kind::Image { path, .. } if has("src") => *path = r.image("src")?,
        _ => {}
    }
    // (an element hidden by an update stays in the panel, empty: `visible = true` brings it back)
    if !visible {
        e.kind = match &e.kind {
            Kind::Text {
                size,
                weight,
                align,
                wrap,
                ..
            } => Kind::Text {
                text: String::new(),
                size: *size,
                weight: *weight,
                align: *align,
                wrap: *wrap,
            },
            k => k.clone(),
        };
    }
    Ok(())
}

/// Reads the keys of one table; `path` says where it is for the error messages.
struct Reader<'a> {
    t: &'a Value,
    path: String,
    folder: Option<&'a Path>,
}

impl Reader<'_> {
    fn get(&self, key: &str) -> &Value {
        self.t.get(key).unwrap_or(&Value::Nil)
    }

    fn at(&self, key: &str) -> String {
        if self.path.is_empty() {
            key.to_string()
        } else {
            format!("{}.{key}", self.path)
        }
    }

    fn number(&self, key: &str, default: f32, min: f32, max: f32) -> Result<f32, String> {
        let v = match self.get(key) {
            Value::Nil => return Ok(default),
            Value::Int(i) => *i as f64,
            Value::Num(n) if n.is_finite() => *n,
            _ => return Err(format!("{}: a number is expected", self.at(key))),
        };
        Ok((v as f32).clamp(min, max))
    }

    fn opt_number(&self, key: &str) -> Result<Option<f32>, String> {
        match self.get(key) {
            Value::Nil => Ok(None),
            _ => self.number(key, 0.0, f32::MIN, f32::MAX).map(Some),
        }
    }

    fn boolean(&self, key: &str, default: bool) -> Result<bool, String> {
        match self.get(key) {
            Value::Nil => Ok(default),
            Value::Bool(b) => Ok(*b),
            _ => Err(format!("{}: true or false is expected", self.at(key))),
        }
    }

    /// The `show` of a `map`: which of the navigator's cards show (each on unless it is `false`).
    fn card_set(&self) -> Result<CardSet, String> {
        let t = self.get("show");
        if matches!(t, Value::Nil) {
            return Ok(CardSet::default());
        }
        if !t.is_table() {
            return Err(format!("{}: a table is expected", self.at("show")));
        }
        let r = Reader {
            t,
            path: self.at("show"),
            folder: None,
        };
        Ok(CardSet {
            turn: r.boolean("turn", true)?,
            clock: r.boolean("clock", true)?,
            speed: r.boolean("speed", true)?,
            stop: r.boolean("stop", true)?,
        })
    }

    /// A text (numbers are written as Lua writes them), at most `MAX_TEXT` characters.
    fn string(&self, key: &str) -> Result<Option<String>, String> {
        let s = match self.get(key) {
            Value::Nil => return Ok(None),
            v @ (Value::Str(_) | Value::Int(_) | Value::Num(_)) => v.to_text().unwrap_or_default(),
            _ => return Err(format!("{}: a text is expected", self.at(key))),
        };
        if s.chars().count() > MAX_TEXT {
            return Err(format!(
                "{}: longer than {MAX_TEXT} characters",
                self.at(key)
            ));
        }
        Ok(Some(s))
    }

    fn color(&self, key: &str) -> Result<Option<Rgba>, String> {
        match self.string(key)? {
            None => Ok(None),
            Some(s) => Rgba::parse(&s).map(Some).ok_or_else(|| {
                format!(
                    "{}: \"{s}\" is no colour (\"#RRGGBB\" or \"#RRGGBBAA\")",
                    self.at(key)
                )
            }),
        }
    }

    fn list(&self, key: &str) -> Result<&[Value], String> {
        match self.get(key) {
            Value::Nil => Ok(&[]),
            Value::List(l) => Ok(l),
            Value::Map(m) if m.is_empty() => Ok(&[]),
            _ => Err(format!("{}: a list is expected", self.at(key))),
        }
    }

    fn texts(&self, key: &str) -> Result<Vec<String>, String> {
        let l = self.list(key)?;
        if l.len() > MAX_POINTS {
            return Err(format!("{}: at most {MAX_POINTS} entries", self.at(key)));
        }
        l.iter()
            .enumerate()
            .map(|(i, v)| {
                let s = v
                    .to_text()
                    .ok_or_else(|| format!("{}[{}]: a text is expected", self.at(key), i + 1))?;
                Ok(s.chars().take(MAX_TEXT).collect())
            })
            .collect()
    }

    fn numbers(&self, key: &str) -> Result<Vec<f32>, String> {
        let l = self.list(key)?;
        if l.len() > MAX_POINTS {
            return Err(format!("{}: at most {MAX_POINTS} values", self.at(key)));
        }
        l.iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_f64()
                    .filter(|n| n.is_finite())
                    .map(|n| n as f32)
                    .ok_or_else(|| format!("{}[{}]: a number is expected", self.at(key), i + 1))
            })
            .collect()
    }

    /// A table's rows, and the colours of the cells that have their own (none at all: an
    /// empty list). A cell is a text, or `{ text = "+3:12", color = "#FF5F57" }`.
    fn rows(&self, key: &str) -> Result<TableRows, String> {
        let l = self.list(key)?;
        let mut cells = 0;
        let mut out = Vec::new();
        let mut inks = Vec::new();
        for (i, row) in l.iter().enumerate() {
            let r = Reader {
                t: row,
                path: format!("{}[{}]", self.at(key), i + 1),
                folder: None,
            };
            let Value::List(c) = row else {
                return Err(format!("{}: a row is a list of texts", r.path));
            };
            let mut cols = Vec::with_capacity(c.len());
            let mut colors = Vec::with_capacity(c.len());
            for (j, v) in c.iter().enumerate() {
                let (text, ink) = if v.is_table() {
                    let cell = Reader {
                        t: v,
                        path: format!("{}[{}]", r.path, j + 1),
                        folder: None,
                    };
                    (
                        cell.string("text")?.unwrap_or_default(),
                        cell.color("color")?,
                    )
                } else {
                    (v.to_text().unwrap_or_default(), None)
                };
                cols.push(text.chars().take(MAX_TEXT).collect());
                colors.push(ink);
            }
            cells += cols.len();
            if cells > MAX_POINTS {
                return Err(format!(
                    "{}: a table has at most {MAX_POINTS} cells",
                    self.at(key)
                ));
            }
            out.push(cols);
            inks.push(colors);
        }
        if inks.iter().flatten().all(Option::is_none) {
            inks.clear();
        }
        Ok((out, inks))
    }

    /// A table's column alignments: `{ "left", "right", "center" }`.
    fn aligns(&self, key: &str) -> Result<Vec<Align>, String> {
        self.texts(key)?
            .iter()
            .map(|a| match a.as_str() {
                "left" => Ok(Align::Left),
                "center" => Ok(Align::Center),
                "right" => Ok(Align::Right),
                _ => Err(format!(
                    "{}: \"{a}\" is no alignment (left, center, right)",
                    self.at(key)
                )),
            })
            .collect()
    }

    fn weight(&self, default: Weight) -> Result<Weight, String> {
        Ok(match self.string("weight")?.as_deref() {
            None => default,
            Some("regular") => Weight::Regular,
            Some("medium") => Weight::Medium,
            Some("bold") => Weight::Bold,
            Some(w) => {
                return Err(format!(
                    "{}: \"{w}\" is no weight (regular, medium, bold)",
                    self.at("weight")
                ));
            }
        })
    }

    /// A bar's parts: `{ { value = 0.6, color = "#34B26E" }, ... }`.
    fn segments(&self, key: &str) -> Result<Vec<(f32, Rgba)>, String> {
        let l = self.list(key)?;
        if l.len() > 16 {
            return Err(format!("{}: at most 16 parts", self.at(key)));
        }
        l.iter()
            .enumerate()
            .map(|(i, t)| {
                let r = Reader {
                    t,
                    path: format!("{}[{}]", self.at(key), i + 1),
                    folder: None,
                };
                if !t.is_table() {
                    return Err(format!("{}: a part is a table", r.path));
                }
                Ok((
                    r.number("value", 0.0, 0.0, 1e9)?,
                    r.color("color")?.unwrap_or(Rgba([232, 160, 48, 255])),
                ))
            })
            .collect()
    }

    /// A list of stops: `{ { name = "Rathaus", time = "07:45", expected = "07:49",
    /// expected_color = "#FF5F57", state = "now", tag = "terminus" }, ... }`.
    fn stops(&self, key: &str) -> Result<Vec<Stop>, String> {
        let l = self.list(key)?;
        if l.len() > MAX_POINTS / 4 {
            return Err(format!(
                "{}: at most {} stops",
                self.at(key),
                MAX_POINTS / 4
            ));
        }
        l.iter()
            .enumerate()
            .map(|(i, t)| {
                let r = Reader {
                    t,
                    path: format!("{}[{}]", self.at(key), i + 1),
                    folder: None,
                };
                if !t.is_table() {
                    return Err(format!("{}: a stop is a table", r.path));
                }
                Ok(Stop {
                    name: r.string("name")?.unwrap_or_default(),
                    time: r.string("time")?.unwrap_or_default(),
                    expected: r.string("expected")?.unwrap_or_default(),
                    expected_color: r.color("expected_color")?,
                    state: match r.string("state")?.as_deref() {
                        None | Some("ahead") => StopState::Ahead,
                        Some("now") => StopState::Now,
                        Some("done") => StopState::Done,
                        Some(s) => {
                            return Err(format!(
                                "{}: \"{s}\" is no state (done, now, ahead)",
                                r.at("state")
                            ));
                        }
                    },
                    tag: r.string("tag")?.unwrap_or_default(),
                })
            })
            .collect()
    }

    /// An image of the plugin's folder: the file when it is there.
    fn image(&self, key: &str) -> Result<Option<PathBuf>, String> {
        let Some(src) = self.string(key)? else {
            return Ok(None);
        };
        let Some(folder) = self.folder else {
            return Ok(None);
        };
        let p = crate::api::paths::inside(folder, &src)
            .map_err(|e| format!("{}: {e}", self.at(key)))?;
        Ok(p.is_file().then_some(p))
    }

    fn children(&self, key: &str, depth: usize, count: &mut usize) -> Result<Vec<Element>, String> {
        let list = match self.get(key) {
            Value::Nil => return Ok(Vec::new()),
            Value::List(l) => l.as_slice(),
            Value::Map(m) if m.is_empty() => &[],
            _ => return Err(format!("{}: a list of elements is expected", self.at(key))),
        };
        if depth >= MAX_DEPTH {
            return Err(format!(
                "{}: rows go at most {MAX_DEPTH} deep",
                self.at(key)
            ));
        }
        let mut out = Vec::new();
        for (i, t) in list.iter().enumerate() {
            let path = format!("{}[{}]", self.at(key), i + 1);
            if !t.is_table() {
                return Err(format!("{path}: an element is a table"));
            }
            *count += 1;
            if *count > MAX_ELEMENTS {
                return Err(format!("a panel has at most {MAX_ELEMENTS} elements"));
            }
            if let Some(e) = (Reader {
                t,
                path,
                folder: self.folder,
            })
            .element(depth, count)?
            {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// One element; None when it is hidden or of a type this game does not know.
    fn element(&self, depth: usize, count: &mut usize) -> Result<Option<Element>, String> {
        let ty = self
            .string("type")?
            .ok_or_else(|| format!("{}: no type", self.path))?;
        let id = self.string("id")?;
        if let Some(id) = id.as_deref() {
            check_id(&self.at("id"), id)?;
        }
        let text = || self.string("text").map(Option::unwrap_or_default);
        let needs_id = |what: &str| -> Result<(), String> {
            match id {
                Some(_) => Ok(()),
                None => Err(format!("{}: a {what} needs an id", self.path)),
            }
        };
        let kind = match ty.as_str() {
            "text" => Kind::Text {
                text: text()?,
                size: self.number("size", 14.0, 6.0, 96.0)?,
                weight: self.weight(Weight::Regular)?,
                align: match self.string("align")?.as_deref() {
                    None | Some("left") => Align::Left,
                    Some("center") => Align::Center,
                    Some("right") => Align::Right,
                    Some(a) => {
                        return Err(format!(
                            "{}: \"{a}\" is no alignment (left, center, right)",
                            self.at("align")
                        ));
                    }
                },
                wrap: self.boolean("wrap", true)?,
            },
            "icon" => Kind::Icon {
                name: self.string("name")?.unwrap_or_default(),
                size: self.number("size", 20.0, 6.0, 128.0)?,
            },
            "row" => Kind::Row {
                children: self.children("children", depth + 1, count)?,
                gap: self.number("gap", 8.0, 0.0, 64.0)?,
                align: match self.string("align")?.as_deref() {
                    None | Some("start") => RowAlign::Start,
                    Some("center") => RowAlign::Center,
                    Some("end") => RowAlign::End,
                    Some("between") => RowAlign::Between,
                    Some(a) => {
                        return Err(format!(
                            "{}: \"{a}\" is no alignment (start, center, end, between)",
                            self.at("align")
                        ));
                    }
                },
            },
            "bar" => Kind::Bar {
                value: self.number("value", 0.0, 0.0, 1.0)?,
                height: self.number("height", 6.0, 1.0, 64.0)?,
                background: self.color("background")?,
                segments: self.segments("segments")?,
            },
            "badge" => Kind::Badge {
                text: text()?,
                text_color: self.color("text_color")?,
                weight: self.weight(Weight::Medium)?,
            },
            "box" => Kind::Box {
                children: self.children("children", depth + 1, count)?,
                gap: self.number("gap", 4.0, 0.0, 64.0)?,
            },
            "stops" => Kind::Stops {
                stops: self.stops("stops")?,
                size: self.number("size", 13.0, 6.0, 48.0)?,
            },
            "divider" => Kind::Divider,
            "map" => Kind::Map {
                height: self.number("height", 0.0, 0.0, 4000.0)?,
                cards: self.boolean("cards", true)?,
                show: self.card_set()?,
            },
            "space" => Kind::Space {
                size: self.number("size", 8.0, 0.0, 1000.0)?,
            },
            "button" => {
                needs_id("button")?;
                Kind::Button {
                    text: text()?,
                    icon: self.string("icon")?,
                }
            }
            "image" => Kind::Image {
                path: self.image("src")?,
                width: self.number("width", 64.0, 1.0, 4000.0)?,
                height: self.number("height", 64.0, 1.0, 4000.0)?,
            },
            "checkbox" => {
                needs_id("checkbox")?;
                Kind::Checkbox {
                    text: text()?,
                    checked: self.boolean("checked", false)?,
                }
            }
            "slider" => {
                needs_id("slider")?;
                let min = self.number("min", 0.0, -1e9, 1e9)?;
                let max = self.number("max", 1.0, -1e9, 1e9)?;
                Kind::Slider {
                    value: self.number("value", min, min.min(max), max.max(min))?,
                    min,
                    max,
                    step: self.number("step", 0.0, 0.0, 1e9)?,
                }
            }
            "input" => {
                needs_id("text field")?;
                let max = self.number("max", 100.0, 1.0, MAX_TEXT as f32)? as usize;
                Kind::Input {
                    text: text()?.chars().take(max).collect(),
                    placeholder: self.string("placeholder")?.unwrap_or_default(),
                    max,
                }
            }
            "tabs" => {
                needs_id("tabs element")?;
                let tabs = self.texts("tabs")?;
                let n = tabs.len().max(1) as f32;
                Kind::Tabs {
                    selected: (self.number("selected", 1.0, 1.0, n)? as usize).saturating_sub(1),
                    tabs,
                }
            }
            "chart" => Kind::Chart {
                values: self.numbers("values")?,
                min: self.opt_number("min")?,
                max: self.opt_number("max")?,
                height: self.number("height", 48.0, 8.0, 1000.0)?,
                style: match self.string("style")?.as_deref() {
                    None | Some("line") => ChartStyle::Line,
                    Some("bars") => ChartStyle::Bars,
                    Some(s) => {
                        return Err(format!(
                            "{}: \"{s}\" is no chart style (line, bars)",
                            self.at("style")
                        ));
                    }
                },
                fill: self.boolean("fill", false)?,
            },
            "table" => {
                let (rows, inks) = self.rows("rows")?;
                Kind::Table {
                    columns: self.texts("columns")?,
                    rows,
                    widths: self.numbers("widths")?,
                    size: self.number("size", 13.0, 6.0, 48.0)?,
                    aligns: self.aligns("aligns")?,
                    inks,
                }
            }
            // (an element of a later version of the API: left out)
            _ => return Ok(None),
        };
        let e = Element {
            id,
            color: self.color("color")?,
            grow: self.boolean("grow", false)?,
            clickable: self.boolean("clickable", false)?,
            kind,
        };
        Ok(self.boolean("visible", true)?.then_some(e))
    }
}
