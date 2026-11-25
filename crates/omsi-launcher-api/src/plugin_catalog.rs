//! The Plugins page's catalog commands (#3920); `omsi_launcher_lib::plugin_catalog` checks the files.

use crate::{Answer, err};
use omsi_launcher_lib::plugin_catalog as cat;
use serde_json::{Value, json};

/// A URL's body, at most `max` bytes.
pub(crate) fn fetch(url: &str, max: u64) -> anyhow::Result<Vec<u8>> {
    let r = omsi_platform::updater::agent()
        .get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut out = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(r.into_reader(), max + 1), &mut out)?;
    if out.len() as u64 > max {
        anyhow::bail!("{url}: larger than expected");
    }
    Ok(out)
}

fn entries() -> anyhow::Result<Vec<cat::Entry>> {
    let bytes = fetch(cat::CATALOG_URL, 4 << 20)?;
    cat::parse(&String::from_utf8_lossy(&bytes))
}

/// `{plugins: [entry + {installed, update, source}]}`, then `{id, name, installed, removed}`
/// of each plugin the catalog installed and no longer lists.
pub(crate) fn list() -> Answer {
    let list = entries().map_err(err)?;
    let dir = cat::plugins_dir().ok();
    let gone = dir.as_deref().map(|d| gone(d, &list)).unwrap_or_default();
    let plugins: Vec<Value> = list
        .into_iter()
        .map(|e| {
            let installed = dir.as_deref().and_then(|d| cat::installed(d, &e.id));
            let update = installed
                .as_deref()
                .and_then(cat::version)
                .is_some_and(|have| cat::version(&e.version).is_some_and(|new| new > have));
            let mut v = serde_json::to_value(&e).unwrap_or(Value::Null);
            v["installed"] = json!(installed);
            v["update"] = json!(update);
            v["source"] = json!(format!("{}{}", cat::SOURCE_URL, e.id));
            v
        })
        .chain(gone)
        .collect();
    Ok(json!({ "plugins": plugins }))
}

/// The plugins the catalog installed that it no longer lists (taken out of openOMSI-Plugins):
/// the page shows them so they can be removed; nothing is removed by itself, as a plugin's
/// saved data goes with it.
fn gone(plugins: &std::path::Path, list: &[cat::Entry]) -> Vec<Value> {
    let mut out: Vec<Value> = std::fs::read_dir(plugins)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let id = e.file_name().to_string_lossy().into_owned();
                    let installed = cat::installed(plugins, &id)?;
                    (!list.iter().any(|x| x.id == id)).then(|| {
                        json!({ "id": id, "name": id, "installed": installed, "removed": true,
                                "update": false, "tags": [] })
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    out
}

/// Install (or update) the catalog's plugin `id`; its version.
pub(crate) fn install(id: &str) -> Answer {
    let list = entries().map_err(err)?;
    let e = list
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("{id} is not in the plugin catalog"))?;
    let dir = cat::plugins_dir().map_err(err)?;
    cat::install(&dir, e, &|url| fetch(url, 2 << 20)).map_err(err)?;
    Ok(json!(e.version))
}

pub(crate) fn remove(id: &str) -> Answer {
    cat::remove(&cat::plugins_dir().map_err(err)?, id).map_err(err)?;
    Ok(json!(true))
}
