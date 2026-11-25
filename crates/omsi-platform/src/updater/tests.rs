use super::*;

#[test]
fn versions_compare_by_number() {
    assert!(newer("0.1.8", "0.1.7"));
    assert!(newer("v0.1.10", "0.1.9"));
    assert!(newer("0.2.0", "0.1.99"));
    assert!(newer("1.0", "0.9.9"));
    assert!(!newer("0.1.7", "0.1.7"));
    assert!(!newer("v0.1.6", "0.1.7"));
    assert!(!newer("garbage", "0.1.7"));
}

#[test]
fn the_platform_file_of_a_github_release() {
    let name = asset_name("0.1.9").unwrap();
    let v = serde_json::json!({
        "tag_name": "v0.1.9", "html_url": "https://github.com/openOMSI-org/openOMSI/releases/tag/v0.1.9", "body": "notes",
        "assets": [
            {"name": "openOMSI-0.1.9-server-linux-x64.zip", "browser_download_url": "https://x/server", "size": 5},
            {"name": name, "browser_download_url": "https://x/mine", "size": 42, "digest": "sha256:ABCDEF"}
        ]
    });
    let r = parse_release(&v, "0.1.7", false).unwrap().unwrap();
    assert_eq!(
        (
            r.version.as_str(),
            r.asset_url.as_str(),
            r.size,
            r.sha256.as_deref()
        ),
        ("0.1.9", "https://x/mine", 42, Some("abcdef"))
    );
    // not newer, a draft, or without this platform's file: nothing to offer
    assert!(parse_release(&v, "0.1.9", false).unwrap().is_none());
    let mut d = v.clone();
    d["draft"] = serde_json::json!(true);
    assert!(parse_release(&d, "0.1.7", false).unwrap().is_none());
    let mut n = v.clone();
    n["assets"] = serde_json::json!([]);
    assert!(parse_release(&n, "0.1.7", false).unwrap().is_none());
    // a pull request's test build keeps itself, however new the release
    assert!(parse_release(&v, "0.1.7-pr12", false).unwrap().is_none());
    assert!(is_test_build("0.1.1313-pr1192") && !is_test_build("0.1.1313"));
}

#[test]
fn the_releases_to_choose_from_newest_first() {
    let release = |tag: &str, date: &str| {
        let version = tag.trim_start_matches('v');
        serde_json::json!({
            "tag_name": tag, "published_at": date, "html_url": format!("https://x/{tag}"),
            "assets": [{"name": asset_name(version).unwrap(), "browser_download_url": format!("https://x/{tag}.zip"), "size": 7}]
        })
    };
    let mut draft = release("v0.2.40", "2026-10-13T01:00:00Z");
    draft["draft"] = serde_json::json!(true);
    let mut pre = release("v0.2.39", "2026-10-13T00:00:00Z");
    pre["prerelease"] = serde_json::json!(true);
    let mut bare = release("v0.2.35", "2026-10-11T23:00:00Z");
    bare["assets"] = serde_json::json!([]);
    let v = serde_json::json!([
        release("v0.2.33", "2026-10-11T20:15:13Z"),
        release("v0.2.38", "2026-10-12T13:55:06Z"),
        draft,
        pre,
        bare,
        release("v0.2.31", "2026-10-11T19:00:00Z"),
        release("v0.2.10", "2026-10-10T19:00:00Z"),
        release("v0.2.32", "2026-10-11T20:00:00Z"),
        release("v0.2.37", "2026-10-12T04:12:02Z"),
        release("v0.2.37", "2026-10-12T04:12:02Z"),
        {"tag_name": "v0.2.36-pr12", "assets": []},
        {"name": "no tag"}
    ]);
    let list = releases_of(&v, false);
    let versions: Vec<&str> = list.iter().map(|l| l.release.version.as_str()).collect();
    assert_eq!(versions, ["0.2.38", "0.2.37", "0.2.33", "0.2.32"]);
    assert_eq!(list[0].published, "2026-10-12");
    assert_eq!(
        (list[0].release.asset_url.as_str(), list[0].release.size),
        ("https://x/v0.2.38.zip", 7)
    );
    assert!(releases_of(&serde_json::json!({"message": "rate limited"}), false).is_empty());
}

