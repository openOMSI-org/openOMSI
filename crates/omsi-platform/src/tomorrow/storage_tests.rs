//! Exercise real private files without credentials, environment changes or HTTP.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "openbus-weather-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn cache(&self) -> PathBuf {
        cache_path_in(&self.0, "test-key")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn read(path: &Path) -> Cache {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn no_request() -> Result<ureq::Response, ureq::Error> {
    panic!("a cache hit, invalid store or exhausted budget must not send HTTP")
}
fn temperature(v: &Value) -> Result<f64, &'static str> {
    v.get("temperature")
        .and_then(Value::as_f64)
        .ok_or("response has no valid temperature")
}
fn values() -> Value {
    serde_json::json!({"temperature":12,"humidity":80,"windSpeed":6.6})
}

#[test]
fn private_key_file_is_trimmed_validated_and_overridden() {
    let dir = Directory::new();
    let path = dir.0.join("tomorrow-api-key.txt");
    assert_eq!(read_credential(None, Some(&path)), None);
    std::fs::write(&path, "  test_key-123\r\n").unwrap();
    assert_eq!(
        read_credential(None, Some(&path)).as_deref(),
        Some("test_key-123")
    );
    assert_eq!(
        read_credential(Some("override"), Some(&path)).as_deref(),
        Some("override")
    );
    // An explicitly invalid override must not silently use the stored credential.
    assert_eq!(read_credential(Some("invalid key"), Some(&path)), None);
    for invalid in ["", "\n", "key=value", "two\nkeys", &"x".repeat(512)] {
        std::fs::write(&path, invalid).unwrap();
        assert_eq!(read_credential(None, Some(&path)), None);
    }
    let cache = dir.cache();
    assert_ne!(cache, cache_path_in(&dir.0, "other-key"));
    assert!(
        !cache
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("test-key")
    );
}

#[test]
fn failed_call_reserves_budget_on_disk_before_transport_and_after_restart() {
    let dir = Directory::new();
    let path = dir.cache();
    let result = fetch_in(&path, "50,14", 900, 100000, temperature, || {
        assert_eq!(read(&path).requests, [100000]);
        Err(ureq::Error::Status(
            500,
            ureq::Response::new(500, "error", "").unwrap(),
        ))
    });
    assert_eq!(result.err(), Some("Tomorrow.io returned an HTTP error"));
    assert_eq!(read(&path).requests, [100000]);
    assert!(fetch_in(&path, "51,15", 900, 100179, temperature, no_request).is_err());
    assert!(!path.with_extension("tmp").exists());
    assert!(!path.with_extension("lock").exists());
}

#[test]
fn quota_and_auth_cooldown_survive_reopening_the_store() {
    for status in [401, 403, 429] {
        let dir = Directory::new();
        let path = dir.cache();
        assert!(
            fetch_in(&path, "50,14", 900, 100000, temperature, || {
                Err(ureq::Error::Status(
                    status,
                    ureq::Response::new(status, "error", "").unwrap(),
                ))
            })
            .is_err()
        );
        assert_eq!(read(&path).blocked_until, 103600);
        assert!(fetch_in(&path, "51,15", 900, 103599, temperature, no_request).is_err());
        assert!(read(&path).reserve(103600).is_ok());
    }
}

#[test]
fn hourly_and_daily_limits_survive_restart_and_expire() {
    let dir = Directory::new();
    let path = dir.cache();
    for (requests, time, expires) in [
        ((0..20).map(|i| 100000 + i * 180).collect(), 103599, 103600),
        ((0..450).map(|i| 100000 + i * 180).collect(), 181000, 186400),
    ] {
        let cache = Cache {
            requests,
            ..Cache::default()
        };
        write_cache(&path, &cache).unwrap();
        assert!(fetch_in(&path, "50,14", 900, time, temperature, no_request).is_err());
        assert!(read(&path).reserve(expires).is_ok());
    }
}

#[test]
fn fresh_shared_response_works_during_cooldown_without_spending_budget() {
    let dir = Directory::new();
    let path = dir.cache();
    let mut cache = Cache {
        requests: vec![100000],
        blocked_until: 103600,
        ..Cache::default()
    };
    cache.values.insert(
        "50,14".into(),
        Cached {
            at: 100000,
            values: values(),
        },
    );
    write_cache(&path, &cache).unwrap();
    for _ in 0..2 {
        let loaded = fetch_in(&path, "50,14", 900, 100100, temperature, no_request).unwrap();
        assert_eq!(loaded.observed, 12.0);
        assert_eq!(read(&path).requests, [100000]);
    }
    assert!(fetch_in(&path, "50,14", 900, 100900, temperature, no_request).is_err());
}

#[test]
fn successful_response_reopens_with_budget_and_without_credentials() {
    let dir = Directory::new();
    let path = dir.cache();
    let body = serde_json::json!({"data":{"values":values()}}).to_string();
    let loaded = fetch_in(&path, "50,14", 900, 100000, temperature, || {
        Ok(ureq::Response::new(200, "OK", &body).unwrap())
    })
    .unwrap();
    assert_eq!(loaded.observed, 12.0);
    assert_eq!(read(&path).requests, [100000]);
    assert!(!std::fs::read_to_string(&path).unwrap().contains("test-key"));
    assert!(fetch_in(&path, "50,14", 900, 100001, temperature, no_request).is_ok());
    assert!(fetch_in(&path, "51,15", 900, 100001, temperature, no_request).is_err());
    // Replacing an existing cache must keep the earlier call in the ledger.
    assert!(
        fetch_in(&path, "51,15", 900, 100180, temperature, || {
            Ok(ureq::Response::new(200, "OK", &body).unwrap())
        })
        .is_ok()
    );
    assert_eq!(read(&path).requests, [100000, 100180]);
    assert_eq!(read(&path).values.len(), 2);
}

#[test]
fn corrupt_cache_and_concurrent_request_do_not_reset_the_budget() {
    let dir = Directory::new();
    let path = dir.cache();
    std::fs::write(&path, "broken JSON").unwrap();
    assert_eq!(
        fetch_in(&path, "50,14", 900, 100000, temperature, no_request).err(),
        Some("weather cache is invalid; no request sent")
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken JSON");
    assert!(!path.with_extension("lock").exists());
    let cache = Cache {
        requests: vec![100000],
        ..Cache::default()
    };
    write_cache(&path, &cache).unwrap();
    let lock = path.with_extension("lock");
    std::fs::write(&lock, "").unwrap();
    assert_eq!(
        fetch_in(&path, "50,14", 900, 100180, temperature, no_request).err(),
        Some("another weather request is running")
    );
    assert_eq!(read(&path).requests, [100000]);
    assert!(lock.exists());
}
