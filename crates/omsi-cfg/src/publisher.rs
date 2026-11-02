//! Optional publisher packs, trusted provenance and map dependency references.
use omsi_pack::{Dependency, Manifest, TrustedPublishers, VerifiedPack};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use unicode_normalization::UnicodeNormalization;

static NEXT_DEPENDENCY_MOUNT: AtomicU64 = AtomicU64::new(1);
static MAP_ALIASES: std::sync::RwLock<Vec<MapAliasScope>> = std::sync::RwLock::new(Vec::new());

struct MapAliasScope {
    id: u64,
    map: PathBuf,
    declarations: BTreeMap<String, String>,
    aliases: BTreeMap<String, PathBuf>,
    mounts: Vec<PathBuf>,
    leases: usize,
}

const MAX_ALIAS_SCOPES: usize = 64;
const MAX_ACTIVE_ALIASES: usize = 32768;

fn invalid(message: impl ToString) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

pub fn digest(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn map_scope_path(path: &Path) -> io::Result<PathBuf> {
    if let Ok(path) = std::fs::canonicalize(path) {
        return Ok(path);
    }
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                clean.push(component.as_os_str())
            }
            std::path::Component::Normal(name) => clean.push(name),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !clean.pop() {
                    return Err(invalid("map path escapes its root"));
                }
            }
        }
    }
    Ok(clean)
}

pub fn bounded_read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(invalid("publisher metadata must not be a symbolic link"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("publisher input exceeds its size limit"));
    }
    Ok(bytes)
}

/// One trusted source/output identity. Exact hashes do not recognize arbitrary modifications.
#[derive(Clone, Debug)]
pub struct Provenance {
    pub dependency: Dependency,
    pub asset_id: String,
    pub path: String,
    pub source_sha256: String,
    pub output_sha256: String,
    pub redistribution: bool,
    pub license: String,
    pub author: String,
    pub protected: bool,
}

#[derive(Default)]
pub struct Catalog {
    pub entries: Vec<Provenance>,
}

fn trust(content: &Path) -> io::Result<TrustedPublishers> {
    let path = content.join("Publishers.json");
    if !path.exists() {
        return Ok(TrustedPublishers::default());
    }
    TrustedPublishers::load_json(&bounded_read(&path, 1024 * 1024)?).map_err(invalid)
}

fn folders(root: &Path) -> io::Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    if std::fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(invalid("publisher folder must not be a symbolic link"));
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            return Err(invalid("symbolic link in publisher folder"));
        }
        if entry.file_type()?.is_dir() {
            result.push(entry.path());
            if result.len() > 128 {
                return Err(invalid("too many publisher packs"));
            }
        }
    }
    result.sort();
    Ok(result)
}

fn dependency(m: &Manifest) -> Dependency {
    Dependency {
        publisher_id: m.publisher_id.clone(),
        pack_id: m.pack_id.clone(),
        version: m.version.clone(),
    }
}

fn cache_manifest(content: &Path, pack: &Path) -> io::Result<()> {
    let bytes = bounded_read(&pack.join("manifest.json"), 1024 * 1024)?;
    let sig = bounded_read(&pack.join("manifest.sig"), 1024)?;
    let root = content.join("PackIndex");
    if root.exists() && std::fs::symlink_metadata(&root)?.file_type().is_symlink() {
        return Err(invalid("publisher index must not be a symbolic link"));
    }
    std::fs::create_dir_all(&root)?;
    let target = root.join(digest(&bytes));
    if target.exists() {
        return Ok(());
    }
    let temp = root.join(format!(".new-{}", digest(&crate::vfs::random_key()?)));
    std::fs::create_dir(&temp)?;
    let result = (|| {
        std::fs::write(temp.join("manifest.json"), bytes)?;
        std::fs::write(temp.join("manifest.sig"), sig)?;
        std::fs::rename(&temp, &target)
    })();
    if temp.exists() {
        let _ = std::fs::remove_dir_all(temp);
    }
    if result.is_err() && target.is_dir() {
        return Ok(());
    }
    result
}