#[test]
fn stable_releases_behind_a_page_of_betas_are_found() {
    let release_of_minor = |minor: u32, n: u32, prerelease: bool| {
        let tag = format!("v0.{minor}.{n}");
        serde_json::json!({
            "tag_name": tag, "prerelease": prerelease, "published_at": "2026-10-20T00:00:00Z",
            "assets": [{"name": asset_name(&tag[1..]).unwrap(), "browser_download_url": format!("https://x/{tag}.zip"), "size": 7}]
        })
    };
    let release = |n: u32, prerelease: bool| release_of_minor(3, n, prerelease);
    let pages = std::cell::Cell::new(0);
    let fetch = |page: u32| {
        pages.set(pages.get() + 1);
        let list: Vec<_> = match page {
            1 => (100..200).rev().map(|n| release(n, n != 150)).collect(),
            2 => (40..100).rev().map(|n| release(n, n % 10 != 0)).collect(),
            _ => panic!("page {page} asked after a short page"),
        };
        Ok(serde_json::Value::Array(list).to_string())
    };
    let versions: Vec<String> = releases_paged(fetch, false)
        .unwrap()
        .into_iter()
        .map(|l| l.release.version)
        .collect();
    assert_eq!(
        versions,
        [
            "0.3.150", "0.3.90", "0.3.80", "0.3.70", "0.3.60", "0.3.50", "0.3.40"
        ]
    );
    assert_eq!(pages.get(), 2);

    let rate_limited = |page: u32| {
        if page == 1 {
            Ok(r#"{"message": "rate limited"}"#.to_string())
        } else {
            anyhow::bail!("offline")
        }
    };
    assert!(releases_paged(rate_limited, false).unwrap().is_empty());
    let page_two_fails = |page: u32| {
        if page == 1 {
            Ok(
                serde_json::Value::Array((0..100).map(|n| release(n, n != 7)).collect())
                    .to_string(),
            )
        } else {
            anyhow::bail!("offline")
        }
    };
    assert_eq!(releases_paged(page_two_fails, false).unwrap().len(), 1);
    assert!(releases_paged(|_| anyhow::bail!("offline"), false).is_err());

    let pages = std::cell::Cell::new(0);
    let reaches_unlisted = |page: u32| {
        pages.set(pages.get() + 1);
        let list: Vec<_> = match page {
            1 => (100..200).rev().map(|n| release(n, true)).collect(),
            2 => (0..100)
                .rev()
                .map(|n| release_of_minor(2, n, false))
                .collect(),
            _ => panic!("page {page} asked after releases too old to list"),
        };
        Ok(serde_json::Value::Array(list).to_string())
    };
    let found = releases_paged(reaches_unlisted, false).unwrap();
    assert_eq!(
        (
            found.first().map(|l| l.release.version.as_str()),
            found.len()
        ),
        (Some("0.2.99"), 68)
    );
    assert_eq!(pages.get(), 2);
}

#[test]
fn the_beta_channel_takes_the_newest_release_betas_included() {
    let release = |tag: &str, prerelease: bool, draft: bool| {
        let version = tag.trim_start_matches('v');
        let asset = asset_name(version).unwrap_or_default();
        serde_json::json!({
            "tag_name": tag, "prerelease": prerelease, "draft": draft, "published_at": "2026-10-20T00:00:00Z",
            "assets": [{"name": asset, "browser_download_url": format!("https://x/{tag}.zip"), "size": 7}]
        })
    };
    let beta = release("v0.3.7", true, false);
    assert!(parse_release(&beta, "0.3.5", false).unwrap().is_none());
    assert_eq!(
        parse_release(&beta, "0.3.5", true)
            .unwrap()
            .map(|r| r.version),
        Some("0.3.7".into())
    );

    let list = serde_json::json!([
        release("v0.3.9", false, true),
        {"tag_name": "avatars-1", "prerelease": true, "assets": []},
        release("v0.3.7", true, false),
        release("v0.3.5", false, false),
        release("v0.3.6", true, false),
    ]);
    assert_eq!(
        newest_of(&list, "0.3.5", &[]).unwrap().map(|r| r.version),
        Some("0.3.7".into())
    );
    assert!(newest_of(&list, "0.3.7", &[]).unwrap().is_none());
    assert!(newest_of(&list, "0.3.8", &[]).unwrap().is_none());
    assert!(
        newest_of(
            &serde_json::json!({"message": "rate limited"}),
            "0.3.5",
            &[]
        )
        .is_err()
    );
    assert_eq!(
        newest_of(&list, "0.3.4", &["0.3.7".into()])
            .unwrap()
            .map(|r| r.version),
        Some("0.3.6".into())
    );
    let mut one_platform = release("v0.3.8", true, false);
    one_platform["assets"] = serde_json::json!([{"name": "openOMSI-0.3.8-some-other-platform.zip", "browser_download_url": "https://x/o.zip", "size": 7}]);
    let with_one_platform = serde_json::json!([
        one_platform,
        release("v0.3.7", true, false),
        release("v0.3.5", false, false)
    ]);
    assert_eq!(
        newest_of(&with_one_platform, "0.3.5", &[])
            .unwrap()
            .map(|r| r.version),
        Some("0.3.7".into())
    );

    let stable_only: Vec<(String, bool)> = releases_of(&list, false)
        .into_iter()
        .map(|l| (l.release.version, l.beta))
        .collect();
    assert_eq!(stable_only, [("0.3.5".to_string(), false)]);
    let with_betas: Vec<(String, bool)> = releases_of(&list, true)
        .into_iter()
        .map(|l| (l.release.version, l.beta))
        .collect();
    assert_eq!(
        with_betas,
        [
            ("0.3.7".to_string(), true),
            ("0.3.6".to_string(), true),
            ("0.3.5".to_string(), false)
        ]
    );
}

#[test]
fn a_look_waits_for_the_one_running_unless_the_channel_changed() {
    let r = Release {
        version: "0.3.7".into(),
        page: String::new(),
        notes: String::new(),
        asset_name: String::new(),
        asset_url: String::new(),
        size: 0,
        sha256: None,
    };
    for switched in [false, true] {
        assert!(check_waits(
            &Status::Downloading {
                release: r.clone(),
                done: 0,
                total: 1
            },
            switched
        ));
        assert!(check_waits(&Status::Installing(r.clone()), switched));
        assert!(!check_waits(&Status::Idle, switched));
        assert!(!check_waits(&Status::UpToDate, switched));
        assert!(!check_waits(&Status::Available(r.clone()), switched));
    }
    assert!(check_waits(&Status::Checking, false));
    assert!(check_waits(&Status::Downloaded(r.clone()), false));
    assert!(!check_waits(&Status::Checking, true));
    assert!(!check_waits(&Status::Downloaded(r.clone()), true));

    let status = Mutex::new(Status::Idle);
    let generation = std::sync::atomic::AtomicU64::new(0);
    let old = start_check(&status, &generation);
    let new = start_check(&status, &generation);
    assert!(!finish_check(
        &status,
        &generation,
        old,
        Status::Available(r.clone())
    ));
    assert_eq!(lock(&status).clone(), Status::Checking);
    assert!(finish_check(&status, &generation, new, Status::UpToDate));
    assert_eq!(lock(&status).clone(), Status::UpToDate);
    assert!(!finish_check(
        &status,
        &generation,
        new,
        Status::Available(r)
    ));
    assert_eq!(lock(&status).clone(), Status::UpToDate);
}

#[test]
fn versions_are_ordered_by_number() {
    assert!(compare("0.2.10", "0.2.9").is_gt());
    assert!(compare("v0.2.32", "0.2.32").is_eq());
    assert!(compare("0.2", "0.2.0").is_eq());
    assert!(compare("0.1.800", "0.2.0").is_lt());
}

/// A server on this computer that cuts the first answer off half way (as a connection
/// that times out does) and then answers range requests: the download goes on where it
/// stopped, the file comes out whole and its SHA-256 checks.
#[test]
fn a_broken_download_goes_on_where_it_stopped() {
    use sha2::Digest;
    use std::io::{BufRead, BufReader};
    let data: Vec<u8> = (0..600_000u32).map(|i| (i * 7 + i / 13) as u8).collect();
    let sha: String = sha2::Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let served = data.clone();
    let ranges = Arc::new(Mutex::new(Vec::<u64>::new()));
    let seen = ranges.clone();
    std::thread::spawn(move || {
        for (n, conn) in listener.incoming().enumerate() {
            let Ok(mut c) = conn else { continue };
            let mut from = 0u64;
            let mut reader = BufReader::new(c.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(r) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                    from = r.trim().trim_end_matches('-').parse().unwrap();
                }
            }
            seen.lock().unwrap().push(from);
            let rest = &served[from as usize..];
            let head = if from > 0 {
                format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{}\r\nConnection: close\r\n\r\n",
                    rest.len(),
                    served.len() - 1,
                    served.len()
                )
            } else {
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    rest.len()
                )
            };
            let _ = c.write_all(head.as_bytes());
            // (the first answer breaks off at 40 %)
            let send = if n == 0 {
                &rest[..rest.len() * 2 / 5]
            } else {
                rest
            };
            let _ = c.write_all(send);
        }
    });
    let r = Release {
        version: "9.9.9".into(),
        page: String::new(),
        notes: String::new(),
        asset_name: format!("openOMSI-test-{}.zip", std::process::id()),
        asset_url: format!("http://127.0.0.1:{port}/file.zip"),
        size: data.len() as u64,
        sha256: Some(sha),
    };
    let to = std::env::temp_dir().join(&r.asset_name);
    let _ = std::fs::remove_file(&to);
    let status = Mutex::new(Status::Idle);
    download(&r, &to, &status).unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), data);
    let r1 = ranges.lock().unwrap().clone();
    assert_eq!(r1.len(), 2, "one break, one range request: {r1:?}");
    assert_eq!(r1[1], (data.len() * 2 / 5) as u64);
    let _ = std::fs::remove_file(&to);
    let _ = std::fs::remove_file(checked_note(&to));
}

