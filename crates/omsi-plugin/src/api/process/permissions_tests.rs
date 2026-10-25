use super::*;

fn copy_request(request: &StartRequest) -> StartRequest {
    StartRequest {
        permissions: request.permissions.clone(),
        plugin: request.plugin.clone(),
        grants: request.grants.clone(),
        entry: request.entry.clone(),
        folder: request.folder.clone(),
        file: request.file.clone(),
        args: request.args.clone(),
        cwd: request.cwd.clone(),
        env: request.env.clone(),
    }
}

fn fixture() -> (PathBuf, StartRequest, ProcessPermissions) {
    let root = std::env::temp_dir().join(format!(
        "omsi-denial-{}-{}",
        std::process::id(),
        crate::lua::next_owner()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let entry = root.join("main.lua");
    std::fs::write(&entry, "return 1").unwrap();
    std::fs::write(root.join("helper.exe"), "helper").unwrap();
    let request = StartRequest {
        permissions: None,
        plugin: "test".into(),
        grants: Arc::new(ProcessGrants::default()),
        entry,
        folder: Some(root.clone()),
        file: "helper.exe".into(),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
    };
    (root, request, ProcessPermissions::new(None))
}

#[test]
fn deny_immediately_covers_queued_requests_with_different_content() {
    let (root, request, permissions) = fixture();
    let Authorization::Pending(_, first) = permissions.authorize(&request).unwrap() else {
        panic!("expected prompt");
    };
    let id = permissions.prompt().unwrap().id;
    std::fs::write(&request.entry, "return 2").unwrap();
    let Authorization::Pending(_, second) = permissions.authorize(&request).unwrap() else {
        panic!("expected prompt");
    };
    std::fs::create_dir_all(root.join("data")).unwrap();
    std::fs::write(root.join("data/renamed.exe"), "another helper").unwrap();
    let mut renamed = copy_request(&request);
    renamed.file = "data/renamed.exe".into();
    let Authorization::Pending(_, third) = permissions.authorize(&renamed).unwrap() else {
        panic!("expected prompt for another helper");
    };
    assert!(permissions.respond(id, ProcessDecision::Deny));
    assert_eq!(first.load(Ordering::Acquire), ProcessDecision::Deny as u8);
    assert_eq!(second.load(Ordering::Acquire), ProcessDecision::Deny as u8);
    assert_eq!(third.load(Ordering::Acquire), ProcessDecision::Deny as u8);
    assert!(permissions.prompt().is_none());
    // The owner thread need not have processed either decision yet.
    std::fs::write(root.join("helper.exe"), "new helper").unwrap();
    assert!(permissions.authorize(&request).is_err());
    assert!(permissions.authorize(&renamed).is_err());
    renamed.file = "data/new-name.exe".into();
    std::fs::write(root.join(&renamed.file), "new helper name").unwrap();
    assert!(permissions.authorize(&renamed).is_err());
    assert!(permissions.prompt().is_none());
    // Another plugin, even in the same folder, is not denied.
    let mut unrelated = copy_request(&request);
    unrelated.entry = root.join("other.lua");
    std::fs::write(&unrelated.entry, "return 1").unwrap();
    assert!(matches!(
        permissions.authorize(&unrelated),
        Ok(Authorization::Pending(..))
    ));
    let fresh_session = ProcessPermissions::new(None);
    assert!(matches!(
        fresh_session.authorize(&request),
        Ok(Authorization::Pending(..))
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn worker_denial_covers_other_helpers_and_overrides_saved_approval() {
    let (root, request, permissions) = fixture();
    std::fs::write(root.join("other.exe"), "other helper").unwrap();
    let Authorization::Pending(grant, _first) = permissions.authorize(&request).unwrap() else {
        panic!("expected prompt");
    };
    let mut other = copy_request(&request);
    other.file = "other.exe".into();
    let Authorization::Pending(other_grant, second) = permissions.authorize(&other).unwrap() else {
        panic!("expected prompt");
    };
    // Even previously saved Always allow cannot override a session-wide Deny.
    permissions.saved.lock().unwrap().grants.push(other_grant);
    permissions.deny(grant);
    assert_eq!(second.load(Ordering::Acquire), ProcessDecision::Deny as u8);
    assert!(permissions.check_denied(&other.entry).is_err());
    assert!(permissions.authorize(&other).is_err());
    assert!(permissions.prompt().is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn denial_budget_is_fail_closed_instead_of_forgetting_new_denials() {
    let (root, request, permissions) = fixture();
    for n in 0..256 {
        permissions
            .denied
            .lock()
            .unwrap()
            .insert(DenialKey::new(&root.join(format!("plugin-{n}.lua"))));
    }
    assert!(
        permissions
            .check_denied(&request.entry)
            .is_err()
    );
    assert!(permissions.authorize(&request).is_err());
    assert!(permissions.prompt().is_none());
    std::fs::remove_dir_all(root).unwrap();
}
