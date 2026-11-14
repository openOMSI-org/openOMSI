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
mod tests {
    use super::*;

    fn v(name: &str, ty: &str, file: &str) -> (String, String, String) {
        (name.into(), ty.into(), file.into())
    }

    #[test]
    fn a_folders_types_are_one_model_with_its_versions() {
        let f = families(&[
            v(
                "Mercedes-Benz Citaro C2 K",
                "Citaro C2 K",
                "Vehicles/Citaro_C2/K.bus",
            ),
            v(
                "Mercedes-Benz Citaro C2",
                "Citaro C2",
                "Vehicles/Citaro_C2/S.bus",
            ),
            v(
                "Mercedes-Benz Citaro C2 G",
                "Citaro C2 G",
                "Vehicles/Citaro_C2/G.bus",
            ),
            v("Mercedes-Benz O305", "O305", "Vehicles/O305/O305.bus"),
        ]);
        assert_eq!(f.len(), 2);
        let citaro = f.iter().find(|x| x.name == "Citaro").unwrap();
        assert_eq!(
            citaro
                .versions
                .iter()
                .map(|x| x.0.as_str())
                .collect::<Vec<_>>(),
            ["C2", "C2 G", "C2 K"]
        );
        // (one bus alone: its whole name)
        assert!(
            f.iter()
                .any(|x| x.name == "Mercedes-Benz O305" && x.versions[0].0 == "O305")
        );
    }

    #[test]
    fn types_sharing_no_word_go_by_their_folder() {
        let f = families(&[
            v("MAN NL202", "NL202", "Vehicles/MAN_city/NL.bus"),
            v("MAN SD202", "SD202", "Vehicles/MAN_city/SD.bus"),
        ]);
        assert_eq!(f[0].name, "MAN city");
        assert_eq!(
            f[0].versions
                .iter()
                .map(|x| x.0.as_str())
                .collect::<Vec<_>>(),
            ["NL202", "SD202"]
        );
    }

    #[test]
    fn equal_labels_get_the_file_or_the_folder() {
        let row = |l: &str, a: &str| (l.to_string(), a.to_string());
        let mut rows = vec![
            row("MAN", "heading"),
            row("SD202", "bus Vehicles/MAN_SD/SD202.bus"),
            row("SD202", "bus Vehicles/MAN_SD/SD202_v2.bus"),
            row("MAN SD200", "bus Vehicles/SD200_a/SD200.bus"),
            row("MAN SD200", "bus Vehicles/SD200_b/SD200.bus"),
            row("NL202", "bus Vehicles/MAN_SD/NL202.bus"),
        ];
        tell_apart(&mut rows);
        let labels: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(
            labels,
            [
                "MAN",
                "SD202  |  SD202",
                "SD202  |  SD202 v2",
                "MAN SD200  |  SD200 a",
                "MAN SD200  |  SD200 b",
                "NL202"
            ]
        );
    }

    #[test]
    fn the_second_line_is_the_description_or_the_folder() {
        assert_eq!(
            about("\n  Standard bus of 1985\nmore", "Vehicles/SD/x.bus"),
            "Standard bus of 1985"
        );
        assert_eq!(about("", "Vehicles/MAN_SD202/SD202.bus"), "MAN SD202");
    }
}
