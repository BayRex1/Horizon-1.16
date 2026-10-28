//! Opt-in inventory feed for the sandboxed Mod Menu. Contains display metadata only.
use crate::browser::{installed::Installed, java};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::{collections::HashMap, fs, path::Path};

const MENU: &str = "mod_menu";
const NATIVE_DIR: &str = "horizon-native-mods";

#[derive(Deserialize)]
struct NativeMetadata {
    id: String,
    version: String,
}

fn native_metadata(path: &Path) -> Result<NativeMetadata, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    zip.by_name("horizon-native.json")
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("native metadata exceeds 64 KiB".into());
    }
    let m: NativeMetadata = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !horizon_mods::index::valid_id(&m.id) {
        return Err("invalid native mod id".into());
    }
    Ok(m)
}

#[derive(Debug, Serialize)]
struct Entry {
    id: String,
    name: String,
    version: String,
    kind: &'static str,
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<String>,
}

fn collect(home: &Path) -> (bool, Vec<Entry>) {
    let installed = Installed::load(home);
    let names: HashMap<_, _> = installed
        .entries
        .iter()
        .map(|m| (m.project.as_str(), m.title.as_str()))
        .collect();
    let mut found = Vec::new();
    let mut has_menu = false;
    for path in horizon_mods::find(&home.join(java::MODS_DIR)) {
        let file = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let entry = match horizon_mods::Package::read(&path) {
            Ok(package) => {
                let m = package.manifest;
                has_menu |= m.id == MENU;
                let kind = if package.wasm.is_some() {
                    "Sandboxed"
                } else {
                    "Content"
                };
                Entry {
                    enabled: java::is_enabled(home, &m.id),
                    id: m.id,
                    name: m.name,
                    version: m.version,
                    kind,
                    problem: None,
                }
            }
            Err(error) => Entry {
                id: file.clone(),
                name: file,
                version: String::new(),
                kind: "Sandboxed",
                enabled: false,
                problem: Some(error.chars().take(256).collect()),
            },
        };
        found.push(entry);
    }
    if let Ok(files) = fs::read_dir(home.join(NATIVE_DIR)) {
        for file in files
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "hnmod"))
        {
            let stem = file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let entry = match native_metadata(&file) {
                Ok(bundle) => {
                    let m = bundle;
                    Entry {
                        enabled: java::is_enabled(home, &m.id),
                        name: names
                            .get(m.id.as_str())
                            .copied()
                            .unwrap_or(&m.id)
                            .to_owned(),
                        id: m.id,
                        version: m.version,
                        kind: "Native",
                        problem: None,
                    }
                }
                Err(error) => Entry {
                    id: stem.clone(),
                    name: stem,
                    version: String::new(),
                    kind: "Native",
                    enabled: false,
                    problem: Some(error.chars().take(256).collect()),
                },
            };
            found.push(entry);
        }
    }
    found.sort_by_key(|m| (m.name.to_lowercase(), m.id.clone()));
    (has_menu, found)
}

/// Called after launcher changes and just before starting the selected instance.
/// No feed or directory is created unless Mod Menu itself is installed.
pub fn refresh(home: &Path) {
    if let Err(error) = write(home) {
        eprintln!("Mod Menu inventory: {error}");
    }
}

fn write(home: &Path) -> Result<(), String> {
    let (has_menu, mods) = collect(home);
    if !has_menu {
        return Ok(());
    }
    let bytes = serde_json::to_vec(&serde_json::json!({ "schema": 1, "mods": mods }))
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1 << 20 {
        return Err("inventory exceeds 1 MiB".into());
    }
    let dir = home
        .join(java::MODS_DIR)
        .join(horizon_mods::manifest::STORAGE_DIR)
        .join(MENU);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let part = dir.join(".inventory.json.part");
    fs::write(&part, bytes).map_err(|e| e.to_string())?;
    fs::rename(part, dir.join("inventory.json")).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn scans_manual_mods_and_native_sources_and_keeps_disabled_mods_visible() {
        let home = std::env::temp_dir().join(format!("mod-menu-inventory-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join(java::MODS_DIR)).unwrap();
        write(&home).unwrap();
        assert!(!home
            .join(java::MODS_DIR)
            .join(horizon_mods::manifest::STORAGE_DIR)
            .exists());
        for (id, name) in [(MENU, "Mod Menu"), ("disabled", "Disabled mod")] {
            let manifest = format!("id = \"{id}\"\nname = \"{name}\"\nversion = \"1.0.0\"\napi = \"0.1\"\nside = \"client\"\n");
            let bytes =
                horizon_mods::Package::write(&manifest, b"fake component for metadata inspection")
                    .unwrap();
            fs::write(home.join(java::MODS_DIR).join(format!("{id}.hmod")), bytes).unwrap();
        }
        let native = home.join(NATIVE_DIR);
        fs::create_dir(&native).unwrap();
        let m = serde_json::json!({ "schema":1, "id":"veinminer", "version":"0.3.0", "horizon":"a".repeat(40), "mixins":"b".repeat(40) });
        let mut zip =
            zip::ZipWriter::new(fs::File::create(native.join("veinminer.hnmod")).unwrap());
        zip.start_file(
            "horizon-native.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&serde_json::to_vec(&m).unwrap()).unwrap();
        zip.start_file(
            "mixins/veinminer/horizon-mixin.toml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(format!("[mod]\nid = \"veinminer\"\nversion = \"0.3.0\"\n[compatibility]\nhorizon_revision = \"{}\"\n",m["horizon"].as_str().unwrap()).as_bytes()).unwrap();
        zip.finish().unwrap();
        fs::write(home.join(java::MODS_DIR).join("broken.hmod"), b"invalid").unwrap();
        java::set_enabled(&home, MENU, true).unwrap();
        java::set_enabled(&home, "veinminer", true).unwrap();
        write(&home).unwrap();
        let path = home
            .join(java::MODS_DIR)
            .join(horizon_mods::manifest::STORAGE_DIR)
            .join(MENU)
            .join("inventory.json");
        let snapshot: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        let rows = snapshot["mods"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        let by_id = |id| rows.iter().find(|r| r["id"] == id).unwrap();
        assert_eq!(by_id("veinminer")["kind"], "Native");
        assert_eq!(by_id("veinminer")["enabled"], true);
        assert_eq!(by_id("disabled")["enabled"], false);
        assert!(by_id("broken")["problem"].is_string());
        assert!(!snapshot.to_string().contains(home.to_str().unwrap()));
        fs::remove_dir_all(home).unwrap();
    }
}