impl Catalog {
    pub fn load(content: &Path) -> io::Result<Self> {
        let trust = trust(content)?;
        for path in folders(&content.join("Packs"))? {
            if path.join("manifest.json").exists() {
                omsi_pack::load_manifest(&path, &trust).map_err(invalid)?;
                cache_manifest(content, &path)?;
            }
        }
        let mut catalog = Self::default();
        let mut total = 0;
        for path in folders(&content.join("PackIndex"))? {
            total += bounded_read(&path.join("manifest.json"), 1024 * 1024)?.len();
            if total > 16 * 1024 * 1024 {
                return Err(invalid("publisher index exceeds its size limit"));
            }
            let manifest = omsi_pack::load_manifest(&path, &trust).map_err(invalid)?;
            for asset in &manifest.assets {
                catalog.entries.push(Provenance {
                    dependency: dependency(&manifest),
                    asset_id: asset.id.clone(),
                    path: asset.path.clone(),
                    source_sha256: asset.source_sha256.clone(),
                    output_sha256: asset.output_sha256.clone(),
                    redistribution: manifest.redistribution,
                    license: manifest.license.clone(),
                    author: manifest.author.clone(),
                    protected: manifest.key_id.is_some(),
                });
                if catalog.entries.len() > 32768 {
                    return Err(invalid("too many indexed publisher assets"));
                }
            }
        }
        Ok(catalog)
    }

    pub fn recognize(&self, hash: &str) -> Option<&Provenance> {
        self.recognize_restrictive(hash)
    }

    pub fn matches(&self, hash: &str) -> Vec<&Provenance> {
        self.entries
            .iter()
            .filter(|a| a.source_sha256 == hash || a.output_sha256 == hash)
            .collect()
    }

    /// If publishers make conflicting claims for the same bytes, surface the most restrictive
    /// claim first so ordinary export warns until a rights-holder explicitly overrides it.
    pub fn recognize_restrictive(&self, hash: &str) -> Option<&Provenance> {
        self.matches(hash)
            .into_iter()
            .min_by_key(|a| (!a.protected, a.redistribution))
    }
}

/// Local output path -> stable signed asset identity, carried alongside a shared map.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetReference {
    pub path: String,
    pub dependency: Dependency,
    pub asset_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapReferences {
    pub format_version: u32,
    pub dependencies: Vec<Dependency>,
    pub assets: Vec<AssetReference>,
}

impl Default for MapReferences {
    fn default() -> Self {
        Self {
            format_version: 1,
            dependencies: Vec::new(),
            assets: Vec::new(),
        }
    }
}

fn same(a: &Dependency, b: &Dependency) -> bool {
    a.publisher_id == b.publisher_id && a.pack_id == b.pack_id && a.version == b.version
}

fn installed(content: &Path, trust: &TrustedPublishers) -> io::Result<Vec<(PathBuf, Manifest)>> {
    let mut result: Vec<(PathBuf, Manifest)> = Vec::new();
    for path in folders(&content.join("Packs"))? {
        let manifest = omsi_pack::load_manifest(&path, trust).map_err(invalid)?;
        if result
            .iter()
            .any(|(_, m)| same(&dependency(m), &dependency(&manifest)))
        {
            return Err(invalid("duplicate installed publisher pack identity"));
        }
        result.push((path, manifest));
    }
    Ok(result)
}

type PackKey = (String, String, String);

fn pack_key(d: &Dependency) -> PackKey {
    (d.publisher_id.clone(), d.pack_id.clone(), d.version.clone())
}

fn trusted_installed(
    content: &Path,
    trust: &TrustedPublishers,
) -> io::Result<Vec<(PathBuf, Manifest)>> {
    let mut result = Vec::new();
    for path in folders(&content.join("Packs"))? {
        // Untrusted or incomplete packages are never mounted. A map naming one fails below
        // as a missing dependency; unrelated legacy folders do not disable ordinary maps.
        let Ok(manifest) = omsi_pack::load_manifest(&path, trust) else {
            continue;
        };
        if result
            .iter()
            .any(|(_, m): &(PathBuf, Manifest)| dependency(m) == dependency(&manifest))
        {
            return Err(invalid("duplicate installed publisher pack identity"));
        }
        result.push((path, manifest));
    }
    Ok(result)
}

