//! Shared desktop user-data routing. Android keeps its existing home-based layout.
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

static ACTIVE: RwLock<Option<PathBuf>> = RwLock::new(None);
const CONFIG_FILE: &str = ".openomsi-config";

/// Pointer beside the executable, or inside an application's Contents directory.
pub fn config_file_for(program: &Path) -> PathBuf {
    for ancestor in program.ancestors() {
        if ancestor
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"))
        {
            return ancestor.join("Contents").join(CONFIG_FILE);
        }
    }
    program
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(CONFIG_FILE)
}

/// Require an absolute OS-native directory whose final component is exactly .openomsi.
pub fn validate_path(path: &Path) -> io::Result<()> {
    let text = path
        .to_str()
        .ok_or_else(|| invalid("data path is not valid UTF-8"))?;
    if !path.is_absolute()
        || path.file_name() != Some(std::ffi::OsStr::new(".openomsi"))
        || !text.ends_with(".openomsi")
        || text.contains(['\r', '\n', '\0'])
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid(
            "config_path must be an absolute OS-native path ending exactly in .openomsi",
        ));
    }
    if path.exists() && !path.is_dir() {
        return Err(invalid("config_path is not a directory"));
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn parse_config(text: &str) -> io::Result<PathBuf> {
    let mut selected = None;
    for line in text.trim_start_matches('\u{feff}').lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let value = line
            .strip_prefix("config_path=")
            .ok_or_else(|| invalid("expected config_path=<absolute path>"))?;
        if selected.is_some() {
            return Err(invalid("duplicate config_path"));
        }
        let path = PathBuf::from(value);
        validate_path(&path)?;
        selected = Some(path);
    }
    selected.ok_or_else(|| invalid("missing config_path"))
}

fn default_dir() -> io::Result<PathBuf> {
    let path = [std::env::var_os("HOME"), std::env::var_os("USERPROFILE")]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .find(|p| p.is_absolute() && (!cfg!(windows) || p.is_dir()))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "cannot locate the user's home directory",
            )
        })?
        .join(".openomsi");
    validate_path(&path)?;
    Ok(path)
}

/// Write a desktop pointer. This does not change the active path or migrate data.
pub fn write_config_for(program: &Path, path: &Path) -> io::Result<()> {
    validate_path(path)?;
    let file = config_file_for(program);
    let contents = format!("config_path={}\n", path.to_str().unwrap());
    if std::fs::read_to_string(&file).ok().as_deref() == Some(&contents) { return Ok(()); }
    let tmp = file.with_extension("openomsi-config.tmp");
    std::fs::write(&tmp, contents)?;
    match std::fs::rename(&tmp, &file) {
        Ok(()) => Ok(()),
        Err(first) => {
            // Windows cannot replace an existing file with rename. Keep a rollback copy.
            let backup = file.with_extension("openomsi-config.bak");
            let had_old = file.exists();
            if had_old { std::fs::rename(&file, &backup)?; }
            match std::fs::rename(&tmp, &file) {
                Ok(()) => { let _ = std::fs::remove_file(&backup); Ok(()) }
                Err(error) => {
                    let _ = std::fs::remove_file(&tmp);
                    if had_old { let _ = std::fs::rename(&backup, &file); }
                    Err(if had_old { error } else { first })
                }
            }
        }
    }
}

fn resolve_for(
    program: &Path,
    default: impl FnOnce() -> io::Result<PathBuf>,
) -> io::Result<PathBuf> {
    let path = match std::fs::read_to_string(config_file_for(program)) {
        Ok(text) => parse_config(&text)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let path = default()?;
            validate_path(&path)?;
            std::fs::create_dir_all(&path)?;
            // A second process may have created its pointer while we made the directory.
            use std::io::Write;
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(config_file_for(program))
            {
                Ok(mut file) => writeln!(file, "config_path={}", path.to_str().unwrap())?,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    let selected =
                        parse_config(&std::fs::read_to_string(config_file_for(program))?)?;
                    std::fs::create_dir_all(&selected)?;
                    return Ok(selected);
                }
                Err(e) => return Err(e),
            }
            path
        }
        Err(e) => return Err(e),
    };
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Initialize before settings/logs are read, using the game's executable path.
/// Errors leave the previously active directory unchanged.
pub fn initialize_for(program: &Path) -> io::Result<PathBuf> {
    let mut active = ACTIVE.write().unwrap_or_else(|e| e.into_inner());
    let path = if cfg!(target_os = "android") {
        default_dir()?
    } else {
        resolve_for(program, default_dir)?
    };
    *active = Some(path.clone());
    Ok(path)
}

