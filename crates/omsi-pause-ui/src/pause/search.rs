//! Search input integration for the pause settings pages.
use super::*;
pub type State = omsi_settings_ui::SearchState;
const OPTIONS_SEARCH_PAGE: &str = "Options";

pub(super) fn toolbar(sh: &mut Shell, list: &ListView, area: Rect, body: &mut Rect) {
    if sh.state.settings_search.page != OPTIONS_SEARCH_PAGE {
        sh.state.settings_search = State {
            page: OPTIONS_SEARCH_PAGE.to_owned(),
            ..Default::default()
        };
        sh.ui.focus = None;
    }
    let search = &mut sh.state.settings_search;
    let ui = &mut sh.ui;
    let (field, body_offset) = omsi_settings_ui::toolbar_layout(area, *body);
    let changed = ui.text_input(
        "pause-settings-search",
        field,
        &mut search.query,
        &omsi_ui::tr("Search settings"),
        Some("search"),
    );
    if ui.flat {
        ui.p()
            .rounded_border(field, corner(8.0), super::EDGE_WIDTH, super::visible_edge());
    }
    let clear = ui.icon_button(
        "pause-settings-search-clear",
        Vec2::new(field.right() + 22.0, field.center().y),
        18.0,
        "close",
        "Clear search",
    );
    if clear {
        search.query.clear();
        ui.focus = None;
    }
    if changed || clear {
        let id = id_of(&format!("pause-list-{}", list.key));
        sh.ui.scroll.insert(id, 0.0);
        sh.ui.scroll.insert(id ^ 0xabc, 0.0);
        sh.last_sel = None;
    }
    *body = Rect::new(
        body.x,
        body.y + body_offset,
        body.w,
        (body.h - body_offset).max(0.0),
    );
}

pub(super) fn field(sh: &mut Shell, list: &ListView, field: Rect) {
    if sh.state.settings_search.page != OPTIONS_SEARCH_PAGE {
        sh.state.settings_search = State {
            page: OPTIONS_SEARCH_PAGE.to_owned(),
            ..Default::default()
        };
        sh.ui.focus = None;
    }
    let search = &mut sh.state.settings_search;
    let ui = &mut sh.ui;
    let changed = ui.text_input(
        "pause-settings-search",
        field,
        &mut search.query,
        &omsi_ui::tr("Search settings"),
        Some("search"),
    );
    if ui.flat {
        ui.p()
            .rounded_border(field, corner(8.0), super::EDGE_WIDTH, super::visible_edge());
    }
    let clear = ui.icon_button(
        "pause-settings-search-clear",
        Vec2::new(field.right() - 18.0, field.center().y),
        16.0,
        "close",
        "Clear search",
    );
    if clear {
        search.query.clear();
        ui.focus = None;
    }
    if changed || clear {
        let id = id_of(&format!("pause-list-{}", list.key));
        sh.ui.scroll.insert(id, 0.0);
        sh.ui.scroll.insert(id ^ 0xabc, 0.0);
        sh.last_sel = None;
    }
}

pub fn focused(sh: &Shell) -> bool {
    sh.ui.focus.is_some_and(|id| {
        id == id_of("pause-settings-search") || id == id_of("pause-online-search")
    })
}

pub fn key(sh: &mut Shell, code: winit::keyboard::KeyCode, text: Option<&str>) -> bool {
    let field = if sh.ui.focus == Some(id_of("pause-online-search")) {
        "pause-online-search"
    } else {
        "pause-settings-search"
    };
    omsi_settings_ui::search_key(&mut sh.ui, field, code, text)
}
