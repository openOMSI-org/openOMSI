//! Translate the existing vehicle selection workflows into the launcher's preview contract.

use super::{App, ListKind, ORIGINAL_PAINT};
use crate::launcher::showroom::Look;

pub(crate) fn is_vehicle_list(kind: Option<&ListKind>) -> bool {
    matches!(
        kind,
        Some(
            ListKind::Liveries
                | ListKind::PlaceMaker
                | ListKind::PlaceType(_)
                | ListKind::PlaceLivery(_)
                | ListKind::PlaceHof(..)
        )
    )
}

pub(crate) fn selectable(action: &str) -> bool {
    ["bus ", "livery ", "placehof ", "depot "]
        .iter()
        .any(|prefix| action.starts_with(prefix))
}

fn vehicle_paint(kind: &ListKind, action: &str) -> Option<(String, String)> {
    let (bus, paint) = match kind {
        ListKind::PlaceMaker | ListKind::PlaceType(_) => (action.strip_prefix("bus ")?, ""),
        ListKind::PlaceLivery(bus) => (bus.as_str(), action.strip_prefix("livery ")?),
        ListKind::PlaceHof(bus, paint) if action.starts_with("placehof ") => {
            (bus.as_str(), paint.as_str())
        }
        _ => return None,
    };
    Some((
        bus.to_string(),
        if paint == ORIGINAL_PAINT {
            String::new()
        } else {
            paint.to_string()
        },
    ))
}

fn repaint_scheme(action: &str, count: usize) -> Option<Option<usize>> {
    let index = super::liveries::chosen(action.strip_prefix("livery ")?)?;
    if index.is_some_and(|i| i >= count) {
        return None;
    }
    Some(index)
}

pub(crate) fn repaint_bus_path(root: &std::path::Path, file: &std::path::Path) -> String {
    let roots = omsi_cfg::content_roots();
    let relative = roots
        .iter()
        .map(|root| root.as_path())
        .chain(std::iter::once(root))
        .filter(|root| file.starts_with(root))
        .max_by_key(|root| root.components().count())
        .and_then(|root| file.strip_prefix(root).ok())
        .unwrap_or(file);
    relative.to_string_lossy().replace(char::from(92), "/")
}

pub(crate) fn selection(app: &App) -> (Option<Look>, String, String) {
    let Some((kind, sel)) = app.menus.list_kind.as_ref().zip(app.menus.chooser) else {
        return Default::default();
    };
    let Some((label, action)) = app.menus.admin_list.as_ref().and_then(|rows| rows.get(sel)) else {
        return Default::default();
    };
    let selected = if matches!(kind, ListKind::Liveries) {
        app.player.as_ref().and_then(|p| {
            let index = repaint_scheme(action, p.vehicle.ty.paint_schemes.len())?;
            let bus = repaint_bus_path(&app.args.root, &p.vehicle.ty.def.path);
            Some((bus, String::new(), index))
        })
    } else {
        vehicle_paint(kind, action)
            .or_else(|| {
                let id = action.strip_prefix("depot ")?;
                let bus = crate::depot_bus::listed()
                    .into_iter()
                    .find(|bus| bus.id == id)?;
                Some((bus.bus, bus.paint))
            })
            .map(|(bus, paint)| (bus, paint, None))
    };
    let Some((bus, paint, paint_index)) = selected else {
        return (None, label.clone(), String::new());
    };
    let title = app
        .menus
        .vehicle_meta
        .get(&bus)
        .map(|(maker, model)| {
            format!("{} {}", maker.trim(), model.trim())
                .trim()
                .to_string()
        })
        .or_else(|| {
            let player = app
                .player
                .as_ref()
                .filter(|_| matches!(kind, ListKind::Liveries))?;
            let def = &player.vehicle.ty.def;
            Some(
                format!("{} {}", def.manufacturer.trim(), def.type_name.trim())
                    .trim()
                    .to_string(),
            )
        })
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| label.clone());
    let meta = if matches!(
        kind,
        ListKind::Liveries | ListKind::PlaceLivery(_) | ListKind::PlaceHof(..)
    ) {
        label.clone()
    } else {
        bus.clone()
    };
    (
        Some(Look {
            root: app.args.root.clone(),
            map: app.args.map.clone(),
            bus,
            paint,
            paint_index,
            ..Default::default()
        }),
        title,
        meta,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_game_bus_preview_repaint_resolves_the_loaded_vehicle_without_repeating_the_root() {
        let root = std::env::temp_dir().join("openomsi preview root");
        let relative = "Vehicles/Preview Pack/#Bus.bus";
        let loaded_file = root.join(relative);
        let bus = repaint_bus_path(&root, &loaded_file);
        assert_eq!(
            bus, relative,
            "a loaded vehicle path must become content-relative"
        );
        assert_eq!(omsi_cfg::resolve_path(&root, &bus), loaded_file);
        assert_eq!(
            repaint_bus_path(&root, std::path::Path::new(relative)),
            relative
        );
    }

    #[test]
    fn in_game_bus_preview_repaint_preserves_exact_indices() {
        assert_eq!(repaint_scheme("livery -1", 2), Some(None));
        assert_eq!(repaint_scheme("livery 0", 2), Some(Some(0)));
        assert_eq!(repaint_scheme("livery 1", 2), Some(Some(1)));
        for action in ["livery 2", "livery invalid", "back"] {
            assert_eq!(repaint_scheme(action, 2), None);
        }
        let first = Look {
            bus: "bus".into(),
            paint_index: Some(0),
            ..Default::default()
        };
        let second = Look {
            paint_index: Some(1),
            ..first.clone()
        };
        assert_ne!(
            first, second,
            "same-named paint schemes need separate previews"
        );
    }

    #[test]
    fn in_game_bus_preview_maps_the_selected_bus_and_paint_without_changing_actions() {
        assert_eq!(
            vehicle_paint(
                &ListKind::PlaceType("maker".into()),
                "bus Vehicles/Pack/Bus.bus"
            ),
            Some(("Vehicles/Pack/Bus.bus".into(), String::new()))
        );
        assert_eq!(
            vehicle_paint(&ListKind::PlaceLivery("bus".into()), "livery Blue paint"),
            Some(("bus".into(), "Blue paint".into()))
        );
        assert_eq!(
            vehicle_paint(
                &ListKind::PlaceLivery("bus".into()),
                &format!("livery {ORIGINAL_PAINT}")
            ),
            Some(("bus".into(), String::new()))
        );
        assert_eq!(
            vehicle_paint(
                &ListKind::PlaceHof("bus".into(), "Blue paint".into()),
                "placehof Map.hof"
            ),
            Some(("bus".into(), "Blue paint".into()))
        );
        assert_eq!(vehicle_paint(&ListKind::PlaceMaker, "search"), None);
        assert_eq!(vehicle_paint(&ListKind::PlaceMaker, "maker test"), None);
        assert_eq!(
            vehicle_paint(&ListKind::PlaceLivery("bus".into()), "back"),
            None
        );
        assert!(selectable("livery "));
        assert!(selectable("placehof "));
        assert!(!selectable("maker test"));
    }
}
