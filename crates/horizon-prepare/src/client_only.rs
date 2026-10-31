//! Strict, server-free validation of a player's vanilla Minecraft 1.16.5 client files.
//! This deliberately does not invoke Java or the legacy exporter pipeline.
use crate::pyjson::Value;
use crate::{digest, read_json, write_json, Log};
use std::fs;
use std::path::{Path, PathBuf};

const VERSION_JSON_SHA1: &str = "fba9f7833e858a1257d810d21a3a9e3c967f9077";
const CLIENT_JAR_SHA1: &str = "37fd3c903861eeff3bc24b71eed48f828b5269c8";
const ASSET_INDEX_SHA1: &str = "f3c4aa96e12951cd2781b3e1c0e8ab82bf719cf2";

fn require_sha1(path: &Path, expected: &str, label: &str) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!("Missing {label}: {}", path.display()));
    }
    let actual = digest(path, "sha1")?;
    if actual != expected {
        return Err(format!(
            "{label} SHA-1 mismatch: expected {expected}, got {actual} ({})",
            path.display()
        ));
    }
    Ok(())
}

/// Validates the exact 1.16.5 vanilla client and all objects referenced by its 1.16 asset index.
/// Source layout: versions/1.16.5/{1.16.5.jar,1.16.5.json}, assets/{indexes,objects}.
pub fn verify_1_16_5(resources: &Path, root: &Path, log: Log) -> Result<PathBuf, String> {
    let version_dir = resources.join("versions/1.16.5");
    let version_json = version_dir.join("1.16.5.json");
    let client_jar = version_dir.join("1.16.5.jar");
    let index = resources.join("assets/indexes/1.16.json");

    require_sha1(
        &version_json,
        VERSION_JSON_SHA1,
        "Minecraft 1.16.5 version JSON",
    )?;
    require_sha1(&client_jar, CLIENT_JAR_SHA1, "Minecraft 1.16.5 client JAR")?;
    require_sha1(&index, ASSET_INDEX_SHA1, "Minecraft 1.16 asset index")?;

    let manifest = read_json(&version_json)?;
    if manifest.get("id").and_then(Value::as_str) != Some("1.16.5") {
        return Err("Version JSON id is not 1.16.5".into());
    }
    if manifest
        .get("javaVersion")
        .and_then(|v| v.get("majorVersion"))
        .and_then(Value::as_i64)
        != Some(8)
    {
        return Err("Minecraft 1.16.5 must declare Java 8".into());
    }
    let client_pin = manifest
        .get("downloads")
        .and_then(|v| v.get("client"))
        .and_then(|v| v.get("sha1"))
        .and_then(Value::as_str);
    if client_pin != Some(CLIENT_JAR_SHA1) {
        return Err(
            "Version JSON client download pin does not match the official 1.16.5 client".into(),
        );
    }
    let asset_meta = manifest
        .get("assetIndex")
        .ok_or("Version JSON has no assetIndex")?;
    if asset_meta.get("id").and_then(Value::as_str) != Some("1.16")
        || asset_meta.get("sha1").and_then(Value::as_str) != Some(ASSET_INDEX_SHA1)
    {
        return Err("Version JSON does not reference the pinned 1.16 asset index".into());
    }

    let asset_index = read_json(&index)?;
    let objects = asset_index
        .get("objects")
        .and_then(Value::as_object)
        .ok_or("Asset index has no objects map")?;
    let mut checked = 0usize;
    for (name, object) in objects {
        let hash = object
            .get("hash")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Asset {name} has no hash"))?;
        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("Asset {name} has an invalid SHA-1"));
        }
        let path = resources.join("assets/objects").join(&hash[..2]).join(hash);
        require_sha1(&path, hash, &format!("Asset object {name}"))?;
        checked += 1;
    }

    let report_dir = root.join(".local/horizon-client/1.16.5");
    fs::create_dir_all(&report_dir).map_err(|e| e.to_string())?;
    let report = Value::Object(vec![
        ("version".into(), Value::str("1.16.5")),
        ("protocol".into(), Value::Int("754".into())),
        ("data_version".into(), Value::Int("2586".into())),
        ("java_major".into(), Value::Int("8".into())),
        ("client_jar_sha1".into(), Value::str(CLIENT_JAR_SHA1)),
        ("version_json_sha1".into(), Value::str(VERSION_JSON_SHA1)),
        ("asset_index_id".into(), Value::str("1.16")),
        ("asset_index_sha1".into(), Value::str(ASSET_INDEX_SHA1)),
        (
            "verified_asset_objects".into(),
            Value::Int(checked.to_string()),
        ),
        (
            "source_directory".into(),
            Value::str(resources.display().to_string()),
        ),
    ]);
    let report_path = report_dir.join("source-verified.json");
    write_json(&report_path, &report)?;
    log("Minecraft 1.16.5: client JAR and version metadata verified");
    log(&format!("Minecraft assets: {checked} objects verified"));
    log(&format!("Verification report: {}", report_path.display()));
    Ok(report_path)
}
