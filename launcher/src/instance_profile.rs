//! Per-instance launch preferences and game-native content switches.
use crate::{
    backend,
    browser::{horizon, installed::Entry, java, Kind, SourceId},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path},
    process::Command,
};

const SETTINGS_FILE: &str = "horizon-instance.json";
const LOG_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub name: Option<String>,
    pub build: Option<String>,
    pub arguments: String,
    pub disabled_ported: BTreeSet<String>,
}

impl Preferences {
    /// Reads this profile's preferences; a new profile uses the defaults.
    pub fn load(home: &Path) -> Result<Self, String> {
        match fs::read(home.join(SETTINGS_FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("Could not read instance preferences: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!("Could not read instance preferences: {error}")),
        }
    }
    /// Replaces the preferences file after the complete JSON has been written.
    pub fn save(&self, home: &Path) -> Result<(), String> {
        fs::create_dir_all(home)
            .map_err(|error| format!("Could not create instance folder: {error}"))?;
        let temporary = home.join(".horizon-instance.json.part");
        fs::write(
            &temporary,
            serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?,
        )
        .and_then(|()| fs::rename(&temporary, home.join(SETTINGS_FILE)))
        .map_err(|error| format!("Could not save instance preferences: {error}"))
    }
    /// Adds this profile's arguments without passing them through a shell.
    pub fn apply(&self, command: &mut Command) {
        command.args(backend::split_args(&self.arguments));
    }
}

/// Copies local resource packs into one profile without replacing existing files.
pub fn add_resource_packs(home: &Path, paths: &[std::path::PathBuf]) -> Result<usize, String> {
    let folder = home.join("resourcepacks");
    let mut names = BTreeSet::new();
    // Validate the entire selection before copying the first file.
    for path in paths {
        if path
            .extension()
            .is_none_or(|extension| !extension.eq_ignore_ascii_case("zip"))
        {
            return Err(
                "Choose .zip resource packs. Use Browse content for Java mods or import a modpack."
                    .into(),
            );
        }
        let name = path.file_name().ok_or("A selected file has no name.")?;
        if !names.insert(name.to_owned()) || folder.join(name).exists() {
            return Err(format!(
                "{} already exists or is selected twice; no file was overwritten.",
                name.to_string_lossy()
            ));
        }
    }
    fs::create_dir_all(&folder)
        .map_err(|error| format!("Could not create resource pack folder: {error}"))?;
    for (completed, path) in paths.iter().enumerate() {
        let name = path.file_name().ok_or("A selected file has no name.")?;
        copy_new_resource_pack(path, &folder.join(name))
            .map_err(|error| format!("{error} {completed} earlier files were added."))?;
    }
    Ok(paths.len())
}

fn copy_new_resource_pack(source: &Path, target: &Path) -> Result<(), String> {
    let mut source_file = fs::File::open(source)
        .map_err(|error| format!("Could not open {}: {error}", source.display()))?;
    let mut destination = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| format!("Could not create {}: {error}", target.display()))?;
    if let Err(error) = std::io::copy(&mut source_file, &mut destination) {
        drop(destination);
        let cleanup = fs::remove_file(target);
        return Err(format!(
            "Could not add {}: {error}. Partial-file cleanup: {cleanup:?}",
            source.display()
        ));
    }
    Ok(())
}

fn entry_id(entry: &Entry) -> Result<&str, String> {
    let prefix = match entry.kind {
        Kind::Mod => "horizon-mods/",
        Kind::ResourcePack => "resourcepacks/",
        Kind::Modpack => return Err("Modpacks are separate instances.".into()),
    };
    let id = entry
        .file
        .strip_prefix(prefix)
        .ok_or_else(|| "The content path does not belong to its declared kind.".to_string())?;
    if id.is_empty()
        || id.contains(['/', '\\', '\n', '\r', ':'])
        || !Path::new(id)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("The content path must be a plain name inside this instance.".into());
    }
    Ok(
        if entry.source == SourceId::Horizon && entry.kind == Kind::Mod {
            id.strip_suffix(".hmod").unwrap_or(id)
        } else {
            id
        },
    )
}

/// Uses the already loaded index; drawing or toggling a row never waits for the network.
pub fn taken_down(entry: &Entry) -> Option<String> {
    if entry.source != SourceId::Horizon {
        return None;
    }
    horizon::known_index().and_then(|index| revoked_reason(entry, &index))
}

fn revoked_reason(entry: &Entry, index: &horizon_mods::index::Index) -> Option<String> {
    let hash = entry
        .checksum
        .strip_prefix("SHA-256:")
        .unwrap_or(&entry.checksum);
    index
        .revoked(&entry.project, hash)
        .map(|revoked| revoked.reason.clone())
}

fn options(home: &Path) -> Result<String, String> {
    match fs::read_to_string(home.join("options.txt")) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("Could not read game options: {error}")),
    }
}

