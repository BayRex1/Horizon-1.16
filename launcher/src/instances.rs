//! Local instance folders, separate from the default game home and imported modpacks.
use crate::instance_profile::Preferences;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const MAX_NAME_LENGTH: usize = 80;
const MAX_INSTANCE_FOLDERS: usize = 10_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum InstanceId {
    #[default]
    Horizon,
    Imported(String),
    Registry(String),
    Local(String),
}

impl InstanceId {
    /// Returns a pack key only for imported instances.
    pub fn imported_slug(&self) -> Option<&String> {
        match self {
            Self::Imported(slug) => Some(slug),
            _ => None,
        }
    }

    pub fn registry_id(&self) -> Option<&String> {
        match self {
            Self::Registry(id) => Some(id),
            _ => None,
        }
    }

    /// Stable, distinct identity for native UI controls.
    pub fn key(&self) -> String {
        match self {
            Self::Horizon => "horizon".into(),
            Self::Imported(slug) => format!("imported-{slug}"),
            Self::Registry(id) => format!("registry-{id}"),
            Self::Local(slug) => format!("local-{slug}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalInstance {
    pub slug: String,
    pub home: PathBuf,
    pub preferences: Preferences,
}

/// Reads only instance folders with a valid preferences file.
pub fn list(game_home: &Path) -> Result<Vec<LocalInstance>, String> {
    let root = game_home.join("instances");
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(format!("Could not list instances: {error}")),
    };
    let mut instances = vec![];
    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not read an instance folder: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
            || !entry.path().join("horizon-instance.json").is_file()
        {
            continue;
        }
        let preferences = Preferences::load(&entry.path())?;
        instances.push(LocalInstance {
            slug: entry.file_name().to_string_lossy().into_owned(),
            home: entry.path(),
            preferences,
        });
    }
    instances.sort_by_key(|instance| {
        instance
            .preferences
            .name
            .clone()
            .unwrap_or_default()
            .to_lowercase()
    });
    Ok(instances)
}

/// Creates an empty, independent profile. Names never become filesystem paths.
pub fn create(
    game_home: &Path,
    name: &str,
    build: Option<String>,
) -> Result<LocalInstance, String> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_LENGTH
        || name.chars().any(char::is_control)
    {
        return Err("Use a name between 1 and 80 characters, without line breaks.".into());
    }
    let root = game_home.join("instances");
    fs::create_dir_all(&root)
        .map_err(|error| format!("Could not create the instances folder: {error}"))?;
    for number in 1..=MAX_INSTANCE_FOLDERS {
        let slug = format!("instance-{number}");
        let home = root.join(&slug);
        match fs::create_dir(&home) {
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Could not create instance: {error}")),
            Ok(()) => {}
        }
        let preferences = Preferences {
            name: Some(name.into()),
            build,
            ..Default::default()
        };
        preferences.save(&home)?;
        return Ok(LocalInstance {
            slug,
            home,
            preferences,
        });
    }
    Err("There are no free instance folders. Open your instances folder to check it.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_separate_profiles_and_preserves_names_as_text() {
        let root =
            std::env::temp_dir().join(format!("horizon-local-instances-{}", std::process::id()));
        assert!(create(&root, "  ", None).is_err());
        assert!(create(&root, "bad\nname", None).is_err());
        assert!(create(&root, &"a".repeat(MAX_NAME_LENGTH + 1), None).is_err());
        let first = create(&root, "../My survival", Some("v1".into())).unwrap();
        let second = create(&root, "../My survival", None).unwrap();
        assert_ne!(first.home, second.home);
        assert_eq!(first.home.parent(), Some(root.join("instances").as_path()));
        let loaded = list(&root).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded[0].preferences.name.as_deref(),
            Some("../My survival")
        );
        assert_eq!(loaded[0].preferences.build.as_deref(), Some("v1"));
        for instance in loaded {
            fs::remove_file(instance.home.join("horizon-instance.json")).unwrap();
            fs::remove_dir(instance.home).unwrap();
        }
        fs::remove_dir(root.join("instances")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