/// The map-scoped virtual roots installed during world open. Dropping the World removes
/// these aliases so another map cannot inherit them.
#[derive(Debug, Default)]
pub struct DependencyMounts {
    mounts: Vec<PathBuf>,
    alias_scope: Option<u64>,
}

impl Drop for DependencyMounts {
    fn drop(&mut self) {
        if let Some(id) = self.alias_scope.take() {
            let mut scopes = MAP_ALIASES.write().unwrap_or_else(|e| e.into_inner());
            if let Some(index) = scopes.iter().position(|s| s.id == id) {
                scopes[index].leases = scopes[index].leases.saturating_sub(1);
                if scopes[index].leases == 0 {
                    let scope = scopes.remove(index);
                    drop(scopes);
                    for root in scope.mounts.into_iter().rev() {
                        crate::vfs::unmount_sealed(&root);
                    }
                    crate::content_changed();
                }
            }
        } else {
            for root in self.mounts.drain(..).rev() {
                crate::vfs::unmount_sealed(&root);
            }
        }
    }
}

fn path_key(path: &str) -> String {
    path.nfkc().flat_map(char::to_lowercase).collect()
}

fn safe_game_path(path: &str) -> bool {
    if path.is_empty()
        || path.len() > 1024
        || path.contains(['\\', ':', '\0'])
        || path.starts_with('/')
    {
        return false;
    }
    path.split('/').all(|part| {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part.chars().any(char::is_control)
        {
            return false;
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        !matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    })
}

/// Validate a map's data alias against the signed asset path without mounting it.
pub fn validate_asset_alias(alias: &str, canonical: &str) -> io::Result<()> {
    if !safe_game_path(alias) || !safe_game_path(canonical) {
        return Err(invalid("unsafe publisher asset path"));
    }
    let category = |p: &str| p.split('/').next().unwrap_or("").to_ascii_lowercase();
    let extension = |p: &str| {
        Path::new(p)
            .extension()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
    };
    let supported = [
        "maps",
        "sceneryobjects",
        "splines",
        "texture",
        "vehicles",
        "fonts",
    ];
    let formats = [
        "cfg", "map", "terrain", "sco", "sli", "o3d", "x", "gltf", "glb", "fbx", "obj", "dds",
        "bmp", "tga", "png", "jpg", "jpeg", "wav", "oft", "bin",
    ];
    if category(alias) != category(canonical)
        || !supported.contains(&category(alias).as_str())
        || extension(alias) != extension(canonical)
        || !formats.contains(&extension(alias).as_str())
    {
        return Err(invalid(
            "publisher aliases must keep a data file's folder and format",
        ));
    }
    Ok(())
}

fn check_aliases(aliases: &[(String, String)]) -> io::Result<()> {
    let mut seen = BTreeSet::new();
    for (alias, canonical) in aliases {
        validate_asset_alias(alias, canonical)?;
        let key = path_key(alias);
        if !seen.insert(key.clone()) {
            return Err(invalid("duplicate publisher dependency alias"));
        }
        for slash in key.match_indices('/') {
            let prefix = &key[..slash.0];
            if seen.contains(prefix) {
                return Err(invalid("publisher aliases collide as a file and folder"));
            }
        }
        if seen
            .iter()
            .any(|prior: &String| prior.starts_with(&(key.clone() + "/")))
        {
            return Err(invalid("publisher aliases collide as a file and folder"));
        }
    }
    Ok(())
}

fn register_aliases(
    map_dir: &Path,
    declarations: BTreeMap<String, String>,
    aliases: BTreeMap<String, PathBuf>,
    mounts: &mut Vec<PathBuf>,
) -> io::Result<(u64, bool)> {
    if aliases.is_empty() {
        return Ok((0, false));
    }
    let map = map_scope_path(map_dir)?;
    let mut scopes = MAP_ALIASES.write().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = scopes.iter_mut().find(|s| s.map == map) {
        if existing.declarations != declarations {
            return Err(invalid(
                "the same map is already open with different publisher dependencies",
            ));
        }
        existing.leases = existing
            .leases
            .checked_add(1)
            .ok_or_else(|| invalid("publisher map scope limit reached"))?;
        let id = existing.id;
        drop(scopes);
        for root in mounts.drain(..).rev() {
            crate::vfs::unmount_sealed(&root);
        }
        return Ok((id, true));
    }
    let active: usize = scopes.iter().map(|s| s.aliases.len()).sum();
    if scopes.len() >= MAX_ALIAS_SCOPES || active.saturating_add(aliases.len()) > MAX_ACTIVE_ALIASES
    {
        return Err(invalid("too many active publisher map aliases"));
    }
    let id = NEXT_DEPENDENCY_MOUNT.fetch_add(1, Ordering::Relaxed);
    scopes.push(MapAliasScope {
        id,
        map,
        declarations,
        aliases,
        mounts: std::mem::take(mounts),
        leases: 1,
    });
    drop(scopes);
    crate::content_changed();
    Ok((id, false))
}

/// Resolve a signed asset alias only for the map that declared it.
pub(crate) fn resolve_alias(base: &Path, rel: &str) -> Option<PathBuf> {
    let scopes = MAP_ALIASES.read().unwrap_or_else(|e| e.into_inner());
    let base = map_scope_path(base).ok()?;
    for scope in scopes.iter().rev() {
        let suffix = base.strip_prefix(&scope.map).ok().or_else(|| {
            scope
                .mounts
                .iter()
                .find_map(|mount| base.strip_prefix(mount).ok())
        });
        let Some(suffix) = suffix else { continue };
        let mut parts: Vec<String> = suffix
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(p) => Some(p.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        for part in rel.split(['\\', '/']) {
            match part {
                "" | "." => {}
                ".." if !parts.is_empty() => {
                    parts.pop();
                }
                ".." => return None,
                p if p.starts_with('/') || p.contains(':') => return None,
                p => parts.push(p.to_string()),
            }
        }
        if let Some(path) = scope.aliases.get(&path_key(&parts.join("/"))) {
            return Some(path.clone());
        }
    }
    None
}

fn dependency_closure(
    required: &Dependency,
    available: &BTreeMap<PackKey, (PathBuf, Manifest)>,
    visiting: &mut BTreeSet<PackKey>,
    selected: &mut BTreeSet<PackKey>,
    depth: usize,
) -> io::Result<()> {
    let key = pack_key(required);
    if selected.contains(&key) {
        return Ok(());
    }
    if depth > 128 || !visiting.insert(key.clone()) {
        return Err(invalid("publisher dependency cycle or depth limit"));
    }
    let (_, manifest) = available.get(&key).ok_or_else(|| {
        invalid(format!(
            "missing or wrong-version publisher dependency {}/{}/{}",
            required.publisher_id, required.pack_id, required.version
        ))
    })?;
    for dependency in &manifest.dependencies {
        dependency_closure(dependency, available, visiting, selected, depth + 1)?;
    }
    visiting.remove(&key);
    selected.insert(key);
    Ok(())
}

/// Verify every asset before making a publisher pack visible to parsers and decoders.
pub fn mount_packs(content: &Path) -> io::Result<Vec<PathBuf>> {
    let trust = trust(content)?;
    let mut packs: Vec<(PathBuf, Manifest)> = Vec::new();
    for path in folders(&content.join("Packs"))? {
        let manifest = match omsi_pack::load_manifest(&path, &trust) {
            Ok(manifest) => manifest,
            Err(error) => {
                log::warn!("publisher pack {} is unavailable: {error}", path.display());
                continue;
            }
        };
        if packs
            .iter()
            .any(|(_, existing)| same(&dependency(existing), &dependency(&manifest)))
        {
            log::warn!(
                "publisher pack {} duplicates an installed identity and was skipped",
                path.display()
            );
            continue;
        }
        packs.push((path, manifest));
    }
    let mut opened = Vec::new();
    for (path, manifest) in packs {
        match VerifiedPack::open_unlocked(&path, &trust, &omsi_pack::SystemKeyStore) {
            Ok(pack) => opened.push((path, manifest, Arc::new(pack))),
            Err(error) => log::warn!(
                "publisher pack {} is unavailable until unlocked: {error}",
                path.display()
            ),
        }
    }
    loop {
        let before = opened.len();
        let available: BTreeSet<PackKey> = opened
            .iter()
            .map(|(_, manifest, _)| pack_key(&dependency(manifest)))
            .collect();
        opened.retain(|(path, manifest, _)| {
            let missing = manifest.dependencies.iter().find(|required| {
                !available.contains(&pack_key(required))
            });
            if let Some(required) = missing {
                log::warn!(
                    "publisher pack {} is unavailable because dependency {}/{}/{} is not mounted",
                    path.display(), required.publisher_id, required.pack_id, required.version
                );
                false
            } else {
                true
            }
        });
        if opened.len() == before {
            break;
        }
    }
    let mut mounts = Vec::new();
    for (path, _, pack) in opened {
        let mount = path.join(".mount");
        match crate::vfs::mount_publisher(&mount, pack, BTreeMap::new()) {
            Ok(_) => {
                crate::add_content_root(mount.clone());
                mounts.push(mount);
            }
            Err(error) => log::warn!(
                "publisher pack {} could not be mounted: {error}",
                path.display()
            ),
        }
    }
    Ok(mounts)
}

/// Called at the common world-loader boundary; ordinary maps require no new metadata.
pub fn validate_map(map_dir: &Path) -> io::Result<DependencyMounts> {
    let file = map_dir.join("dependencies.json");
    if !crate::vfs::is_file(&file) {
        return Ok(DependencyMounts::default());
    }
    let bytes = crate::vfs::read_bounded(&file, 1024 * 1024)?;
    let refs: MapReferences = serde_json::from_slice(&bytes).map_err(invalid)?;
    if refs.format_version != 1 || refs.dependencies.len() > 128 || refs.assets.len() > 4096 {
        return Err(invalid("unsupported or oversized map dependency manifest"));
    }
    let mut roots = Vec::new();
    let mut available = BTreeMap::new();
    for root in crate::content_roots()
        .into_iter()
        .filter(|r| r.join("Publishers.json").is_file())
    {
        let trusted = trust(&root)?;
        for (path, manifest) in trusted_installed(&root, &trusted)? {
            let key = pack_key(&dependency(&manifest));
            if available.insert(key, (path, manifest)).is_some() {
                return Err(invalid(
                    "publisher dependency is duplicated across content roots",
                ));
            }
        }
        roots.push((root, trusted));
    }
    let mut selected = BTreeSet::new();
    for required in &refs.dependencies {
        dependency_closure(required, &available, &mut BTreeSet::new(), &mut selected, 0)?;
    }
    let mut packs = Vec::new();
    for key in &selected {
        let (path, manifest) = available
            .get(key)
            .ok_or_else(|| invalid("publisher dependency disappeared"))?;
        let trusted = roots
            .iter()
            .find(|(root, _)| path.starts_with(root))
            .map(|(_, trust)| trust)
            .ok_or_else(|| invalid("publisher trust root disappeared"))?;
        let pack = VerifiedPack::open_unlocked(path, trusted, &omsi_pack::SystemKeyStore)
            .map_err(invalid)?;
        packs.push((path.clone(), Arc::new(pack)));
    }
    for required in &refs.dependencies {
        if !packs
            .iter()
            .any(|(_, p)| same(required, &dependency(p.manifest())))
        {
            return Err(invalid(format!(
                "missing or wrong-version publisher dependency {}/{}/{}",
                required.publisher_id, required.pack_id, required.version
            )));
        }
    }
    let mut aliases = Vec::new();
    let mut declarations = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for reference in &refs.assets {
        if !seen.insert(path_key(&reference.path)) {
            return Err(invalid("duplicate dependency alias"));
        }
        if !refs
            .dependencies
            .iter()
            .any(|d| same(d, &reference.dependency))
        {
            return Err(invalid("asset names an undeclared dependency"));
        }
        let pack = packs
            .iter()
            .find(|(_, p)| same(&reference.dependency, &dependency(p.manifest())))
            .map(|(_, p)| p)
            .ok_or_else(|| invalid("asset dependency is not in the verified closure"))?;
        let asset = pack
            .assets()
            .iter()
            .find(|a| a.id == reference.asset_id)
            .ok_or_else(|| invalid("dependency asset identity was not found"))?;
        validate_asset_alias(&reference.path, &asset.path)?;
        aliases.push((reference.path.clone(), asset.path.clone()));
        let dep = pack_key(&reference.dependency);
        let signed_manifest = serde_json::to_vec(pack.manifest()).map_err(invalid)?;
        declarations.insert(
            reference.path.clone(),
            format!(
                "{}\0{}\0{}\0{}\0{}",
                dep.0,
                dep.1,
                dep.2,
                digest(&signed_manifest),
                asset.path
            ),
        );
    }
    check_aliases(&aliases)?;
    let mut roots_added = BTreeMap::new();
    let mut virtual_mounts: Vec<PathBuf> = Vec::new();
    for (index, (_, pack)) in packs.into_iter().enumerate() {
        let mut pack_aliases: BTreeMap<String, String> = pack
            .assets()
            .iter()
            .filter(|asset| validate_asset_alias(&asset.path, &asset.path).is_ok())
            .map(|asset| (asset.path.clone(), asset.path.clone()))
            .collect();
        for reference in refs
            .assets
            .iter()
            .filter(|r| same(&r.dependency, &dependency(pack.manifest())))
        {
            let canonical = pack
                .assets()
                .iter()
                .find(|a| a.id == reference.asset_id)
                .unwrap()
                .path
                .clone();
            pack_aliases.insert(reference.path.clone(), canonical);
        }
        if pack_aliases.is_empty() {
            continue;
        }
        let id = NEXT_DEPENDENCY_MOUNT.fetch_add(1, Ordering::Relaxed);
        let mount = map_dir.join(format!(
            ".publisher-world/{}/{id}-{index}",
            std::process::id()
        ));
        for alias in pack_aliases.keys() {
            let key = path_key(alias);
            let target = mount.join(alias);
            if roots_added.insert(key, target.clone()).is_some_and(|prior| prior != target) {
                for root in virtual_mounts.drain(..).rev() {
                    crate::vfs::unmount_sealed(&root);
                }
                return Err(invalid("publisher aliases collide across dependencies"));
            }
        }
        if let Err(error) = crate::vfs::mount_publisher(&mount, pack.clone(), pack_aliases) {
            for root in virtual_mounts.drain(..).rev() {
                crate::vfs::unmount_sealed(&root);
            }
            return Err(error);
        }
        virtual_mounts.push(mount);
    }
    let (alias_scope, shared_scope) =
        match register_aliases(map_dir, declarations, roots_added, &mut virtual_mounts) {
            Ok(id) => id,
            Err(error) => {
                for root in virtual_mounts.drain(..).rev() {
                    crate::vfs::unmount_sealed(&root);
                }
                return Err(error);
            }
        };
    if shared_scope {
        for root in virtual_mounts.drain(..).rev() {
            crate::vfs::unmount_sealed(&root);
        }
    }
    Ok(DependencyMounts {
        mounts: virtual_mounts,
        alias_scope: (alias_scope != 0).then_some(alias_scope),
    })
}
