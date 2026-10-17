//! Durable cache of immutable face geometry and the per-state mesh tables.
//! The dependency key includes pack and mod overrides of all table inputs.
use horizon_client::{assets::Assets, blocks::BlockFacts, mesher::MeshTables, models};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::{Instant, UNIX_EPOCH},
};

// Bump whenever the snapshot contents or meshing-table interpretation changes.
const VERSION: u32 = 2;
const INPUTS: &[&str] = &[
    "occlusion.nbt",
    "render-definitions.json",
    "facts.json",
    "gameplay-facts.json",
    "block-behaviours.json",
    "block-tags.json",
];

fn cache_key(root: &Path, sprite_layers: &BTreeMap<String, u32>) -> io::Result<String> {
    let mut hash = Sha256::new();
    hash.update(VERSION.to_le_bytes());
    hash.update(fs::canonicalize(root)?.as_os_str().as_encoded_bytes());
    for name in INPUTS {
        let path = root.join(name);
        let metadata = fs::metadata(&path)?;
        hash.update((name.len() as u64).to_le_bytes());
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
            // On platforms without a change timestamp hash source bytes, so
            // rewrites with preserved length and mtime still invalidate.
            hash.update(Sha256::digest(fs::read(path)?));
        }
    }
    // Sprite array layers depend on model definitions and textures. Keying on
    // the resolved mapping catches pack/mod changes without rewalking assets/.
    hash.update((sprite_layers.len() as u64).to_le_bytes());
    for (name, layer) in sprite_layers {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(layer.to_le_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn cache_path(root: &Path, sprite_layers: &BTreeMap<String, u32>) -> io::Result<PathBuf> {
    let key = cache_key(root, sprite_layers)?;
    Ok(std::env::current_dir()?
        .join(".horizon-cache/pack-render")
        .join(format!("{key}.bin")))
}

fn read_snapshot(path: &Path, expected_states: usize) -> horizon_client::Result<MeshTables> {
    let bytes = fs::read(path)?;
    if bytes.len() < 32 || Sha256::digest(&bytes[32..]).as_slice() != &bytes[..32] {
        return Err("invalid mesh snapshot checksum".into());
    }
    MeshTables::from_cache_bytes(&bytes[32..], expected_states)
}

fn write_snapshot(path: &Path, tables: &MeshTables) -> horizon_client::Result<()> {
    let bytes = tables.to_cache_bytes()?;
    let mut signed = Sha256::digest(&bytes).to_vec();
    signed.extend_from_slice(&bytes);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Atomic publication prevents consumers from seeing an incomplete file.
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, signed)?;
    if let Err(e) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(e.into());
    }
    Ok(())
}

pub(super) fn load_tables(
    root: &Path,
    assets: &Assets,
    facts: &BlockFacts,
) -> horizon_client::Result<MeshTables> {
    let start = Instant::now();
    // Measure the original loader under precisely the same executable.
    let disabled = std::env::var_os("HORIZON_DISABLE_PACK_RENDER_CACHE").is_some();
    let path = if disabled {
        None
    } else {
        cache_path(root, &assets.sprite_layers).ok()
    };
    if let Some(path) = &path
        && let Ok(tables) = read_snapshot(path, facts.states.len())
    {
        log::info!(
            "Horizon: cached pack mesh tables in {:.2}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        return Ok(tables);
    }
    let visibility = models::load_visibility(root, assets.registry)?;
    let tables = MeshTables::new(assets, facts, visibility)?;
    log::info!(
        "Horizon: built pack mesh tables in {:.2}ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    if let Some(path) = path
        && let Err(e) = write_snapshot(&path, &tables)
    {
        log::warn!("Horizon: could not write mesh table cache: {e}");
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temporary_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "pack-render-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        fs::create_dir_all(&root).unwrap();
        for name in INPUTS {
            fs::write(root.join(name), b"one").unwrap();
        }
        root
    }

    #[test]
    fn source_stamp_invalidates_on_change() {
        let root = temporary_root();
        let layers = BTreeMap::new();
        let original = cache_key(&root, &layers).unwrap();
        for name in INPUTS {
            fs::write(root.join(name), b"two").unwrap();
            assert_ne!(original, cache_key(&root, &layers).unwrap(), "{name}");
            fs::write(root.join(name), b"one").unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sprite_layer_remapping_invalidates() {
        let root = temporary_root();
        let mut layers = BTreeMap::from([("minecraft:block/stone".to_owned(), 1)]);
        let original = cache_key(&root, &layers).unwrap();
        layers.insert("minecraft:block/stone".to_owned(), 2);
        assert_ne!(original, cache_key(&root, &layers).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn damaged_snapshot_is_rejected() {
        let root = temporary_root();
        let path = root.join("bad.bin");
        fs::write(&path, b"broken").unwrap();
        assert!(read_snapshot(&path, 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
