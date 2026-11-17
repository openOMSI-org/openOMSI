//! "Place a vehicle..." names, grouped as `families` in `launcher/ui/js/pages/bus-picker.js`.

/// One model: its name and its versions (label, bus file).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Family {
    pub(crate) name: String,
    pub(crate) versions: Vec<(String, String)>,
}

fn words(x: &str) -> Vec<String> {
    x.replace('_', " ")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn has_alnum(w: &str) -> bool {
    w.chars().any(char::is_alphanumeric)
}

/// `vehicles`: (name as the launcher gives it - manufacturer and type -, type, bus file) of
/// one maker's vehicles.
pub(crate) fn families(vehicles: &[(String, String, String)]) -> Vec<Family> {
    let mut by: Vec<(String, Vec<&(String, String, String)>)> = Vec::new();
    for v in vehicles {
        let file = v.2.replace('\\', "/").to_lowercase();
        let dir = file
            .rsplit_once('/')
            .map(|x| x.0.to_string())
            .unwrap_or_default();
        match by.iter_mut().find(|b| b.0 == dir) {
            Some(b) => b.1.push(v),
            None => by.push((dir, vec![v])),
        }
    }
    let mut out: Vec<Family> = by
        .into_iter()
        .map(|(_, vs)| {
            let types: Vec<Vec<String>> = vs
                .iter()
                .map(|v| words(if v.1.trim().is_empty() { &v.0 } else { &v.1 }))
                .collect();
            let mut n = 0;
            while types
                .iter()
                .all(|w| n < w.len() && w[n].to_lowercase() == types[0][n].to_lowercase())
            {
                n += 1;
            }
            if vs.len() > 1 && types.iter().any(|w| w.len() == n) {
                n = n.saturating_sub(1);
            }
            while n > 0 && !has_alnum(&types[0][n - 1]) {
                n -= 1;
            }
            let folder = || {
                vs[0]
                    .2
                    .replace('\\', "/")
                    .rsplit('/')
                    .nth(1)
                    .unwrap_or_default()
                    .replace('_', " ")
            };
            let name = if vs.len() == 1 {
                vs[0].0.clone()
            } else if n > 0 {
                types[0][..n].join(" ")
            } else {
                folder()
            };
            let rest = |w: &[String]| {
                let mut i = n;
                while i + 1 < w.len() && !has_alnum(&w[i]) {
                    i += 1;
                }
                w[i.min(w.len())..].join(" ")
            };
            let mut versions: Vec<(String, String)> = vs
                .iter()
                .zip(&types)
                .map(|(v, w)| {
                    let label = if vs.len() > 1 && n > 0 {
                        rest(w)
                    } else if v.1.trim().is_empty() {
                        v.0.clone()
                    } else {
                        v.1.clone()
                    };
                    (label, v.2.clone())
                })
                .collect();
            versions.sort_by(|a, b| super::bus_cmp(&a.0, &b.0));
            Family { name, versions }
        })
        .collect();
    out.sort_by(|a, b| super::bus_cmp(&a.name, &b.name));
    out
}

/// One maker's page: a model of several versions as a heading over them, a model of one as
/// its bus's row. `found` (label, action) is the search: a version is matched with its model's
/// name before its own label, and a heading stays while one of its versions does.
pub(crate) fn rows(
    families: Vec<Family>,
    found: impl Fn(&str, &str) -> bool,
) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    // (each row's family, and the model's name its search starts with: none on a heading)
    let mut model: Vec<(usize, Option<String>)> = Vec::new();
    for (i, f) in families.into_iter().enumerate() {
        if f.versions.len() > 1 {
            rows.push((f.name.clone(), super::HEADING.to_string()));
            model.push((i, None));
            for (label, bus) in f.versions {
                rows.push((label, format!("bus {bus}")));
                model.push((i, Some(f.name.clone())));
            }
        } else if let Some((_, bus)) = f.versions.into_iter().next() {
            rows.push((super::bus_label(&f.name), format!("bus {bus}")));
            model.push((i, Some(String::new())));
        }
    }
    tell_apart(&mut rows);
    let shown: Vec<bool> = rows
        .iter()
        .zip(&model)
        .map(|(r, (_, m))| {
            m.as_ref()
                .is_some_and(|m| found(&format!("{m} {}", r.0), &r.1))
        })
        .collect();
    let family_shown = |i: usize| {
        model
            .iter()
            .zip(&shown)
            .any(|((j, m), s)| *j == i && m.is_some() && *s)
    };
    rows.into_iter()
        .zip(&model)
        .zip(&shown)
        .filter(|((_, (i, m)), s)| **s || (m.is_none() && family_shown(*i)))
        .map(|((r, _), _)| r)
        .collect()
}

/// What the picture's second line says of a bus: the first line of its description (its
/// `[description]`), else its folder - not its file's path.
pub(crate) fn about(description: &str, bus: &str) -> String {
    let line = description
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    if !line.is_empty() {
        let mut s: String = line.chars().take(140).collect();
        if line.chars().count() > 140 {
            s.push_str("...");
        }
        return s;
    }
    bus.replace('\\', "/")
        .rsplit('/')
        .nth(1)
        .unwrap_or(bus)
        .replace('_', " ")
}

/// Bus rows (`bus <file>`) of one page with the same label get the file's stem, or its folder
/// when the stems are the same too: only the picture tells them apart otherwise.
pub(crate) fn tell_apart(rows: &mut [(String, String)]) {
    let part = |b: &str, n: usize| b.rsplit('/').nth(n).unwrap_or_default().to_string();
    let keys: Vec<Option<(String, String, String)>> = rows
        .iter()
        .map(|r| {
            let b = r.1.strip_prefix("bus ")?.replace('\\', "/");
            let file = part(&b, 0);
            let stem = file
                .rsplit_once('.')
                .map_or(file.as_str(), |x| x.0)
                .to_string();
            Some((r.0.to_lowercase(), stem, part(&b, 1)))
        })
        .collect();
    for (r, key) in rows.iter_mut().zip(&keys) {
        let Some((label, stem, folder)) = key else {
            continue;
        };
        let twins: Vec<_> = keys.iter().flatten().filter(|k| k.0 == *label).collect();
        if twins.len() < 2 {
            continue;
        }
        let stem_alone = twins
            .iter()
            .filter(|k| k.1.eq_ignore_ascii_case(stem))
            .count()
            == 1;
        let suffix = if stem_alone { stem } else { folder };
        r.0 = format!("{}  |  {}", r.0, super::bus_label(suffix));
    }
}

#[cfg(test)]
#[path = "place_names_tests.rs"]
mod tests;
