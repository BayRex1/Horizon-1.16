//! Builds the minimum runtime registry bundle for the 1.16.5 single-player prototype.
//! This is intentionally separate from the pinned 26.3 Java exporter pipeline.
use horizon_world::legacy_1_16_5::{blocks, LegacyBlock, LegacyBlockProperty};
use serde_json::{json, Value};
use std::{\n    collections::BTreeMap,\n    fs,\n    path::{Path, PathBuf},\n};

fn property_values(property: &LegacyBlockProperty) -> Result<Vec<String>, String> {
    match property.kind.as_str() {
        "bool" => Ok(vec!["true".into(), "false".into()]),
        "enum" if !property.values.is_empty() => Ok(property.values.clone()),
        other => Err(format!("Unsupported 1.16.5 property type {other:?} for {}", property.name)),
    }
}

fn state_properties(\n    block: &LegacyBlock,\n    mut offset: u32,\n) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    for property in block.states.iter().rev() {
        let values = property_values(property)?;
        let radix = u32::try_from(values.len()).map_err(|e| e.to_string())?;
        if radix == 0 {
            return Err(format!("Empty property values for {}.{}", block.name, property.name));
        }
        let digit = (offset % radix) as usize;
        offset /= radix;
        result.insert(property.name.clone(), values[digit].clone());
    }
    if offset != 0 {
        return Err(format!("State range exceeds properties for {}", block.name));
    }
    Ok(result)
}

fn facts_bundle() -> Result<(Value, usize), String> {
    let mut blocks = blocks().map_err(|e| e.to_string())?;
    blocks.sort_by_key(|block| block.min_state_id);
    let mut rows = Vec::new();
    let mut defaults = BTreeMap::new();
    let mut next_state = 0u32;
    let mut motion = Vec::new();
    let mut motion_no_leaves = Vec::new();
    for block in &blocks {
        if block.min_state_id != next_state || block.max_state_id < block.min_state_id {
            return Err(format!("Non-contiguous legacy state range at {}", block.name));
        }
        let count = block.max_state_id - block.min_state_id + 1;
        let mut combinations = 1u32;
        for property in &block.states {
            combinations = combinations.checked_mul(property_values(property)?.len() as u32)
                .ok_or_else(|| format!("State count overflow for {}", block.name))?;
        }
        if combinations != count {
            return Err(format!("Property combinations ({combinations}) do not match state range ({count}) for {}", block.name));
        }
        if block.default_state < block.min_state_id || block.default_state > block.max_state_id {
            return Err(format!("Invalid default state for {}", block.name));
        }
        let name = format!("minecraft:{}", block.name);
        defaults.insert(name.clone(), json!(block.default_state));
        let air = matches!(block.name.as_str(), "air" | "cave_air" | "void_air");
        let fluid = matches!(block.name.as_str(), "water" | "lava");
        for state_id in block.min_state_id..=block.max_state_id {
            let properties = state_properties(block, state_id - block.min_state_id)?;
            let state = if properties.is_empty() {
                json!(name)
            } else {
                json!({"id": name, "properties": properties})
            };
            rows.push(json!([name, air, fluid, "", state]));
        }
        if block.bounding_box.as_deref() == Some("block") {
            motion.push(name.clone());
            if !block.name.ends_with("_leaves") {
                motion_no_leaves.push(name.clone());
            }
        }
        next_state = block.max_state_id + 1;
    }
    if rows.is_empty() || defaults.len() != blocks.len() {
        return Err("Legacy block registry is empty or has duplicate block names".into());
    }
    let count = rows.len();
    let facts = json!({"defaults": defaults, "states": rows});
    let tags = json!({
        "minecraft:blocks_motion_in_heightmap": json!({"replace": false, "values": motion}),
        "minecraft:blocks_motion_in_heightmap_no_leaves": json!({"replace": false, "values": motion_no_leaves}),
        "minecraft:blocks_fluid_flow": json!({"replace": false, "values": ["minecraft:water", "minecraft:lava"]})
    });
    Ok((json!({"facts": facts, "tags": tags}), count))
}

/// Writes facts.json, block-tags.json and the explicit prototype readiness manifest.
pub fn prepare(resources: &Path, root: &Path, log: &dyn Fn(&str)) -> Result<PathBuf, String> {
    crate::client_only::verify_1_16_5(resources, root, log)?;
    let (bundle, states) = facts_bundle()?;
    let facts_path = resources.join("facts.json");
    let tags_path = resources.join("block-tags.json");
    fs::write(&facts_path, serde_json::to_vec(&bundle["facts"]).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", facts_path.display()))?;
    fs::write(&tags_path, serde_json::to_vec(&bundle["tags"]).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", tags_path.display()))?;
    let manifest = json!({
        "minecraft": "1.16.5",
        "data_version": 2586,
        "bundle_kind": "prototype-1.16.5",
        "registry_states": states,
        "registry_source": "bundled Minecraft 1.16.2 registry reference; 1.16.5 block-state layout"
    });
    let manifest_path = resources.join("horizon-resources.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    log(&format!("Prepared 1.16.5 prototype registry: {states} block states"));
    log(&format!("Runtime bundle: {}", resources.display()));
    Ok(PathBuf::from(resources))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_state_order_matches_the_known_grass_block_default() {
        let block = blocks()\n            .unwrap()\n            .into_iter()\n            .find(|block| block.name == "grass_block")\n            .unwrap();
        assert_eq!(block.default_state - block.min_state_id, 1);
        assert_eq!(\n            state_properties(&block, 1).unwrap().get("snowy").map(String::as_str),\n            Some("false")\n        );
    }

    #[test]
    fn generated_facts_and_tags_load_through_the_runtime_codecs() {
        let (bundle, _) = facts_bundle().unwrap();
        let facts = serde_json::to_vec(&bundle["facts"]).unwrap();
        let registry = horizon_world::registry::BlockRegistry::from_facts_json(&facts[..]).unwrap();
        let mut definitions = horizon_world::worldgen::tags::TagDefinitions::default();
        for (name, value) in bundle["tags"].as_object().unwrap() {
            definitions.apply_json(name, "test", &value.to_string()).unwrap();
        }
        let built = definitions.build(|name, _| registry.default_state(name).map(|_| name.to_owned())).unwrap();
        assert!(built.missing.is_empty(), "unresolved tags: {:?}", built.missing.keys());
        for tag in ["minecraft:blocks_motion_in_heightmap", "minecraft:blocks_motion_in_heightmap_no_leaves", "minecraft:blocks_fluid_flow"] {
            assert!(built.tags.contains_key(tag), "missing {tag}");
        }
    }

    #[test]
    fn generated_registry_has_required_runtime_materials_and_tags() {
        let (bundle, count) = facts_bundle().unwrap();
        assert!(count > 16_000);
        for name in ["minecraft:air", "minecraft:bedrock", "minecraft:stone", "minecraft:dirt", "minecraft:grass_block", "minecraft:sand", "minecraft:oak_planks", "minecraft:bricks"] {
            assert!(bundle["facts"]["defaults"].get(name).is_some(), "missing {name}");
        }
        for name in ["minecraft:blocks_motion_in_heightmap", "minecraft:blocks_motion_in_heightmap_no_leaves", "minecraft:blocks_fluid_flow"] {
            assert!(bundle["tags"].get(name).is_some(), "missing {name}");
        }
    }
}
