//! The Plugins page's catalog (#3920): the list of openOMSI-org/openOMSI-Plugins with what is
//! installed of it, and installing, updating and removing a plugin of it (see
//! `omsi_launcher_lib::plugin_catalog`, which checks every file against the catalog).

use crate::{Answer, err};
use omsi_launcher_lib::plugin_catalog as cat;
use serde_json::{Value, json};

/// A URL's body, at most `max` bytes.
fn fetch(url: &str, max: u64) -> anyhow::Result<Vec<u8>> {
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

/// `{plugins: [entry + {installed, update, source}]}`.
pub(crate) fn list() -> Answer {
    let list = entries().map_err(err)?;
    let dir = cat::plugins_dir().ok();
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
        .collect();
    Ok(json!({ "plugins": plugins }))
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
