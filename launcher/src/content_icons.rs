//! Resolve real artwork on worker threads; archive contents never leave the image cache.
use crate::browser::{self, images, SourceId};
use horizon_port_pack::{import::Pack, modrinth::Cache};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

const METADATA_LIMIT: u64 = 256 * 1024;
const IMPORT_ART: &str = "launcher-art.json";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Url(String, u32),
    Project(SourceId, String),
    Archive(PathBuf),
    ImportedItem {
        work: PathBuf,
        key: String,
    },
    Instance {
        home: PathBuf,
        work: Option<PathBuf>,
        build: Option<String>,
    },
}

#[derive(Default, Deserialize, Serialize)]
struct ImportArt {
    source: Option<PathBuf>,
    icon: Option<String>,
}

/// Retain the original instance/archive location for its own artwork.
pub fn remember_import(work: &Path, source: &Path) -> Result<(), String> {
    let mut art = import_art(work);
    art.source = Some(fs::canonicalize(source).map_err(|error| error.to_string())?);
    save_import_art(work, &art)
}

/// Preserve the browser project's actual icon when importing a downloaded modpack.
pub fn remember_project_icon(work: &Path, icon: Option<&str>) -> Result<(), String> {
    let mut art = import_art(work);
    art.icon = icon.map(str::to_owned);
    save_import_art(work, &art)
}

