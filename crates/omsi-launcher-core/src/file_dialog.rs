use std::path::PathBuf;

/// Native folder / file picker (Finder, Explorer, the GTK dialog) for a mod. Must run on
/// the main thread. (None on a phone: the launcher browses the storage itself there, and on an iPhone or an iPad
/// the system's document picker of the Swift launcher hands the path over.)
pub fn pick_mod(zip: bool) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if zip {
            rfd::FileDialog::new().set_title(crate::i18n::tr("Choose a mod archive")).add_filter(crate::i18n::tr("Mod archive"), &["zip", "7z", "rar"]).pick_file()
        } else {
            rfd::FileDialog::new().set_title(crate::i18n::tr("Choose the mod folder")).pick_folder()
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = zip;
        None
    }
}

/// Folder picker (Setup: the OMSI 2 folder).
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        rfd::FileDialog::new().set_title(title).pick_folder()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = title;
        None
    }
}

/// File picker (Setup: the game program).
pub fn pick_file(title: &str) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        rfd::FileDialog::new().set_title(title).pick_file()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = title;
        None
    }
}

pub fn pick_hofs(title: &str, filter: &str) -> Option<Vec<PathBuf>> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        rfd::FileDialog::new().set_title(title).add_filter(filter, &["hof"]).pick_files()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (title, filter);
        None
    }
}
