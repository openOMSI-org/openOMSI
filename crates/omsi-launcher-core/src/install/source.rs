//! The source of an install: what kind of archive it is, unpacking the ones only an outside
//! tool reads, and the list of files in a folder or a zip.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArchiveKind {
    Zip,
    SevenZip,
    Rar,
}

pub(super) fn archive_kind(path: &Path) -> Option<ArchiveKind> {
    if !path.is_file() {
        return None;
    }
    match path.extension()?.to_string_lossy().to_ascii_lowercase().as_str() {
        "zip" => Some(ArchiveKind::Zip),
        "7z" => Some(ArchiveKind::SevenZip),
        "rar" => Some(ArchiveKind::Rar),
        _ => None,
    }
}

/// Expand formats the game cannot mount into the install job's own staging area. Header
/// sizes are checked against the content volume before any data is written; the later
/// install step hard-links these staged files into their final layout.
pub(super) fn unpack_archive(job: &Job, content: &Path, src: &Path, dest: &Path, kind: ArchiveKind) -> Result<()> {
    let (files, bytes) = match kind {
        ArchiveKind::SevenZip => {
            let reader = sevenz_rust2::ArchiveReader::open(src, sevenz_rust2::Password::empty())
                .with_context(|| trf("{name} is not a readable 7z archive", &[("name", &src.display())]))?;
            let entries = &reader.archive().files;
            let files = entries.iter().filter(|e| e.has_stream && !e.is_directory).count() as u64;
            let bytes = entries.iter().filter(|e| e.has_stream && !e.is_directory).fold(0u64, |sum, e| sum.saturating_add(e.size));
            (files, bytes)
        }
        ArchiveKind::Rar => {
            let archive = unrar_rs::RarArchive::open(std::fs::File::open(src)?)
                .with_context(|| trf("{name} is not a readable RAR archive", &[("name", &src.display())]))?;
            let members: Vec<_> = archive.entries().collect();
            if members.iter().any(|m| m.unpacked_size.is_none() && !m.is_directory) {
                return Err(anyhow!(trf("{name} is a multi-volume RAR archive; add all parts and start with the first .rar file", &[("name", &src.display())])));
            }
            let files = members.iter().filter(|m| !m.is_directory && !m.is_symlink && !m.is_hardlink).count() as u64;
            let bytes = members.iter().filter(|m| !m.is_directory && !m.is_symlink && !m.is_hardlink).fold(0u64, |sum, m| sum.saturating_add(m.unpacked_size.unwrap_or(0)));
            (files, bytes)
        }
        ArchiveKind::Zip => unreachable!(),
    };
    let free = free_space(content).unwrap_or(u64::MAX);
    let needed = bytes.saturating_add(MIN_MARGIN);
    job.set(|p| {
        p.files_total = files;
        p.bytes_total = bytes;
        p.free_bytes = free;
        p.needed_bytes = needed;
    });
    if needed > free {
        return Err(anyhow!(trf("not enough disk space to unpack {name}: {size} plus {margin} kept free needs {needed}, but only {free} is free on {disk}", &[("name", &src.display()), ("size", &gb(bytes)), ("margin", &gb(MIN_MARGIN)), ("needed", &gb(needed)), ("free", &gb(free)), ("disk", &content.display())])));
    }
    if job.cancelled() {
        return Err(anyhow!(Cancelled));
    }
    std::fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    match kind {
        ArchiveKind::SevenZip => {
            let mut done = 0u64;
            sevenz_rust2::decompress_file_with_extract_fn(src, dest, |entry, reader, _| {
                if job.cancelled() {
                    return Err(sevenz_rust2::Error::Io(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"), "install cancelled".into()));
                }
                let Some(target) = staged_target(dest, entry.name()) else {
                    std::io::copy(reader, &mut std::io::sink()).map_err(|e| sevenz_rust2::Error::Io(e, "skipping a refused entry".into()))?;
                    if !entry.is_directory {
                        job.set(|p| p.warnings.push(trf("unsafe 7z path refused: {path}", &[("path", &entry.name())])));
                    }
                    return Ok(true);
                };
                let wrote = sevenz_rust2::default_entry_extract_fn(entry, reader, &target)?;
                if wrote && entry.has_stream && !entry.is_directory {
                    done += 1;
                    job.files_done.store(done, Ordering::Relaxed);
                    if free_space(content).is_some_and(|left| left < ABORT_BELOW) {
                        return Err(sevenz_rust2::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "free disk space fell below 1 GB"), "install stopped".into()));
                    }
                }
                Ok(wrote)
            }).with_context(|| trf("unpacking {name} (the archive may be damaged or encrypted)", &[("name", &src.display())]))?;
        }
        ArchiveKind::Rar => {
            let mut archive = unrar_rs::RarArchive::open(std::fs::File::open(src)?)
                .with_context(|| format!("opening {}", src.display()))?;
            let members: Vec<_> = archive.entries().collect();
            for (index, member) in members.iter().enumerate() {
                if job.cancelled() {
                    return Err(anyhow!(Cancelled));
                }
                if member.is_directory || member.is_symlink || member.is_hardlink {
                    continue;
                }
                let Some(rel) = safe_rel(&member.name) else {
                    job.set(|p| p.warnings.push(trf("unsafe RAR path refused: {path}", &[("path", &member.name)])));
                    continue;
                };
                let out = dest.join(&rel);
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut file = std::fs::File::create(&out)?;
                let n = archive.by_index(index)?.copy_to(&mut file)
                    .with_context(|| format!("unpacking {} from {}", rel, src.display()))?;
                job.files_done.fetch_add(1, Ordering::Relaxed);
                job.bytes_done.fetch_add(n, Ordering::Relaxed);
                check_space(content)?;
            }
        }
        ArchiveKind::Zip => unreachable!(),
    }
    if job.cancelled() {
        return Err(anyhow!(Cancelled));
    }
    Ok(())
}

