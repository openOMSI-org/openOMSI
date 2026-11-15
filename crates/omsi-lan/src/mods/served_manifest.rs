use super::transfer_policy::POLICY_REFRESH;
use super::*;

pub(super) type ReadyManifest = Mutex<Option<Arc<ServedManifest>>>;

#[derive(Debug)]
pub(super) struct ServedManifest(
    pub(super) Manifest,
    pub(super) Vec<PathBuf>,
    Mutex<ManifestPolicy>,
);

#[derive(Debug)]
struct ManifestPolicy {
    checked: Instant,
    generation: u64,
    blocked: std::collections::HashSet<String>,
    fleet: bool,
    allowed: bool,
}

impl ServedManifest {
    pub(super) fn new((manifest, sources): (Manifest, Vec<PathBuf>), fleet: bool) -> Self {
        let policy = ManifestPolicy::check(&manifest, &sources, fleet);
        Self(manifest, sources, Mutex::new(policy))
    }

    pub(super) fn allows(&self, entry: &Entry) -> bool {
        let policy = self.2.lock().unwrap_or_else(|e| e.into_inner());
        policy.is_current()
            && policy.allowed
            && transfer_policy::vehicle_folder(&entry.path).is_none_or(|folder| {
                transfer_policy::transfers_enabled() && !policy.blocked.contains(&folder)
            })
    }

    pub(super) fn refresh(&self) {
        let (fleet, previous) = {
            let policy = self.2.lock().unwrap_or_else(|e| e.into_inner());
            if policy.is_current() && policy.checked.elapsed() < POLICY_REFRESH {
                return;
            }
            (policy.fleet, policy.checked)
        };
        let updated = ManifestPolicy::check(&self.0, &self.1, fleet);
        let mut policy = self.2.lock().unwrap_or_else(|e| e.into_inner());
        if policy.checked == previous && updated.generation == omsi_cfg::content_generation() {
            *policy = updated;
        }
    }
}

impl ManifestPolicy {
    fn is_current(&self) -> bool {
        self.generation == omsi_cfg::content_generation()
            && self.checked.elapsed() < POLICY_REFRESH * 2
    }

    fn check(manifest: &Manifest, sources: &[PathBuf], fleet: bool) -> Self {
        let checked = Instant::now();
        let generation = omsi_cfg::content_generation();
        let mut blocked = std::collections::HashSet::new();
        let mut allowed = manifest.entries.len() == sources.len();
        if fleet {
            allowed &= server_fleet::package_allowed(manifest, sources);
        } else {
            for (entry, source) in manifest.entries.iter().zip(sources) {
                if transfer_policy::is_descriptor(&entry.path)
                    && !transfer_policy::allows_file(source)
                {
                    if let Some(folder) = transfer_policy::vehicle_folder(&entry.path) {
                        blocked.insert(folder);
                    }
                }
            }
        }
        Self {
            checked,
            generation,
            blocked,
            fleet,
            allowed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_policy_refresh_blocks_the_whole_folder_of_a_newly_private_vehicle() {
        let dir = std::env::temp_dir().join(format!("openomsi-map-policy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let descriptor = dir.join("bus.bus");
        std::fs::write(
            &descriptor,
            "[openomsi_multiplayer]\nallow_file_transfer 1\n",
        )
        .unwrap();
        let entries = [
            "Vehicles/Mix/bus.bus",
            "Vehicles/Mix/paint.cti",
            "maps/M/global.cfg",
        ]
        .map(|path| Entry {
            path: path.into(),
            size: 0,
            sha256: String::new(),
        });
        let package = ServedManifest::new(
            (
                Manifest {
                    entries: entries.to_vec(),
                    ..Default::default()
                },
                vec![
                    descriptor.clone(),
                    dir.join("paint.cti"),
                    dir.join("global.cfg"),
                ],
            ),
            false,
        );
        assert!(package.allows(&entries[1]));
        std::fs::write(
            &descriptor,
            "[openomsi_multiplayer]\nallow_file_transfer 0\n",
        )
        .unwrap();
        package.2.lock().unwrap().checked -= POLICY_REFRESH;
        assert!(
            package.allows(&entries[1]),
            "GET is a lookup and leaves refresh to the worker"
        );
        package.refresh();
        assert!(!package.allows(&entries[1]));
        assert!(package.allows(&entries[2]));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