/// Switch runtime consumers after the launcher has successfully moved/written its data.
pub fn set_active_path(path: &Path) -> io::Result<()> {
    validate_path(path)?;
    *ACTIVE.write().unwrap_or_else(|e| e.into_inner()) = Some(path.to_path_buf());
    Ok(())
}

/// Active data directory. Desktop callers lazily initialize from current_exe, once.
/// Unit tests do not create pointers implicitly; explicit initialization still does.
pub fn data_dir() -> PathBuf {
    if cfg!(target_os = "android") {
        return std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".openomsi");
    }
    if let Some(path) = ACTIVE.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return path.clone();
    }
    let mut active = ACTIVE.write().unwrap_or_else(|e| e.into_inner());
    let mut failure = None;
    let path = active
        .get_or_insert_with(|| {
            if !cfg!(test) {
                match std::env::current_exe().and_then(|program| resolve_for(&program, default_dir))
                {
                    Ok(path) => return path,
                    Err(e) => failure = Some(e),
                }
            }
            default_dir().unwrap_or_else(|_| std::env::temp_dir().join(".openomsi"))
        })
        .clone();
    // A logger may itself use data_dir(); publish the fallback and unlock first.
    drop(active);
    if let Some(e) = failure {
        log::warn!("cannot initialize .openomsi-config: {e}; using default user data directory");
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "openomsi-user-data-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn pointer_locations() {
        let root = std::env::temp_dir();
        assert_eq!(
            config_file_for(&root.join("openomsi.exe")),
            root.join(CONFIG_FILE)
        );
        let app = root.join("openomsi.app");
        assert_eq!(
            config_file_for(&app.join("Contents/MacOS/openomsi")),
            app.join("Contents").join(CONFIG_FILE)
        );
        assert_eq!(
            config_file_for(&app),
            app.join("Contents").join(CONFIG_FILE)
        );
    }

    #[test]
    fn parses_native_absolute_path_and_rejects_invalid_values() {
        let root = Temp::new();
        let path = root.0.join("directory with spaces").join(".openomsi");
        let text = format!("config_path={}\r\n", path.display());
        assert_eq!(parse_config(&text).unwrap(), path);
        for text in [
            "",
            "config_path=.openomsi",
            "other=value",
            "config_path=relative/.openomsi",
        ] {
            assert!(parse_config(text).is_err(), "{text}");
        }
        assert!(parse_config(&format!("{text}{text}")).is_err());
        assert!(validate_path(&root.0.join(".openomsi-other")).is_err());
        assert!(validate_path(&root.0.join("..").join(".openomsi")).is_err());
        let file = root.0.join(".openomsi");
        std::fs::write(&file, b"not a directory").unwrap();
        assert!(validate_path(&file).is_err());
    }

    #[test]
    fn missing_pointer_reuses_default_and_existing_pointer_wins() {
        let root = Temp::new();
        let program = root.0.join("openomsi");
        let default = root.0.join("home").join(".openomsi");
        std::fs::create_dir_all(&default).unwrap();
        std::fs::write(default.join("settings.cfg"), b"existing").unwrap();
        assert_eq!(
            resolve_for(&program, || Ok(default.clone())).unwrap(),
            default
        );
        assert_eq!(
            std::fs::read(default.join("settings.cfg")).unwrap(),
            b"existing"
        );
        assert_eq!(
            parse_config(&std::fs::read_to_string(config_file_for(&program)).unwrap()).unwrap(),
            default
        );
        let chosen = root.0.join("elsewhere").join(".openomsi");
        write_config_for(&program, &chosen).unwrap();
        assert_eq!(
            resolve_for(&program, || panic!("must not consult home")).unwrap(),
            chosen
        );
        assert!(chosen.is_dir());
    }

    #[test]
    fn invalid_pointer_is_not_overwritten() {
        let root = Temp::new();
        let program = root.0.join("openomsi");
        let config = config_file_for(&program);
        std::fs::write(&config, "config_path=relative/.openomsi\n").unwrap();
        assert!(resolve_for(&program, || panic!("invalid pointer is not missing")).is_err());
        assert_eq!(
            std::fs::read_to_string(config).unwrap(),
            "config_path=relative/.openomsi\n"
        );
    }

    #[test]
    fn bundle_pointer_initializes_new_default_directory() {
        let root = Temp::new();
        let program = root.0.join("openomsi.app/Contents/MacOS/openomsi");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        let default = root.0.join("home/.openomsi");
        assert!(!default.exists());
        assert_eq!(
            resolve_for(&program, || Ok(default.clone())).unwrap(),
            default
        );
        assert!(default.is_dir());
        assert!(
            root.0
                .join("openomsi.app/Contents/.openomsi-config")
                .is_file()
        );
        assert!(
            !root
                .0
                .join("openomsi.app/Contents/MacOS/.openomsi-config")
                .exists()
        );
    }
}