pub(super) fn staged_target(dest: &Path, name: &str) -> Option<PathBuf> {
    let target = dest.join(safe_rel(name)?);
    target.starts_with(dest).then_some(target)
}

pub(super) enum Source {
    /// A folder's files as they are on disk, by `Entry::index` (the name in `Entry::rel`
    /// may differ: `\` read as a separator).
    Folder(Vec<PathBuf>),
    Zip(zip::ZipArchive<std::io::BufReader<std::fs::File>>),
}

/// The files of a folder, with their paths on disk, and how many were left out because
/// their names cannot be installed safely.
pub(super) fn list_folder(dir: &Path, job: &Job) -> Result<(Vec<Entry>, Vec<PathBuf>, usize)> {
    let mut found = Vec::new();
    let mut refused = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if job.cancelled() {
            return Err(anyhow!(Cancelled));
        }
        for e in std::fs::read_dir(&d).with_context(|| format!("reading {}", d.display()))?.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(dir) {
                let Some(rel) = safe_rel(&rel.to_string_lossy()) else {
                    refused += 1;
                    continue;
                };
                if !is_junk(&rel) {
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    found.push((rel, size, p));
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let mut entries = Vec::with_capacity(found.len());
    let mut paths = Vec::with_capacity(found.len());
    for (index, (rel, size, p)) in found.into_iter().enumerate() {
        entries.push(Entry { rel, size, index });
        paths.push(p);
    }
    Ok((entries, paths, refused))
}

/// The read buffer an archive is opened with. The zip reader looks for the end of the
/// archive's table of contents from the back of the file in 2 KB steps, seeking before
/// each: every step emptied a 64 KB buffer and read it anew. The table of contents of a
/// 30 000-file map took 3.2 s here (0.06 s with 8 KB), and on a phone, whose shared storage
/// is read through a slow layer, minutes: the install stood at "reading the archive's table
/// of contents", 0 of 0 files, for every mod.
pub(super) const ZIP_BUFFER: usize = 1 << 13;

pub(super) fn list_zip(z: &mut zip::ZipArchive<std::io::BufReader<std::fs::File>>, job: &Job) -> Result<(Vec<Entry>, Vec<String>)> {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    let (mut encrypted, mut unsupported, mut unsafe_names) = (0usize, BTreeMap::<String, usize>::new(), 0usize);
    let n = z.len();
    job.set(|p| p.files_total = n as u64);
    for i in 0..n {
        if i % 512 == 0 {
            if job.cancelled() {
                return Err(anyhow!(Cancelled));
            }
            job.files_done.store(i as u64, Ordering::Relaxed);
        }
        let f = z.by_index_raw(i).with_context(|| format!("reading entry {i} of the archive"))?;
        if f.is_dir() {
            continue;
        }
        let Some(rel) = f.enclosed_name().and_then(|name| safe_rel(&name.to_string_lossy())) else {
            unsafe_names += 1;
            continue;
        };
        if is_junk(&rel) {
            continue;
        }
        if f.encrypted() {
            encrypted += 1;
        }
        let method = f.compression();
        let supported = matches!(method, zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated);
        if !supported {
            *unsupported.entry(format!("{method:?}")).or_default() += 1;
        }
        out.push(Entry { rel, size: f.size(), index: i });
    }
    job.files_done.store(0, Ordering::Relaxed);
    if encrypted > 0 {
        problems.push(trf("{encrypted} file(s) in the archive are password protected - unpack it with the password first and install the folder", &[("encrypted", &encrypted)]));
    }
    for (m, c) in unsupported {
        problems.push(trf("{c} file(s) are packed with {m}, which the launcher cannot unpack - unpack the archive with the system's tool and install the folder", &[("c", &c), ("m", &m)]));
    }
    if unsafe_names > 0 {
        problems.push(trf("{unsafe_names} file name(s) point outside the archive and were refused", &[("unsafe_names", &unsafe_names)]));
    }
    Ok((out, problems))
}