#[test]
fn the_program_path_survives_the_swap() {
    assert_eq!(
        program_path(Path::new("/home/me/openOMSI/openomsi")),
        PathBuf::from("/home/me/openOMSI/openomsi")
    );
    assert_eq!(
        program_path(Path::new("/home/me/openOMSI/openomsi.old-update")),
        PathBuf::from("/home/me/openOMSI/openomsi")
    );
    assert_eq!(
        program_path(Path::new("/home/me/openOMSI/openomsi.old-update (deleted)")),
        PathBuf::from("/home/me/openOMSI/openomsi")
    );
    assert_eq!(
        program_path(Path::new("C:\\Games\\openOMSI\\openomsi.exe.old-update")),
        PathBuf::from("C:\\Games\\openOMSI\\openomsi.exe")
    );
    assert_eq!(
        program_path(Path::new("C:\\Games\\openOMSI\\openomsi.exe.2.old-update")),
        PathBuf::from("C:\\Games\\openOMSI\\openomsi.exe")
    );
}

#[test]
fn development_builds_are_recognised() {
    assert!(is_dev_build(Path::new(
        "/src/openOMSI/target/release/openomsi"
    )));
    assert!(is_dev_build(Path::new(
        "C:\\src\\target\\x86_64-pc-windows-msvc\\release\\openomsi.exe"
    )));
    assert!(!is_dev_build(Path::new(
        "/Applications/openOMSI.app/Contents/MacOS/openomsi"
    )));
    assert!(!is_dev_build(Path::new("/home/me/Games/openOMSI/openomsi")));
    let dir = std::env::temp_dir().join(format!("openomsi-dev-build-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("builds/release")).unwrap();
    std::fs::write(dir.join("builds/CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55\n# This file is a cache directory tag created by cargo.\n").unwrap();
    assert!(is_dev_build(&dir.join("builds/release/openomsi")));
    let _ = std::fs::remove_dir_all(dir);
}

/// The whole swap on a folder: the program files replaced, a file only the old version
/// had removed, the player's things left alone, the old files set aside.
#[test]
fn an_archive_replaces_the_program_and_nothing_else() {
    let root = std::env::temp_dir().join(format!("omsi_update_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("install");
    std::fs::create_dir_all(dir.join("Vehicles/MyMod")).unwrap();
    std::fs::write(dir.join("openomsi"), "old game").unwrap();
    std::fs::write(dir.join("openomsi-launcher"), "old cli").unwrap();
    std::fs::write(dir.join("retired.dll"), "old").unwrap();
    std::fs::write(dir.join("Vehicles/MyMod/bus.bus"), "mod").unwrap();
    std::fs::write(
        dir.join(MANIFEST),
        "openomsi\nopenomsi-launcher\nretired.dll\n",
    )
    .unwrap();
    // the new release
    let zip_path = root.join("new.zip");
    {
        let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let o = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        for (n, body) in [
            ("openomsi", "new game"),
            ("openomsi-launcher", "new cli"),
            ("README.md", "readme"),
        ] {
            z.start_file(n, o).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    let place = Place {
        dir: dir.clone(),
        bundle: None,
        exe: dir.join("openomsi"),
    };
    install_archive_with(&zip_path, &place, false).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("openomsi")).unwrap(),
        "new game"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("openomsi-launcher")).unwrap(),
        "new cli"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("README.md")).unwrap(),
        "readme"
    );
    assert!(!dir.join("retired.dll").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("Vehicles/MyMod/bus.bus")).unwrap(),
        "mod"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("openomsi.old-update")).unwrap(),
        "old game"
    );
    assert!(!dir.join(STAGING).exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(dir.join("openomsi"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    // an archive without a program replaces nothing
    let bad = root.join("bad.zip");
    {
        let mut z = zip::ZipWriter::new(std::fs::File::create(&bad).unwrap());
        z.start_file("notes.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"x").unwrap();
        z.finish().unwrap();
    }
    assert!(install_archive_with(&bad, &place, false).is_err());
    assert_eq!(
        std::fs::read_to_string(dir.join("openomsi")).unwrap(),
        "new game"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_app_bundle_update_leaves_the_applications_folder_alone() {
    let root = std::env::temp_dir().join(format!("omsi_update_app_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("Applications");
    let bundle = dir.join("openOMSI.app");
    std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    std::fs::write(bundle.join("Contents/MacOS/openomsi"), "old game").unwrap();
    std::fs::write(dir.join("Other.txt"), "theirs").unwrap();
    let zip_path = root.join("new.zip");
    {
        let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        let o = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
        for (n, body) in [
            ("openOMSI.app/Contents/MacOS/openomsi", "new game"),
            ("README.md", "readme"),
            ("LICENSE", "license"),
        ] {
            z.start_file(n, o).unwrap();
            z.write_all(body.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }
    let place = Place {
        dir: dir.clone(),
        bundle: Some(bundle.clone()),
        exe: bundle.join("Contents/MacOS/openomsi"),
    };
    install_archive_with(&zip_path, &place, false).unwrap();
    assert_eq!(
        std::fs::read_to_string(bundle.join("Contents/MacOS/openomsi")).unwrap(),
        "new game"
    );
    assert!(!dir.join("README.md").exists());
    assert!(!dir.join("LICENSE").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join("Other.txt")).unwrap(),
        "theirs"
    );
    assert!(!dir.join(STAGING).exists());
    let _ = std::fs::remove_dir_all(&root);
}
