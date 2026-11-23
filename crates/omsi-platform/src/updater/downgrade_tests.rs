use super::*;

#[test]
fn stable_can_downgrade_only_a_listed_broken_build_to_a_verified_release() {
    let stable = serde_json::json!({
        "tag_name": "v0.3.52", "draft": false, "prerelease": false,
        "assets": [{
            "name": asset_name("0.3.52").unwrap(),
            "browser_download_url": "https://github.com/openOMSI-org/openOMSI/releases/download/v0.3.52/package.zip",
            "size": 42, "digest": format!("sha256:{}", "a".repeat(64))
        }]
    });
    let broken = ["0.3.56".to_string(), "0.3.56-pr12".to_string()];
    let selected = |release: &serde_json::Value, current: &str, broken: &[String], allowed| {
        stable_release(release, current, broken, allowed, |_| false)
            .unwrap()
            .map(|r| r.version)
    };
    assert_eq!(
        selected(&stable, "0.3.56", &broken, true),
        Some("0.3.52".into())
    );
    assert_eq!(
        selected(&stable, "0.3.51", &broken, true),
        Some("0.3.52".into())
    );
    assert_eq!(selected(&stable, "0.3.56", &[], true), None);
    assert_eq!(selected(&stable, "0.3.56", &broken, false), None);
    assert_eq!(selected(&stable, "0.3.56-pr12", &broken, true), None);
    assert_eq!(selected(&stable, "0.3.52", &broken, true), None);
    for current in ["0.3.51", "0.3.56"] {
        assert!(
            stable_release(&stable, current, &broken, true, |v| v == "0.3.52")
                .unwrap()
                .is_none()
        );
    }
    let both_broken = ["0.3.56".to_string(), "0.3.52".to_string()];
    assert_eq!(selected(&stable, "0.3.56", &both_broken, true), None);
    assert_eq!(selected(&stable, "0.3.51", &both_broken, true), None);
    let mut unverified = stable.clone();
    unverified["assets"][0]["digest"] = serde_json::Value::Null;
    assert_eq!(selected(&unverified, "0.3.56", &broken, true), None);
    unverified["assets"][0]["digest"] = serde_json::json!("sha256:1234");
    assert_eq!(selected(&unverified, "0.3.56", &broken, true), None);
    let mut draft = stable.clone();
    draft["draft"] = serde_json::json!(true);
    assert_eq!(selected(&draft, "0.3.56", &broken, true), None);
    let mut beta = stable;
    beta["prerelease"] = serde_json::json!(true);
    assert_eq!(selected(&beta, "0.3.56", &broken, true), None);
}
