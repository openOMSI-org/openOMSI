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

fn page(query: &str) -> Vec<String> {
    // (the model's name is in no folder's path)
    let f = families(&[
        v(
            "Mercedes-Benz Citaro C2 K",
            "Citaro C2 K",
            "Vehicles/MB_C2/K.bus",
        ),
        v(
            "Mercedes-Benz Citaro C2",
            "Citaro C2",
            "Vehicles/MB_C2/S.bus",
        ),
        v(
            "Mercedes-Benz Citaro C2 G",
            "Citaro C2 G",
            "Vehicles/MB_C2/G.bus",
        ),
        v("Mercedes-Benz O305", "O305", "Vehicles/O305/O305.bus"),
    ]);
    rows(f, |label, action| {
        super::super::search_matches(label, action, query)
    })
    .into_iter()
    .map(|r| r.0)
    .collect()
}

#[test]
fn a_search_for_the_model_finds_its_versions() {
    assert_eq!(
        page(""),
        ["Citaro", "C2", "C2 G", "C2 K", "Mercedes-Benz O305"]
    );
    assert_eq!(page("citaro"), ["Citaro", "C2", "C2 G", "C2 K"]);
    assert_eq!(page("CITARO k"), ["Citaro", "C2 K"]);
    assert_eq!(page("o305"), ["Mercedes-Benz O305"]);
    // (no heading left without a bus under it)
    assert!(page("citaro x").is_empty());
}

#[test]
fn the_second_line_is_the_description_or_the_folder() {
    assert_eq!(
        about("\n  Standard bus of 1985\nmore", "Vehicles/SD/x.bus"),
        "Standard bus of 1985"
    );
    assert_eq!(about("", "Vehicles/MAN_SD202/SD202.bus"), "MAN SD202");
}