fn resource_packs(text: &str) -> Result<Vec<String>, String> {
    let value = text
        .lines()
        .find_map(|line| line.strip_prefix("resourcePacks:"));
    match value {
        Some(value) => serde_json::from_str(value)
            .map_err(|error| format!("Could not read resource-pack selection: {error}")),
        None => Ok(vec!["vanilla".into()]),
    }
}

fn validate_resource_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.contains(['/', '\\', '\n', '\r', ':'])
        || !Path::new(name)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("Resource packs must have a plain name inside this instance.".into());
    }
    Ok(())
}

pub fn resource_pack_enabled(home: &Path, name: &str) -> Result<bool, String> {
    validate_resource_name(name)?;
    Ok(resource_packs(&options(home)?)?.contains(&format!("file/{name}")))
}

/// Reads the same switches the game reads, without changing files during rendering.
pub fn enabled(home: &Path, entry: &Entry) -> Result<bool, String> {
    let id = entry_id(entry)?;
    if entry.kind == Kind::ResourcePack {
        return resource_pack_enabled(home, id);
    }
    let key = format!("mod.{id}:true");
    match fs::read_to_string(home.join("horizon-mods.txt")) {
        Ok(text) => Ok(text.lines().any(|line| line.trim() == key)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not read mod selection: {error}")),
    }
}

/// Updates game-native options; other packs and unrelated options are preserved.
pub fn set_enabled(home: &Path, entry: &Entry, enabled: bool) -> Result<(), String> {
    let id = entry_id(entry)?;
    if let Some(reason) = taken_down(entry).filter(|_| enabled) {
        return Err(format!("{} was taken down: {reason}", entry.title));
    }
    if entry.kind == Kind::Mod {
        return java::set_enabled(home, id, enabled);
    }
    set_resource_pack_enabled(home, id, enabled)
}

pub fn set_resource_pack_enabled(home: &Path, name: &str, enabled: bool) -> Result<(), String> {
    validate_resource_name(name)?;
    let text = options(home)?;
    let mut packs = resource_packs(&text)?;
    let selected = format!("file/{name}");
    packs.retain(|pack| pack != &selected);
    if enabled {
        packs.push(selected);
    }
    let mut lines = text
        .lines()
        .filter(|line| !line.starts_with("resourcePacks:"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.push(format!(
        "resourcePacks:{}",
        serde_json::to_string(&packs).map_err(|error| error.to_string())?
    ));
    fs::create_dir_all(home)
        .map_err(|error| format!("Could not create instance folder: {error}"))?;
    let temporary = home.join(".options.txt.instance-part");
    fs::write(&temporary, lines.join("\n") + "\n")
        .and_then(|()| fs::rename(temporary, home.join("options.txt")))
        .map_err(|error| format!("Could not save content selection: {error}"))
}

/// A bounded tail: opening Logs never reads an entire large launch log into memory.
pub fn latest_log(home: &Path) -> Result<String, String> {
    let mut file = match fs::File::open(home.join("logs/latest.log")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok("No launch log for this instance yet.".into())
        }
        Err(error) => return Err(format!("Could not open the instance log: {error}")),
    };
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    let start = length.saturating_sub(LOG_BYTES);
    file.seek(SeekFrom::Start(start))
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(LOG_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let lines = text
        .lines()
        .skip(usize::from(start > 0))
        .collect::<Vec<_>>();
    Ok(lines[lines.len().saturating_sub(200)..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry() -> Entry {
        Entry {
            source: crate::browser::SourceId::Modrinth,
            project: "glass".into(),
            slug: "glass".into(),
            title: "Glass".into(),
            kind: Kind::ResourcePack,
            version: "one".into(),
            number: "1".into(),
            file: "resourcepacks/glass.zip".into(),
            checksum: String::new(),
            page: String::new(),
            dependency: false,
            verdict: None,
            skipped: vec![],
            namespaces: vec![],
        }
    }
    #[test]
    fn registry_mod_switches_use_manifest_ids_without_the_hmod_extension() {
        let home =
            std::env::temp_dir().join(format!("horizon-registry-switch-{}", std::process::id()));
        let mut content = entry();
        content.source = SourceId::Horizon;
        content.kind = Kind::Mod;
        content.file = "horizon-mods/coords_hud.hmod".into();
        fs::create_dir_all(&home).unwrap();
        set_enabled(&home, &content, true).unwrap();
        assert!(enabled(&home, &content).unwrap());
        let saved = fs::read_to_string(home.join("horizon-mods.txt")).unwrap();
        assert!(saved.contains("mod.coords_hud:true"));
        assert!(!saved.contains("mod.coords_hud.hmod:"));
        set_enabled(&home, &content, false).unwrap();
        assert!(!enabled(&home, &content).unwrap());
        fs::remove_file(home.join("horizon-mods.txt")).unwrap();
        fs::remove_dir(home).unwrap();
    }

    #[test]
    fn registry_takedowns_match_the_project_or_installed_hash() {
        let hash = "a".repeat(64);
        let index = horizon_mods::index::Index::parse(&format!(
            r#"{{"schema":1,"projects":[],"revoked":[{{"id":"glass","reason":"Removed project"}},{{"id":"other","sha256":"{hash}","reason":"Removed file"}}]}}"#
        )).unwrap();
        let mut content = entry();
        assert_eq!(
            revoked_reason(&content, &index).as_deref(),
            Some("Removed project")
        );
        content.project = "other".into();
        content.checksum = format!("SHA-256:{hash}");
        assert_eq!(
            revoked_reason(&content, &index).as_deref(),
            Some("Removed file")
        );
        content.checksum.clear();
        assert!(revoked_reason(&content, &index).is_none());
    }

    #[test]
    fn content_switches_preserve_other_packs_and_options_and_reject_parent_paths() {
        let home =
            std::env::temp_dir().join(format!("horizon-instance-options-{}", std::process::id()));
        fs::create_dir_all(&home).unwrap();
        fs::write(
            home.join("options.txt"),
            "music:0.5\nresourcePacks:[\"vanilla\",\"file/other.zip\"]\n",
        )
        .unwrap();
        let mut entry = entry();
        set_enabled(&home, &entry, true).unwrap();
        set_enabled(&home, &entry, true).unwrap();
        assert!(enabled(&home, &entry).unwrap());
        let saved = options(&home).unwrap();
        assert!(saved.contains("music:0.5"));
        assert_eq!(
            resource_packs(&saved).unwrap(),
            ["vanilla", "file/other.zip", "file/glass.zip"]
        );
        set_enabled(&home, &entry, false).unwrap();
        assert!(!enabled(&home, &entry).unwrap());
        entry.file = "resourcepacks/../options.txt".into();
        assert!(set_enabled(&home, &entry, true).is_err());
        fs::remove_file(home.join("options.txt")).unwrap();
        fs::remove_dir(home).unwrap();
    }
    #[test]
    fn malformed_pack_selection_is_reported_without_overwriting_options() {
        assert!(resource_packs("resourcePacks:broken\n").is_err());
        assert_eq!(resource_packs("music:0.5\n").unwrap(), ["vanilla"]);
        let preferences = Preferences {
            arguments: "--world \"Two words\"".into(),
            ..Default::default()
        };
        let mut command = Command::new("horizon");
        preferences.apply(&mut command);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--world", "Two words"]
        );
    }

    #[test]
    fn instance_preferences_round_trip_without_affecting_other_profiles() {
        let home = std::env::temp_dir().join(format!(
            "horizon-instance-preferences-{}",
            std::process::id()
        ));
        let preferences = Preferences {
            name: Some("My profile".into()),
            build: Some("v0.2.2".into()),
            arguments: "--server-thread".into(),
            disabled_ported: BTreeSet::from(["copper".into()]),
        };
        preferences.save(&home).unwrap();
        let mut loaded = Preferences::load(&home).unwrap();
        assert!(loaded.disabled_ported.contains("copper"));
        assert_eq!(loaded.build.as_deref(), Some("v0.2.2"));
        loaded.arguments.clear();
        loaded.save(&home).unwrap();
        assert!(Preferences::load(&home).unwrap().arguments.is_empty());
        assert!(Preferences::load(&home.join("other"))
            .unwrap()
            .disabled_ported
            .is_empty());
        fs::remove_file(home.join(SETTINGS_FILE)).unwrap();
        fs::remove_dir(home).unwrap();
    }

    #[test]
    fn adding_resource_packs_validates_the_batch_and_never_overwrites_files() {
        let root =
            std::env::temp_dir().join(format!("horizon-instance-import-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("glass.zip");
        let invalid = root.join("mod.jar");
        let home = root.join("profile");
        fs::write(&source, b"test pack").unwrap();
        fs::write(&invalid, b"test jar").unwrap();
        assert!(add_resource_packs(&home, &[source.clone(), invalid.clone()]).is_err());
        assert!(!home.exists());
        assert!(add_resource_packs(&home, &[source.clone(), source.clone()]).is_err());
        assert_eq!(
            add_resource_packs(&home, std::slice::from_ref(&source)).unwrap(),
            1
        );
        fs::write(&source, b"replacement").unwrap();
        assert!(add_resource_packs(&home, std::slice::from_ref(&source)).is_err());
        assert_eq!(
            fs::read(home.join("resourcepacks/glass.zip")).unwrap(),
            b"test pack"
        );
        fs::remove_file(home.join("resourcepacks/glass.zip")).unwrap();
        fs::remove_dir(home.join("resourcepacks")).unwrap();
        fs::remove_dir(home).unwrap();
        fs::remove_file(source).unwrap();
        fs::remove_file(invalid).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