fn import_art(work: &Path) -> ImportArt {
    fs::read(work.join(IMPORT_ART))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_import_art(work: &Path, art: &ImportArt) -> Result<(), String> {
    fs::write(
        work.join(IMPORT_ART),
        serde_json::to_vec_pretty(art).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("Could not save instance artwork metadata: {error}"))
}

#[derive(Default)]
struct ImportedIcons {
    items: HashMap<String, Vec<Icon>>,
    original: Option<PathBuf>,
    icon: Option<String>,
    modified: Option<std::time::SystemTime>,
}

#[derive(Deserialize)]
struct IconRecord {
    key: String,
    path: String,
    #[serde(default)]
    jar: Option<PathBuf>,
}

/// One resolver per artwork worker; large import metadata is loaded once per work folder.
pub struct Resolver {
    home: PathBuf,
    imports: HashMap<PathBuf, ImportedIcons>,
}

impl Resolver {
    pub fn new(home: PathBuf) -> Self {
        Self {
            home,
            imports: HashMap::new(),
        }
    }

    pub fn resolve(&mut self, icon: &Icon) -> Result<Option<PathBuf>, String> {
        match icon {
            Icon::Url(url, size) => images::fetch(&self.home, url, *size).map(Some),
            Icon::Project(source, id) => {
                let Some(source) = browser::source(*source) else {
                    return Ok(None);
                };
                let project = source.project(id)?;
                project
                    .icon
                    .map(|url| images::fetch(&self.home, &url, images::ICON))
                    .transpose()
            }
            Icon::Archive(path) => self.archive(path),
            Icon::ImportedItem { work, key } => {
                self.ensure_import(work)?;
                let candidates = self.imports[work]
                    .items
                    .get(key)
                    .cloned()
                    .unwrap_or_default();
                self.first_available(&candidates)
            }
            Icon::Instance { home, work, build } => {
                let resources = build.as_deref().map(crate::backend::resources);
                self.instance(home, work.as_deref(), resources.as_deref())
            }
        }
    }

    fn ensure_import(&mut self, work: &Path) -> Result<(), String> {
        let modified = fs::metadata(work.join("inventory.json"))
            .and_then(|metadata| metadata.modified())
            .ok();
        if self
            .imports
            .get(work)
            .is_some_and(|icons| icons.modified == modified)
        {
            return Ok(());
        }
        let pack: Pack = read_json(&work.join("pack.json"))?.unwrap_or_default();
        let records: Vec<IconRecord> = read_json(&work.join("inventory.json"))?.unwrap_or_default();
        let cache = Cache::load(&work.join("modrinth.json"));
        let files = pack
            .files
            .iter()
            .map(|file| (file.path.as_str(), file))
            .collect::<HashMap<_, _>>();
        let art = import_art(work);
        let mut imported = ImportedIcons {
            original: art.source,
            icon: art.icon,
            modified,
            ..Default::default()
        };
        for record in records {
            let mut candidates = vec![];
            if let Some(jar) = record.jar {
                candidates.push(Icon::Archive(jar));
            }
            if let Some(file) = files.get(record.path.as_str()) {
                if imported.original.is_none() {
                    imported.original = file
                        .jar
                        .as_deref()
                        .and_then(Path::parent)
                        .and_then(Path::parent)
                        .map(Path::to_path_buf);
                }
                if let Some(url) = file
                    .sha1
                    .as_deref()
                    .and_then(|hash| cache.project_of(hash))
                    .and_then(|project| project["icon_url"].as_str())
                {
                    candidates.push(Icon::Url(url.to_owned(), images::ICON));
                }
            }
            imported.items.insert(record.key, candidates);
        }
        self.imports.insert(work.to_path_buf(), imported);
        Ok(())
    }

    fn first_available(&mut self, candidates: &[Icon]) -> Result<Option<PathBuf>, String> {
        let mut last_error = None;
        for candidate in candidates {
            match self.resolve(candidate) {
                Ok(Some(path)) => return Ok(Some(path)),
                Ok(None) => {}
                Err(error) => last_error = Some(error),
            }
        }
        match last_error {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }

    fn instance(
        &mut self,
        home: &Path,
        work: Option<&Path>,
        resources: Option<&Path>,
    ) -> Result<Option<PathBuf>, String> {
        if let Some(path) = self.folder_icon(home)? {
            return Ok(Some(path));
        }
        if let Some(work) = work {
            if let Some(path) = self.folder_icon(work)? {
                return Ok(Some(path));
            }
            self.ensure_import(work)?;
            let original = self.imports[work].original.clone();
            let url = self.imports[work].icon.clone();
            if let Some(original) = original {
                let local = if original.is_dir() {
                    self.folder_icon(&original)?
                } else {
                    self.archive(&original)?
                };
                if local.is_some() {
                    return Ok(local);
                }
            }
            return url
                .map(|url| images::fetch(&self.home, &url, images::ICON))
                .transpose();
        }
        Ok(resources.and_then(|resources| images::grass_block(&self.home, resources)))
    }

    fn folder_icon(&self, folder: &Path) -> Result<Option<PathBuf>, String> {
        for name in [
            "icon.png",
            "instance.png",
            "pack.png",
            ".minecraft/icon.png",
            "minecraft/icon.png",
        ] {
            let path = folder.join(name);
            if path.is_file() {
                let bytes = read_limited(
                    fs::File::open(&path).map_err(|error| error.to_string())?,
                    images::MAX_DOWNLOAD,
                )?;
                return self.store_local(&path, name, &bytes).map(Some);
            }
        }
        if let Some(profile) =
            read_json::<serde_json::Value>(&folder.join("minecraftinstance.json"))?
        {
            if let Some(path) = profile["profileImagePath"].as_str() {
                let path = folder.join(path);
                if path.is_file() {
                    let bytes = read_limited(
                        fs::File::open(&path).map_err(|error| error.to_string())?,
                        images::MAX_DOWNLOAD,
                    )?;
                    return self
                        .store_local(&path, "profileImagePath", &bytes)
                        .map(Some);
                }
            }
            if let Some(url) = profile["installedModpack"]["thumbnailUrl"]
                .as_str()
                .or_else(|| profile["profileImageUrl"].as_str())
            {
                return images::fetch(&self.home, url, images::ICON).map(Some);
            }
        }
        if let Ok(settings) = fs::read_to_string(folder.join("instance.cfg")) {
            if let Some(key) = settings
                .lines()
                .find_map(|line| line.strip_prefix("iconKey="))
            {
                if plain_archive_path(key) && !key.contains('/') {
                    if let Some(root) = folder.parent().and_then(Path::parent) {
                        let path = root.join("icons").join(format!("{key}.png"));
                        if path.is_file() {
                            let bytes = read_limited(
                                fs::File::open(&path).map_err(|error| error.to_string())?,
                                images::MAX_DOWNLOAD,
                            )?;
                            return self.store_local(&path, "prism-icon", &bytes).map(Some);
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    fn store_local(&self, path: &Path, entry: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        let modified = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok();
        let key = format!("local:{}:{entry}:{modified:?}", path.display());
        if let Some(cached) = images::cached(&self.home, &key, images::ICON) {
            return Ok(cached);
        }
        images::store(&self.home, &key, images::ICON, bytes)
    }

    fn archive(&self, path: &Path) -> Result<Option<PathBuf>, String> {
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "Could not open artwork archive {}: {error}",
                    path.display()
                ))
            }
        };
        let mut archive = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
        let mut names = archive_icon_names(&mut archive)?;
        names.extend(
            [
                "pack.png",
                "icon.png",
                "logo.png",
                "overrides/icon.png",
                "overrides/pack.png",
            ]
            .map(str::to_owned),
        );
        for name in names {
            if !plain_archive_path(&name) {
                continue;
            }
            let entry = match archive.by_name(&name) {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let bytes = read_limited(entry, images::MAX_DOWNLOAD)?;
            return self.store_local(path, &name, &bytes).map(Some);
        }
        Ok(None)
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("{}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = vec![];
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Artwork or its metadata is too large.".into());
    }
    Ok(bytes)
}

fn plain_archive_path(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':'])
        && Path::new(name)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn archive_icon_names(archive: &mut zip::ZipArchive<fs::File>) -> Result<Vec<String>, String> {
    let mut names = vec![];
    if let Ok(entry) = archive.by_name("fabric.mod.json") {
        let metadata: serde_json::Value =
            serde_json::from_slice(&read_limited(entry, METADATA_LIMIT)?)
                .map_err(|error| error.to_string())?;
        match &metadata["icon"] {
            serde_json::Value::String(name) => names.push(name.clone()),
            serde_json::Value::Object(sizes) => names.extend(
                sizes
                    .values()
                    .filter_map(|value| value.as_str().map(str::to_owned)),
            ),
            _ => {}
        }
    }
    for metadata in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Ok(entry) = archive.by_name(metadata) {
            let bytes = read_limited(entry, METADATA_LIMIT)?;
            for line in String::from_utf8_lossy(&bytes).lines() {
                if let Some(value) = line
                    .trim()
                    .strip_prefix("logoFile")
                    .and_then(|rest| rest.trim_start().strip_prefix('='))
                {
                    let value = value.trim();
                    if let Some(quote @ ('\'' | '"')) = value.chars().next() {
                        if let Some(name) = value[1..].split(quote).next() {
                            names.push(name.to_owned());
                        }
                    }
                }
            }
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn png() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(4, 4, image::Rgba([42, 100, 180, 255]));
        let mut bytes = std::io::Cursor::new(vec![]);
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    fn archive(path: &Path, metadata: Option<(&str, &[u8])>, image_name: &str) {
        let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        if let Some((name, bytes)) = metadata {
            archive.start_file(name, options).unwrap();
            archive.write_all(bytes).unwrap();
        }
        archive.start_file(image_name, options).unwrap();
        archive.write_all(&png()).unwrap();
        archive.finish().unwrap();
    }

    fn clean_cache(root: &Path) {
        let folder = root.join("mod-browser/images");
        for entry in fs::read_dir(&folder).unwrap() {
            fs::remove_file(entry.unwrap().path()).unwrap();
        }
        fs::remove_dir(folder).unwrap();
        fs::remove_dir(root.join("mod-browser")).unwrap();
    }

    #[test]
    fn reads_real_fabric_forge_and_resource_pack_artwork() {
        let root =
            std::env::temp_dir().join(format!("horizon-art-archives-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let mut resolver = Resolver::new(root.clone());
        type ArchiveCase<'a> = (Option<(&'a str, &'a [u8])>, &'a str);
        let cases: [ArchiveCase; 3] = [
            (
                Some((
                    "fabric.mod.json",
                    br#"{"icon":{"128":"assets/test/icon.png"}}"#,
                )),
                "assets/test/icon.png",
            ),
            (
                Some((
                    "META-INF/mods.toml",
                    b"logoFile = \"assets/test/logo.png\"\n",
                )),
                "assets/test/logo.png",
            ),
            (None, "pack.png"),
        ];
        for (index, (metadata, name)) in cases.into_iter().enumerate() {
            let path = root.join(format!("test-{index}.zip"));
            archive(&path, metadata, name);
            let resolved = resolver
                .resolve(&Icon::Archive(path.clone()))
                .unwrap()
                .unwrap();
            assert_eq!(
                image::open(resolved).unwrap().to_rgba8().get_pixel(0, 0).0,
                [42, 100, 180, 255]
            );
            fs::remove_file(path).unwrap();
        }
        clean_cache(&root);
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn imported_content_and_the_original_instance_use_their_own_icons() {
        let root = std::env::temp_dir().join(format!("horizon-art-import-{}", std::process::id()));
        let work = root.join("work");
        let original = root.join("original");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&original).unwrap();
        let jar = original.join("mod.jar");
        archive(&jar, None, "icon.png");
        fs::write(original.join("icon.png"), png()).unwrap();
        fs::write(
            work.join("pack.json"),
            serde_json::to_vec(&Pack::default()).unwrap(),
        )
        .unwrap();
        fs::write(
            work.join("inventory.json"),
            serde_json::to_vec(&serde_json::json!([
                {"key":"my-mod", "path":"mods/mod.jar", "jar":jar}
            ]))
            .unwrap(),
        )
        .unwrap();
        remember_import(&work, &original).unwrap();
        let mut resolver = Resolver::new(root.clone());
        assert!(resolver
            .resolve(&Icon::ImportedItem {
                work: work.clone(),
                key: "my-mod".into()
            })
            .unwrap()
            .is_some());
        assert!(resolver
            .resolve(&Icon::Instance {
                home: work.join("home"),
                work: Some(work.clone()),
                build: None
            })
            .unwrap()
            .is_some());
        for path in [
            jar,
            original.join("icon.png"),
            work.join("pack.json"),
            work.join("inventory.json"),
            work.join(IMPORT_ART),
        ] {
            fs::remove_file(path).unwrap();
        }
        clean_cache(&root);
        fs::remove_dir(work).unwrap();
        fs::remove_dir(original).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn rejects_parent_paths_and_oversized_archive_metadata() {
        assert!(!plain_archive_path("../private.png"));
        assert!(!plain_archive_path("/private.png"));
        assert!(!plain_archive_path("C:\\private.png"));
        assert!(plain_archive_path("assets/mod/logo.png"));
        assert!(read_limited(&b"oversized"[..], 4).is_err());
    }
}
