//! The full scan of a pack's files, and the record that lets an unchanged pack skip it.
#[cfg(windows)]
use std::collections::HashMap;
#[cfg(not(windows))]
use std::time::UNIX_EPOCH;
use std::{
    collections::{BTreeSet, HashSet},
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

use rayon::prelude::*;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

#[cfg(windows)]
use crate::change_marker;
use crate::{
    Asset, CHUNK_SIZE, MAX_ASSET_SIZE, PackError, Result, VerifiedPack, chunk_aad,
    ensure_regular_file, hex_digest, sha256_hex,
};

impl VerifiedPack {
    /// `verify_all` once per pack revision (manifest, key and the markers of every pack file) in this process.
    pub(crate) fn verify_once(&self) -> Result<()> {
        static VERIFIED: std::sync::Mutex<BTreeSet<(PathBuf, String, String, String)>> =
            std::sync::Mutex::new(BTreeSet::new());
        let Some(files) = self.files_fingerprint()? else {
            return self.verify_all();
        };
        let revision = (
            self.root.clone(),
            sha256_hex(&serde_json::to_vec(&self.manifest)?),
            self.content_key
                .as_ref()
                .map(|key| sha256_hex(key))
                .unwrap_or_default(),
            files,
        );
        if VERIFIED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&revision)
        {
            return Ok(());
        }
        self.verify_all()?;
        VERIFIED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(revision);
        Ok(())
    }

    /// Size, modification time and change marker of every pack file; `None` when one has no change time or cannot be listed.
    fn files_fingerprint(&self) -> Result<Option<String>> {
        let assets: Vec<Result<Option<Vec<u8>>>> = self
            .manifest
            .assets
            .par_iter()
            .map(|asset| self.asset_fingerprint(asset))
            .collect();
        let mut digest = Sha256::new();
        for asset in assets {
            let Some(markers) = asset? else {
                return Ok(None);
            };
            digest.update(markers);
        }
        Ok(Some(hex_digest(digest.finalize().as_slice())))
    }

    /// The names and markers of one asset's files, in the order `files_fingerprint` hashes them.
    fn asset_fingerprint(&self, asset: &Asset) -> Result<Option<Vec<u8>>> {
        let mut out = Vec::new();
        #[cfg(windows)]
        let mut folders = HashMap::new();
        // folders already found free of links while fingerprinting this asset
        #[cfg(not(windows))]
        let mut dirs = HashSet::new();
        let plain = [format!(
            "assets/{}/plain.bin",
            sha256_hex(asset.id.as_bytes())
        )];
        let files: Vec<&str> = if self.content_key.is_some() {
            asset.chunks.iter().map(|c| c.file.as_str()).collect()
        } else {
            plain.iter().map(String::as_str).collect()
        };
        for file in files {
            out.extend_from_slice(file.as_bytes());
            #[cfg(windows)]
            {
                let Some(marker) = change_marker::file_marker(&self.root, file, &mut folders)?
                else {
                    return Ok(None);
                };
                out.extend_from_slice(&marker);
            }
            #[cfg(not(windows))]
            {
                // a link anywhere in the path fails here, as it does in a full scan
                let meta = ensure_regular_file(&self.root.join(file), &mut dirs)
                    .map_err(|_| PackError::Invalid("chunk is missing, truncated, or changed"))?;
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .unwrap_or_default();
                out.extend_from_slice(&meta.len().to_le_bytes());
                out.extend_from_slice(&modified.as_nanos().to_le_bytes());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt as _;
                    out.extend_from_slice(&meta.ino().to_le_bytes());
                    out.extend_from_slice(&meta.ctime().to_le_bytes());
                    out.extend_from_slice(&meta.ctime_nsec().to_le_bytes());
                }
            }
        }
        Ok(Some(out))
    }

    /// Check every signed blob, each worker thread keeping memory to one authenticated chunk at a time.
    pub fn verify_all(&self) -> Result<()> {
        self.manifest
            .assets
            .par_iter()
            .try_for_each_init(HashSet::new, |dirs, asset| self.verify_asset(asset, dirs))
    }

    /// `verify_all` for one asset; `dirs` holds the folders this scan already found free of links.
    fn verify_asset(&self, asset: &Asset, dirs: &mut HashSet<PathBuf>) -> Result<()> {
        let Some(key) = &self.content_key else {
            return self.verify_plain(asset, dirs);
        };
        let cipher =
            LessSafeKey::new(UnboundKey::new(&AES_256_GCM, key).map_err(|_| PackError::WrongKey)?);
        let mut digest = Sha256::new();
        let mut size = 0u64;
        for (index, chunk) in asset.chunks.iter().enumerate() {
            let blob = self.read_chunk(chunk, dirs)?;
            let nonce = Nonce::try_assume_unique_for_key(&blob[..NONCE_LEN])
                .map_err(|_| PackError::Invalid("invalid chunk nonce"))?;
            let mut body = blob[NONCE_LEN..].to_vec();
            let aad = chunk_aad(
                &self.manifest,
                asset,
                index as u32,
                asset.chunks.len() as u32,
            );
            let plain = cipher
                .open_in_place(nonce, Aad::from(aad.as_slice()), &mut body)
                .map_err(|_| PackError::WrongKey)?;
            if plain.len() != chunk.plaintext_size as usize {
                return Err(PackError::Invalid(
                    "authenticated chunk size does not match manifest",
                ));
            }
            size += plain.len() as u64;
            digest.update(&*plain);
            body.zeroize();
        }
        if size != asset.size || hex_digest(digest.finalize().as_slice()) != asset.output_sha256 {
            return Err(PackError::Invalid(
                "asset bytes do not match the signed output hash",
            ));
        }
        Ok(())
    }

    /// `verify_all` for an unencrypted asset, hashed while it is read so memory stays bounded.
    fn verify_plain(&self, asset: &Asset, dirs: &mut HashSet<PathBuf>) -> Result<()> {
        if asset.size > MAX_ASSET_SIZE {
            return Err(PackError::Invalid("asset exceeds 64 MiB"));
        }
        let opaque = sha256_hex(asset.id.as_bytes());
        let path = self.root.join(format!("assets/{opaque}/plain.bin"));
        if ensure_regular_file(&path, dirs)?.len() > MAX_ASSET_SIZE {
            return Err(PackError::Invalid("file exceeds its size limit"));
        }
        let mut file = File::open(&path)?.take(MAX_ASSET_SIZE + 1);
        let (mut digest, mut size, mut buffer) = (Sha256::new(), 0u64, vec![0u8; CHUNK_SIZE]);
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    digest.update(&buffer[..n]);
                    size += n as u64;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
        if size != asset.size || hex_digest(digest.finalize().as_slice()) != asset.output_sha256 {
            return Err(PackError::Invalid(
                "asset bytes do not match the signed output hash",
            ));
        }
        Ok(())
    }
}
