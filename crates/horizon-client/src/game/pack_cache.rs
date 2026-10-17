//! Durable cache of immutable face geometry prepared for the block renderer.
//! Source stamps include pack overlays and mod-provided replacements.
use horizon_world::{block_render::FaceVisibility, registry::BlockRegistry};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const VERSION: u32 = 1;
const INPUTS: &[&str] = &["occlusion.nbt", "render-definitions.json", "facts.json"];

fn cache_key(root: &Path) -> io::Result<String> {
    let mut hash = Sha256::new();
    hash.update(VERSION.to_le_bytes());
    hash.update(fs::canonicalize(root)?.as_os_str().as_encoded_bytes());
    for name in INPUTS {
        let path = root.join(name);
        let metadata = fs::metadata(&path)?;
        hash.update(name.as_bytes());
        hash.update(metadata.len().to_le_bytes());
        let changed = metadata
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        hash.update(changed.as_secs().to_le_bytes());
        hash.update(changed.subsec_nanos().to_le_bytes());
        // ctime also catches replacements preserving mtime and size on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            hash.update(metadata.dev().to_le_bytes());
            hash.update(metadata.ino().to_le_bytes());
            hash.update(metadata.ctime().to_le_bytes());
            hash.update(metadata.ctime_nsec().to_le_bytes());
        }
        #[cfg(not(unix))]
        {
            // Windows/macOS may lack a change timestamp. Hash the source
            // bytes on these platforms to catch same-size timestamp rewrites.
            hash.update(Sha256::digest(fs::read(path)?));
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn cache_path(root: &Path) -> io::Result<PathBuf> {
    let key = cache_key(root)?;
    Ok(std::env::current_dir()?
        .join(".horizon-cache/pack-render")
        .join(format!("{key}.json")))
}

pub(super) fn load_visibility(
    root: &Path,
    registry: &BlockRegistry,
) -> horizon_client::Result<FaceVisibility> {
    // Profiling switch for a like-for-like measurement of the uncached path.
    if std::env::var_os("HORIZON_DISABLE_PACK_RENDER_CACHE").is_some() {
        return horizon_client::models::load_visibility(root, registry);
    }
    let path = cache_path(root).ok();
    if let Some(path) = &path {
        if let Ok(bytes) = fs::read(path) {
            if bytes.len() >= 32
                && Sha256::digest(&bytes[32..]).as_slice() == &bytes[..32]
                && let Ok(visibility) =
                    FaceVisibility::from_cache_bytes(&bytes[32..], registry.len())
            {
                log::info!("Horizon: cached pack face geometry");
                return Ok(visibility);
            }
        }
    }
    let visibility = horizon_client::models::load_visibility(root, registry)?;
    if let Some(path) = path {
        if let Ok(bytes) = visibility.to_cache_bytes() {
            if let Some(parent) = path.parent() {
                if fs::create_dir_all(parent).is_ok() {
                    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
                    let mut signed = Sha256::digest(&bytes).to_vec();
                    signed.extend_from_slice(&bytes);
                    if fs::write(&temporary, signed).is_ok() {
                        if fs::rename(&temporary, &path).is_err() {
                            let _ = fs::remove_file(&temporary);
                        }
                    }
                }
            }
        }
    }
    Ok(visibility)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_stamp_invalidates_on_change() {
        let root = std::env::temp_dir().join(format!("pack-render-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for name in INPUTS {
            fs::write(root.join(name), b"one").unwrap();
        }
        let original = cache_key(&root).unwrap();
        fs::write(root.join(INPUTS[0]), b"two").unwrap();
        assert_ne!(original, cache_key(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
