fn main() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    println!("cargo:rerun-if-changed={}", src.display());
    let mut files: Vec<_> = std::fs::read_dir(&src).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "rs")).collect();
    files.sort();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |b: &[u8]| {
        for &x in b {
            h = (h ^ x as u64).wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(std::env::var("TARGET").unwrap_or_default().as_bytes());
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let lock = manifest.join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    println!("cargo:rerun-if-changed={}", manifest.join("Cargo.toml").display());
    feed(&std::fs::read(manifest.join("Cargo.toml")).unwrap_or_default());
    // Only the published crates this one is built from (the pictures' decoders and what they
    // use): with every package of the workspace's lock in it, a new TLS or voice crate had
    // every texture compressed once more after an update (0.3.36, 0.3.40)
    for package in texture_packages(&std::fs::read_to_string(&lock).unwrap_or_default()) {
        feed(package.as_bytes());
    }
    // (the tests' own files change nothing a texture comes out as)
    for f in files.iter().filter(|f| !f.file_name().unwrap().to_string_lossy().ends_with("_tests.rs")) {
        feed(f.file_name().unwrap().as_encoded_bytes());
        feed(&std::fs::read(f).unwrap());
    }
    println!("cargo:rustc-env=TEXTURE_SOURCE_HASH={h:016x}");
}

/// The lock's entries of the published packages `omsi-texture` depends on, directly or not,
/// in the lock's order. The workspace's own crates are passed over, and what only they
/// depend on with them: `omsi-cfg` reads the files, whose bytes the cache key holds anyway.
fn texture_packages(lock: &str) -> Vec<&str> {
    // name, version, the whole entry, its dependencies ("name" or "name version")
    let entries: Vec<(&str, &str, &str, Vec<&str>)> = lock
        .split("[[package]]")
        .skip(1)
        .filter_map(|p| {
            let field = |key: &str| p.lines().find_map(|l| l.strip_prefix(key)).map(|v| v.trim_matches('"'));
            let deps = p
                .split_once("dependencies = [")
                .map(|(_, rest)| rest.split(']').next().unwrap_or("").lines().map(|l| l.trim().trim_end_matches(',').trim_matches('"')).filter(|d| !d.is_empty()).collect())
                .unwrap_or_default();
            Some((field("name = ")?, field("version = ")?, p, deps))
        })
        .collect();
    let mut wanted = vec![false; entries.len()];
    let mut stack: Vec<&str> = entries.iter().filter(|e| e.0 == "omsi-texture").flat_map(|e| e.3.iter().copied()).collect();
    while let Some(dep) = stack.pop() {
        let (name, version) = dep.split_once(' ').map_or((dep, None), |(n, v)| (n, Some(v)));
        for (i, e) in entries.iter().enumerate() {
            if wanted[i] || e.0 != name || version.is_some_and(|v| e.1 != v) || !e.2.contains("\nsource = ") {
                continue;
            }
            wanted[i] = true;
            stack.extend(e.3.iter().copied());
        }
    }
    entries.iter().zip(wanted).filter(|(_, w)| *w).map(|(e, _)| e.2).collect()
}
