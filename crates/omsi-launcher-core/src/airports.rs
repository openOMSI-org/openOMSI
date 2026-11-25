//! The airport whose METAR report is a map's current weather.

/// The ICAO code of the airport nearest the map (by its `timezone.txt`), whose METAR report
/// is the map's current weather; Berlin's where the map does not say where it lies.
pub fn nearest_airport(root: &str, map: &str) -> String {
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> =
        std::sync::Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(Default::default);
    if let Some(c) = cache.get(map) {
        return c.clone();
    }
    // (the airports of the list round Europe and a little beyond, where OMSI's maps lie)
    const AIRPORTS: &[(&str, f64, f64)] = &[
        ("BKPR", 42.57, 21.04),
        ("EBBR", 50.90, 4.48),
        ("EDDH", 53.63, 9.99),
        ("EDDM", 48.35, 11.79),
        ("EDDN", 49.50, 11.08),
        ("EDDP", 51.42, 12.24),
        ("EDDS", 48.69, 9.22),
        ("EDDB", 52.37, 13.52),
        ("EDOP", 53.43, 11.78),
        ("EETN", 59.41, 24.83),
        ("EFHF", 60.25, 25.04),
        ("EGAA", 54.66, -6.22),
        ("EGCC", 53.35, -2.27),
        ("EGLL", 51.47, -0.45),
        ("EGPH", 55.95, -3.37),
        ("EHAM", 52.31, 4.76),
        ("EIDW", 53.42, -6.27),
        ("EKBI", 55.74, 9.15),
        ("ELLX", 49.63, 6.21),
        ("ENBR", 60.29, 5.22),
        ("ENGM", 60.19, 11.10),
        ("ESKN", 58.79, 16.91),
        ("EPKK", 50.08, 19.78),
        ("EPWA", 52.17, 20.97),
        ("EVRA", 56.92, 23.97),
        ("EYVI", 54.63, 25.29),
        ("LBSF", 42.70, 23.41),
        ("LDZA", 45.74, 16.07),
        ("LEAL", 38.28, -0.56),
        ("LEBB", 43.30, -2.91),
        ("LEMD", 40.47, -3.56),
        ("LEPA", 39.55, 2.74),
        ("LFBD", 44.83, -0.72),
        ("LFLY", 45.73, 5.08),
        ("LFMD", 43.54, 6.95),
        ("LFPG", 49.01, 2.55),
        ("LGAV", 37.94, 23.94),
        ("LHBP", 47.44, 19.26),
        ("LICJ", 38.18, 13.10),
        ("LIMC", 45.63, 8.72),
        ("LIRA", 41.80, 12.59),
        ("LOWI", 47.26, 11.34),
        ("LOWL", 48.23, 14.19),
        ("LOWW", 48.11, 16.57),
        ("LPPT", 38.78, -9.14),
        ("LQSA", 43.82, 18.33),
        ("LSGG", 46.24, 6.11),
        ("LSZH", 47.46, 8.55),
        ("LTAC", 40.13, 32.99),
        ("LWSK", 41.96, 21.62),
        ("LYBE", 44.82, 20.31),
        ("LYTV", 42.40, 18.72),
        ("LZIB", 48.17, 17.21),
        ("UKKK", 50.40, 30.45),
        ("ULLI", 59.80, 30.26),
        ("UMKK", 54.89, 20.59),
        ("UMMM", 53.88, 28.03),
        ("UUEE", 55.97, 37.41),
        ("USSS", 56.74, 60.80),
    ];
    let dir = std::path::Path::new(map)
        .parent()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default();
    let tz = omsi_cfg::resolve_path(std::path::Path::new(root), &format!("{dir}/timezone.txt"));
    let code = omsi_map::TimeZone::load(&tz)
        .ok()
        .and_then(|t| t.lat_lon())
        .and_then(|(lat, lon)| {
            AIRPORTS
                .iter()
                .map(|(c, a, o)| {
                    (
                        c,
                        (a - lat).powi(2) + ((o - lon) * lat.to_radians().cos()).powi(2),
                    )
                })
                .min_by(|x, y| x.1.total_cmp(&y.1))
                .map(|x| x.0.to_string())
        })
        .unwrap_or_else(|| "EDDB".into());
    cache.insert(map.to_string(), code.clone());
    code
}
