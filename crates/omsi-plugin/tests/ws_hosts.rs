use omsi_plugin::{
    HostConfig, Plugins,
    api::process::{ProcessGrants, ProcessPermissions},
    ws::{self, HOSTS_FILE},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[test]
fn game_loading_reads_allowed_ws_hosts_from_plugins_dir() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "omsi-ws-hosts-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let plugins: PathBuf = root.join("plugins");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&plugins).unwrap();
    std::fs::write(
        plugins.join(HOSTS_FILE),
        "# game feeds\nwss://feeds.example.org/live\n",
    )
    .unwrap();

    let _host = Plugins::load_with_process_permissions(
        std::slice::from_ref(&plugins),
        &HostConfig::default(),
        ProcessGrants::default(),
        Arc::new(ProcessPermissions::new(None)),
    );

    let allowed = ws::hosts();
    assert_eq!(allowed, ["feeds.example.org"]);
    assert_eq!(
        ws::endpoint("wss://feeds.example.org/live", &allowed)
            .unwrap()
            .host,
        "feeds.example.org"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
