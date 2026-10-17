//! Durable cache of immutable face geometry and the per-state mesh tables.
//! The dependency key includes pack and mod overrides of all table inputs.
use horizon_client::{assets::Assets, blocks::BlockFacts, mesher::MeshTables, models};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Instant, UNIX_EPOCH},
};

// Bump whenever the snapshot contents or meshing-table interpretation changes.
const VERSION: u32 = 3;
const MAGIC: &[u8; 4] = b"HMTB";
const HEADER_LEN: usize = 4 + 4 + 32;
const MAX_SNAPSHOT_BYTES: u64 = 256 * 1024 * 1024;
const INPUTS: &[&str] = &[
    "occlusion.nbt",
    "render-definitions.json",
    "facts.json",
    "gameplay-facts.json",
    "block-behaviours.json",
    "block-tags.json",
];

fn cache_key(
    root: &Path,
    sprite_layers: &BTreeMap<String, u32>,
    blocks_fluid_flow: &BTreeSet<String>,
) -> io::Result<String> {
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
    // The pack overlay publishes this marker only after every source and its
    // selection order have been validated. A reused root can change its
    // texture/model bytes without changing the derived mesh inputs.
    let mut marker = root.as_os_str().to_os_string();
    marker.push(".overlay-cache.json");
    match fs::read(PathBuf::from(marker)) {
        Ok(bytes) => {
            hash.update([1]);
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => hash.update([0]),
        Err(e) => return Err(e),
    }
    // Content mods are materialized in a shared root. Their marker identifies
    // enabled content; the stamped inputs below track overrides from each mod.
    match fs::read(root.join("horizon-mod-content.json")) {
        Ok(bytes) => {
            hash.update([1]);
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => hash.update([0]),
        Err(e) => return Err(e),
    }
    // Sprite array layers depend on model definitions and textures. Keying on
    // the resolved mapping catches pack/mod changes without rewalking assets/.
    hash.update((sprite_layers.len() as u64).to_le_bytes());
    for (name, layer) in sprite_layers {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(layer.to_le_bytes());
    }
    // Assets::reload preserves some registry-derived data in memory. Use the
    // actual set passed to MeshTables, regardless of how a pack was reloaded.
    hash.update((blocks_fluid_flow.len() as u64).to_le_bytes());
    for name in blocks_fluid_flow {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn cache_path(root: &Path, assets: &Assets) -> io::Result<PathBuf> {
    let key = cache_key(root, &assets.sprite_layers, &assets.blocks_fluid_flow)?;
    Ok(std::env::current_dir()?
        .join(".horizon-cache/pack-render")
        .join(format!("{key}.bin")))
}

fn read_snapshot(path: &Path, expected_states: usize) -> horizon_client::Result<MeshTables> {
    if fs::metadata(path)?.len() > MAX_SNAPSHOT_BYTES {
        return Err("mesh snapshot exceeds size limit".into());
    }
    let bytes = fs::read(path)?;
    if bytes.len() < HEADER_LEN
        || &bytes[..4] != MAGIC
        || bytes[4..8] != VERSION.to_le_bytes()
        || Sha256::digest(&bytes[HEADER_LEN..]).as_slice() != &bytes[8..HEADER_LEN]
    {
        return Err("invalid mesh snapshot checksum".into());
    }
    MeshTables::from_cache_bytes(&bytes[HEADER_LEN..], expected_states)
}

fn write_snapshot(path: &Path, tables: &MeshTables) -> horizon_client::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let bytes = tables.to_cache_bytes()?;
    let mut signed = MAGIC.to_vec();
    signed.extend_from_slice(&VERSION.to_le_bytes());
    signed.extend_from_slice(&Sha256::digest(&bytes));
    signed.extend_from_slice(&bytes);
    if signed.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err("mesh snapshot exceeds size limit".into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Atomic publication prevents consumers from seeing an incomplete file.
    let temporary = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    use std::io::Write;
    let result = file.write_all(&signed).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    let published = fs::rename(&temporary, path);
    // Windows does not replace an existing file with rename. Recover an
    // invalid older snapshot; a crash between removal and rename is a miss.
    #[cfg(windows)]
    let published = published.or_else(|e| {
        if path.exists() {
            fs::remove_file(path)?;
            fs::rename(&temporary, path)
        } else {
            Err(e)
        }
    });
    if let Err(e) = published {
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
        cache_path(root, assets).ok()
    };
    if let Some(path) = &path
        && let Ok(tables) = read_snapshot(path, facts.states.len())
        && cache_path(root, assets).ok().as_ref() == Some(path)
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
    if let Some(path) = path {
        // In-place pack updates while the tables are being built must never
        // publish a snapshot under an outdated dependency key.
        if cache_path(root, assets).ok().as_ref() == Some(&path) {
            if let Err(e) = write_snapshot(&path, &tables) {
                log::warn!("Horizon: could not write mesh table cache: {e}");
            }
        } else {
            log::warn!("Horizon: mesh table inputs changed during cache build");
        }
    }
    Ok(tables)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn temporary_root() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "pack-render-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
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
        let fluid = BTreeSet::new();
        let original = cache_key(&root, &layers, &fluid).unwrap();
        for name in INPUTS {
            fs::write(root.join(name), b"two").unwrap();
            assert_ne!(
                original,
                cache_key(&root, &layers, &fluid).unwrap(),
                "{name}"
            );
            fs::write(root.join(name), b"one").unwrap();
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sprite_layer_remapping_invalidates() {
        let root = temporary_root();
        let mut layers = BTreeMap::from([("minecraft:block/stone".to_owned(), 1)]);
        let fluid = BTreeSet::new();
        let original = cache_key(&root, &layers, &fluid).unwrap();
        layers.insert("minecraft:block/stone".to_owned(), 2);
        assert_ne!(original, cache_key(&root, &layers, &fluid).unwrap());
        let changed = cache_key(&root, &layers, &fluid).unwrap();
        let mut fluid = fluid;
        fluid.insert("minecraft:stone".into());
        assert_ne!(changed, cache_key(&root, &layers, &fluid).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overlay_and_mod_markers_invalidate() {
        let root = temporary_root();
        let layers = BTreeMap::new();
        let flow = BTreeSet::new();
        let initial = cache_key(&root, &layers, &flow).unwrap();
        let marker = PathBuf::from(format!("{}.overlay-cache.json", root.display()));
        fs::write(&marker, b"pack-A").unwrap();
        let pack_a = cache_key(&root, &layers, &flow).unwrap();
        assert_ne!(pack_a, initial);
        fs::write(&marker, b"pack-B").unwrap();
        assert_ne!(pack_a, cache_key(&root, &layers, &flow).unwrap());
        fs::write(root.join("horizon-mod-content.json"), b"[\"mod-a\"]").unwrap();
        let mod_a = cache_key(&root, &layers, &flow).unwrap();
        fs::write(root.join("horizon-mod-content.json"), b"[\"mod-b\"]").unwrap();
        assert_ne!(mod_a, cache_key(&root, &layers, &flow).unwrap());
        fs::remove_file(&marker).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn damaged_snapshot_is_rejected() {
        let root = temporary_root();
        let path = root.join("bad.bin");
        fs::write(&path, b"broken").unwrap();
        assert!(read_snapshot(&path, 1).is_err());
        // Even a complete payload with its original checksum is not valid
        // across a cache-format change.
        let mut obsolete = MAGIC.to_vec();
        obsolete.extend_from_slice(&(VERSION - 1).to_le_bytes());
        obsolete.extend_from_slice(&Sha256::digest(b"payload"));
        obsolete.extend_from_slice(b"payload");
        fs::write(&path, obsolete).unwrap();
        assert!(read_snapshot(&path, 1).is_err());
        // An interrupted temporary file is never a published cache entry.
        fs::write(path.with_extension("tmp"), b"partial").unwrap();
        assert!(read_snapshot(&path, 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn actual_pack_add_remove_reorder_edit_and_mod_override() {
        use horizon_client::{
            mod_content::{self, ContentMod},
            packs,
        };
        use horizon_mods::{Content, Package};

        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/horizon-client/26.3");
        if !base.join("horizon-resources.json").is_file() {
            return;
        }
        let scratch = temporary_root();
        let packs_dir = scratch.join("resourcepacks");
        let model = "assets/minecraft/models/block/stone.json";
        let write = |name: &str, path: &str, bytes: &[u8]| {
            let at = scratch.join(name).join(path);
            fs::create_dir_all(at.parent().unwrap()).unwrap();
            fs::write(at, bytes).unwrap();
        };
        for (name, texture) in [("resourcepacks/a", "dirt"), ("resourcepacks/b", "granite")] {
            write(
                name,
                "pack.mcmeta",
                br#"{"pack":{"description":"test","pack_format":97}}"#,
            );
            write(
                name,
                model,
                format!(r#"{{"parent":"minecraft:block/cube_all","textures":{{"all":"minecraft:block/{texture}"}}}}"#)
                    .as_bytes(),
            );
        }
        let all = packs::discover(&base, &packs_dir);
        let a = all.iter().find(|p| p.id == "file/a").unwrap().clone();
        let b = all.iter().find(|p| p.id == "file/b").unwrap().clone();
        let overlay_dir = scratch.join("overlays");
        let root_a = packs::resource_root(&base, std::slice::from_ref(&a), &overlay_dir);
        let root_ab = packs::resource_root(&base, &[a.clone(), b.clone()], &overlay_dir);
        let root_ba = packs::resource_root(&base, &[b.clone(), a.clone()], &overlay_dir);
        let root_b = packs::resource_root(&base, std::slice::from_ref(&b), &overlay_dir);
        for root in [&root_a, &root_ab, &root_ba, &root_b] {
            assert_ne!(root, &base, "pack overlay must load");
        }
        let layers = BTreeMap::new();
        let flow = BTreeSet::new();
        let key = |root: &Path| cache_key(root, &layers, &flow).unwrap();
        let (ka, kab, kba, kb, kbase) = (
            key(&root_a),
            key(&root_ab),
            key(&root_ba),
            key(&root_b),
            key(&base),
        );
        let unique: BTreeSet<_> = [ka.clone(), kab, kba, kb, kbase].into_iter().collect();
        assert_eq!(
            unique.len(),
            5,
            "selection and order must produce distinct cache keys"
        );

        // A live folder pack changes while the same overlay directory is reused.
        write(
            "resourcepacks/a",
            model,
            br#"{"parent":"minecraft:block/cube_all","textures":{"all":"minecraft:block/andesite"}}"#,
        );
        let rebuilt = packs::resource_root(&base, std::slice::from_ref(&a), &overlay_dir);
        assert_eq!(rebuilt, root_a);
        assert_ne!(ka, key(&rebuilt), "pack file edit must invalidate");
        assert_eq!(
            fs::read(rebuilt.join(model)).unwrap(),
            fs::read(scratch.join("resourcepacks/a").join(model)).unwrap(),
        );

        // A content mod overrides a tag in the actual resources. Mod overlays
        // share one root regardless of the enabled mod IDs.
        let content_mod = |id: &str, block: &str| {
            let dir = format!("mods/{id}");
            write(
                &dir,
                "horizon-mod.toml",
                format!("id = \"{id}\"\nname = \"{id}\"\nversion = \"0.1.0\"\napi = \"0.2\"\nside = \"both\"\ncapabilities = [\"registries\"]\n").as_bytes(),
            );
            write(
                &dir,
                "data/minecraft/tags/block/blocks_fluid_flow.json",
                format!(r#"{{"values":["minecraft:{block}"]}}"#).as_bytes(),
            );
            let dir = scratch.join(dir);
            let package = Package::read(&dir).unwrap();
            ContentMod {
                id: id.into(),
                content: Content::read(&dir, &package.manifest).unwrap(),
            }
        };
        let one = content_mod("flow_stone", "stone");
        let two = content_mod("flow_dirt", "dirt");
        let mod_cache = scratch.join("mod-cache");
        let mod_root = mod_content::resource_root(&root_a, &[one], &mod_cache).unwrap();
        let mod_key = key(&mod_root);
        let mod_root_other = mod_content::resource_root(&root_a, &[two], &mod_cache).unwrap();
        assert_eq!(mod_root, mod_root_other);
        assert_ne!(
            mod_key,
            key(&mod_root_other),
            "mod selection must invalidate"
        );
        // A real content-mod snapshot must survive an abandoned temporary
        // write and regenerate cleanly after its published bytes are damaged.
        let assets = Assets::load(&mod_root_other).unwrap();
        let facts = BlockFacts::load(&mod_root_other, assets.registry).unwrap();
        let visibility = models::load_visibility(&mod_root_other, assets.registry).unwrap();
        let fresh = MeshTables::new(&assets, &facts, visibility).unwrap();
        let original = fresh.to_cache_bytes().unwrap();
        let snapshot = scratch.join("tested.bin");
        write_snapshot(&snapshot, &fresh).unwrap();
        let restored_bytes = || {
            read_snapshot(&snapshot, facts.states.len())
                .unwrap()
                .to_cache_bytes()
                .unwrap()
        };
        assert_eq!(restored_bytes(), original);
        fs::write(snapshot.with_extension("999.tmp"), b"interrupted").unwrap();
        assert_eq!(restored_bytes(), original);
        let mut damaged = fs::read(&snapshot).unwrap();
        *damaged.last_mut().unwrap() ^= 1;
        fs::write(&snapshot, damaged).unwrap();
        assert!(read_snapshot(&snapshot, facts.states.len()).is_err());
        write_snapshot(&snapshot, &fresh).unwrap();
        assert_eq!(restored_bytes(), original);
        let restored = mod_content::resource_root(&root_a, &[], &mod_cache).unwrap();
        assert_eq!(
            restored, root_a,
            "removing all mods must restore the pack root"
        );
        fs::remove_dir_all(scratch).unwrap();
    }
}
