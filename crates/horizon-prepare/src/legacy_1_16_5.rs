//! Builds the minimum runtime registry bundle for the 1.16.5 single-player prototype.
//! This is intentionally separate from the pinned 26.3 Java exporter pipeline.
use horizon_world::{
    DATA_VERSION,
    legacy_1_16_5::{blocks, LegacyBlock, LegacyBlockProperty},
    nbt::{JavaString, Limits, NamedTag, Tag, write_named},
    registry::{BlockRegistry, BlockStateId},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

fn property_values(property: &LegacyBlockProperty) -> Result<Vec<String>, String> {
    match property.kind.as_str() {
        "bool" => Ok(vec!["true".into(), "false".into()]),
        "enum" if !property.values.is_empty() => Ok(property.values.clone()),
        "int" if property.num_values > 0 => Ok((0..property.num_values)
            .map(|value| value.to_string())
            .collect()),
        other => Err(format!(
            "Unsupported 1.16.5 property type {other:?} for {}",
            property.name
        )),
    }
}

fn state_properties(
    block: &LegacyBlock,
    mut offset: u32,
) -> Result<BTreeMap<String, String>, String> {
    let mut result = BTreeMap::new();
    for property in block.states.iter().rev() {
        let values = property_values(property)?;
        let radix = u32::try_from(values.len()).map_err(|e| e.to_string())?;
        if radix == 0 {
            return Err(format!(
                "Empty property values for {}.{}",
                block.name, property.name
            ));
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
            return Err(format!(
                "Non-contiguous legacy state range at {}",
                block.name
            ));
        }
        let count = block.max_state_id - block.min_state_id + 1;
        let mut combinations = 1u32;
        for property in &block.states {
            let radix =
                u32::try_from(property_values(property)?.len()).map_err(|e| e.to_string())?;
            combinations = combinations
                .checked_mul(radix)
                .ok_or_else(|| format!("State count overflow for {}", block.name))?;
        }
        if combinations != count {
            return Err(format!(
                "Property combinations ({combinations}) do not match state range ({count}) for {}",
                block.name
            ));
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
        "minecraft:blocks_motion_in_heightmap": {
            "replace": false,
            "values": motion
        },
        "minecraft:blocks_motion_in_heightmap_no_leaves": {
            "replace": false,
            "values": motion_no_leaves
        },
        "minecraft:blocks_fluid_flow": {
            "replace": false,
            "values": ["minecraft:water", "minecraft:lava"]
        }
    });
    Ok((json!({"facts": facts, "tags": tags}), count))
}

/// Build the versioned per-block movement table. These are baseline vanilla
/// coefficients; special-case movement effects remain disabled until their
/// exact 1.16.5 behavior exporters are connected.
fn movement_table(registry: &BlockRegistry) -> NamedTag {
    let mut blocks = BTreeMap::new();
    for index in 0..registry.len() {
        let name = &registry.state(BlockStateId(index as u32)).unwrap().name;
        let friction: f32 = match name.as_str() {
            "minecraft:slime_block" => 0.8,
            "minecraft:ice" | "minecraft:packed_ice" | "minecraft:blue_ice" => 0.98,
            "minecraft:honey_block" | "minecraft:soul_sand" => 0.4,
            _ => 0.6,
        };
        let properties = Tag::compound([
            ("friction", Tag::Float(friction.to_bits())),
            ("speedFactor", Tag::Float(1.0f32.to_bits())),
            ("jumpFactor", Tag::Float(1.0f32.to_bits())),
            ("bounceRestitution", Tag::Float(0.0f32.to_bits())),
            ("fallDistanceReduction", Tag::Float(0.0f32.to_bits())),
            (
                "fenceGate",
                Tag::Byte(i8::from(name.ends_with("_fence_gate"))),
            ),
        ]);
        blocks.insert(JavaString::from(name.as_str()), properties);
    }
    let states = (0..registry.len())
        .map(|index| registry.state(BlockStateId(index as u32)).unwrap().to_nbt())
        .collect::<Vec<_>>();
    let nested = Tag::compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        ("states", Tag::List(states.clone())),
        ("blocks", Tag::Compound(blocks)),
    ]);
    NamedTag {
        name: JavaString::default(),
        tag: Tag::compound([
            ("DataVersion", Tag::Int(DATA_VERSION)),
            ("states", Tag::List(states)),
            ("movement", nested),
        ]),
    }
}

fn write_movement_table(path: &Path, registry: &BlockRegistry) -> Result<(), String> {
    let bytes = write_named(&movement_table(registry), Limits::tables())
        .map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Build a deliberately conservative collision/occlusion table for the first
/// terrain prototype. Only registry states known to be empty or full cubes are
/// cached; all partial/dynamic shapes remain uncached instead of being lied about.
fn cached_shape_table(registry: &BlockRegistry) -> Result<NamedTag, String> {
    let blocks = blocks().map_err(|e| e.to_string())?;
    // minecraft-data's broad "block" bounding-box flag also covers partial
    // shapes (fences, stairs, panes, etc.). Cache full cubes only for this
    // explicit vanilla terrain whitelist; everything else stays on fallback.
    const FULL_CUBES: &[&str] = &[
        "stone", "granite", "polished_granite", "diorite", "polished_diorite",
        "andesite", "polished_andesite", "grass_block", "dirt", "coarse_dirt",
        "podzol", "cobblestone", "bedrock", "sand", "red_sand", "gravel",
        "clay", "bricks", "stone_bricks", "cracked_stone_bricks",
        "mossy_stone_bricks", "nether_bricks", "red_nether_bricks",
        "netherrack", "end_stone", "obsidian", "crying_obsidian",
        "sandstone", "red_sandstone", "quartz_block", "prismarine",
        "dark_prismarine", "sea_lantern", "coal_block", "iron_block",
        "gold_block", "diamond_block", "emerald_block", "lapis_block",
        "copper_block", "raw_iron_block", "raw_gold_block", "raw_copper_block",
        "oak_planks", "spruce_planks", "birch_planks", "jungle_planks",
        "acacia_planks", "dark_oak_planks", "crimson_planks", "warped_planks",
    ];
    let bounds: HashMap<String, i32> = blocks
        .into_iter()
        .map(|block| {
            let shape = if FULL_CUBES.contains(&block.name.as_str()) {
                1
            } else if block.bounding_box.as_deref() == Some("empty") {
                0
            } else {
                -1
            };
            (format!("minecraft:{}", block.name), shape)
        })
        .collect();
    let states = (0..registry.len())
        .map(|index| {
            registry
                .state(BlockStateId(index as u32))
                .ok_or_else(|| "registry state disappeared while writing shape tables".to_owned())
                .map(|state| state.to_nbt())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mapping = (0..registry.len())
        .map(|index| {
            let state = registry.state(BlockStateId(index as u32)).unwrap();
            *bounds.get(&state.name).unwrap_or(&-1)
        })
        .collect::<Vec<_>>();
    let empty = Tag::compound([
        ("canonical", Tag::Byte(1)),
        ("x", Tag::LongArray(vec![0])),
        ("y", Tag::LongArray(vec![0])),
        ("z", Tag::LongArray(vec![0])),
        ("occupied", Tag::ByteArray(vec![])),
        ("cube_axes", Tag::Byte(0)),
        ("discrete_min", Tag::IntArray(vec![0, 0, 0])),
        ("discrete_max", Tag::IntArray(vec![0, 0, 0])),
        ("cube", Tag::Byte(0)),
    ]);
    let full = Tag::compound([
        ("canonical", Tag::Byte(2)),
        ("x", Tag::LongArray(vec![0, 1.0f64.to_bits() as i64])),
        ("y", Tag::LongArray(vec![0, 1.0f64.to_bits() as i64])),
        ("z", Tag::LongArray(vec![0, 1.0f64.to_bits() as i64])),
        ("occupied", Tag::ByteArray(vec![1])),
        ("cube_axes", Tag::Byte(7)),
        ("discrete_min", Tag::IntArray(vec![0, 0, 0])),
        ("discrete_max", Tag::IntArray(vec![1, 1, 1])),
        ("cube", Tag::Byte(1)),
    ]);
    let nested = Tag::compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        ("states", Tag::List(states.clone())),
        ("shapes", Tag::List(vec![empty, full])),
        ("cached_shapes", Tag::IntArray(mapping)),
    ]);
    Ok(NamedTag {
        name: JavaString::default(),
        tag: Tag::compound([
            ("DataVersion", Tag::Int(DATA_VERSION)),
            ("states", Tag::List(states)),
            ("collision", nested),
        ]),
    })
}

fn write_shape_table(path: &Path, registry: &BlockRegistry) -> Result<(), String> {
    let table = cached_shape_table(registry)?;
    let bytes = write_named(&table, Limits::tables()).map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes facts.json, block-tags.json and the explicit prototype readiness manifest.
pub fn prepare(resources: &Path, root: &Path, log: &dyn Fn(&str)) -> Result<PathBuf, String> {
    crate::client_only::verify_1_16_5(resources, root, log)?;
    let (bundle, states) = facts_bundle()?;
    // The manifest is the readiness marker. Remove any old version's marker before
    // replacing its registry files, and write the new marker only after both succeed.
    let manifest_path = resources.join("horizon-resources.json");
    if manifest_path.is_file() {
        fs::remove_file(&manifest_path).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    }
    let facts_path = resources.join("facts.json");
    let tags_path = resources.join("block-tags.json");
    fs::write(
        &facts_path,
        serde_json::to_vec(&bundle["facts"]).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{}: {e}", facts_path.display()))?;
    fs::write(
        &tags_path,
        serde_json::to_vec(&bundle["tags"]).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{}: {e}", tags_path.display()))?;

    // The collision and light loaders require canonical empty/full shape
    // identities. Partial shapes deliberately stay uncached for vanilla fallback.
    let registry = BlockRegistry::from_facts_json(fs::File::open(&facts_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    write_shape_table(&resources.join("cached-collision.nbt"), &registry)?;
    write_shape_table(&resources.join("occlusion.nbt"), &registry)?;
    write_movement_table(&resources.join("movement-properties.nbt"), &registry)?;

    let manifest = json!({
        "minecraft": "1.16.5",
        "data_version": 2586,
        "bundle_kind": "prototype-1.16.5",
        "registry_states": states,
        "registry_source": "bundled Minecraft 1.16.2 registry reference; 1.16.5 block-state layout",
        "status": "registry-only-not-playable",
        "limitations": [
            "gameplay-facts.json and block-behaviours.json are not yet exported",
            "collision and occlusion tables cache only known empty/full block shapes; partial shapes are uncached",
            "movement coefficients are a conservative baseline; specialized movement tags and effects are incomplete",
            "biome, environment and model tables are not yet fully prepared",
            "this stage alone is not sufficient to load or create a world"
        ]
    });
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("{}: {e}", manifest_path.display()))?;

    log(&format!(
        "Prepared 1.16.5 prototype registry: {states} block states"
    ));
    log("Prepared conservative 1.16.5 collision and occlusion tables (empty/full shapes only).");
    log("Prepared baseline movement coefficients; specialized player-motion dependencies remain incomplete.");
    log("Gameplay facts, block behaviours and model tables are still required.");
    log(&format!("Staged resources: {}", resources.display()));
    Ok(resources.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_table_round_trips_for_every_registry_block() {
        use horizon_world::{movement_properties::MovementProperties, registry::BlockTableFile};
        let (bundle, _) = facts_bundle().unwrap();
        let registry = BlockRegistry::from_facts_json(bundle["facts"].to_string().as_bytes())
            .unwrap();
        let bytes = write_named(&movement_table(&registry), Limits::tables()).unwrap();
        let file = BlockTableFile::read(&bytes, &registry).unwrap();
        let movement = MovementProperties::from_table(file.table("movement").unwrap()).unwrap();
        let stone = registry.default_state("minecraft:stone").unwrap();
        let ice = registry.default_state("minecraft:ice").unwrap();
        assert_eq!(movement.coefficients(stone).unwrap().friction, 0.6);
        assert_eq!(movement.coefficients(ice).unwrap().friction, 0.98);
    }

    #[test]
    fn cached_shape_tables_round_trip_with_canonical_identities() {
        use horizon_world::{
            collision::CachedCollisionRegistry,
            registry::BlockTableFile,
        };
        let (bundle, _) = facts_bundle().unwrap();
        let registry = BlockRegistry::from_facts_json(bundle["facts"].to_string().as_bytes())
            .unwrap();
        let named = cached_shape_table(&registry).unwrap();
        let bytes = write_named(&named, Limits::tables()).unwrap();
        let file = BlockTableFile::read(&bytes, &registry).unwrap();
        let cached = CachedCollisionRegistry::from_table(file.table("collision").unwrap())
            .unwrap();
        assert!(cached.has_canonical_identities());
        assert!(cached.cached_state_count() > 0);
        assert!(cached.cached_state_count() <= registry.len());
        assert_eq!(cached.shape_count(), 2);
    }

    #[test]
    fn legacy_state_order_matches_the_known_grass_block_default() {
        let block = blocks()
            .unwrap()
            .into_iter()
            .find(|block| block.name == "grass_block")
            .unwrap();
        assert_eq!(block.default_state - block.min_state_id, 1);
        assert_eq!(
            state_properties(&block, 1)
                .unwrap()
                .get("snowy")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn generated_state_order_matches_vanilla_default_state_offsets() {
        let blocks = blocks().unwrap();
        let check = |name: &str, expected: &[(&str, &str)]| {
            let block = blocks.iter().find(|block| block.name == name).unwrap();
            let properties =
                state_properties(block, block.default_state - block.min_state_id).unwrap();
            for &(key, value) in expected {
                assert_eq!(
                    properties.get(key).map(String::as_str),
                    Some(value),
                    "{name}.{key}"
                );
            }
        };
        check("oak_log", &[("axis", "y")]);
        check("water", &[("level", "0")]);
        check("oak_slab", &[("type", "bottom"), ("waterlogged", "false")]);
        check(
            "oak_stairs",
            &[
                ("facing", "north"),
                ("half", "bottom"),
                ("shape", "straight"),
                ("waterlogged", "false"),
            ],
        );
        check(
            "oak_door",
            &[
                ("facing", "north"),
                ("half", "lower"),
                ("hinge", "left"),
                ("open", "false"),
                ("powered", "false"),
            ],
        );
    }

    #[test]
    fn generated_facts_and_tags_load_through_the_runtime_codecs() {
        let (bundle, _) = facts_bundle().unwrap();
        let facts = serde_json::to_vec(&bundle["facts"]).unwrap();
        let registry = horizon_world::registry::BlockRegistry::from_facts_json(&facts[..]).unwrap();
        let mut definitions = horizon_world::worldgen::tags::TagDefinitions::default();
        for (name, value) in bundle["tags"].as_object().unwrap() {
            definitions
                .apply_json(name, "test", &value.to_string())
                .unwrap();
        }
        let built = definitions
            .build(|name, _| registry.default_state(name).map(|_| name.to_owned()))
            .unwrap();
        assert!(
            built.missing.is_empty(),
            "unresolved tags: {:?}",
            built.missing.keys()
        );
        for tag in [
            "minecraft:blocks_motion_in_heightmap",
            "minecraft:blocks_motion_in_heightmap_no_leaves",
            "minecraft:blocks_fluid_flow",
        ] {
            assert!(built.tags.contains_key(tag), "missing {tag}");
        }
    }

    #[test]
    fn generated_registry_has_required_runtime_materials_and_tags() {
        let (bundle, count) = facts_bundle().unwrap();
        assert!(count > 16_000);
        for name in [
            "minecraft:air",
            "minecraft:bedrock",
            "minecraft:stone",
            "minecraft:dirt",
            "minecraft:grass_block",
            "minecraft:sand",
            "minecraft:oak_planks",
            "minecraft:bricks",
        ] {
            assert!(
                bundle["facts"]["defaults"].get(name).is_some(),
                "missing {name}"
            );
        }
        for name in [
            "minecraft:blocks_motion_in_heightmap",
            "minecraft:blocks_motion_in_heightmap_no_leaves",
            "minecraft:blocks_fluid_flow",
        ] {
            assert!(bundle["tags"].get(name).is_some(), "missing {name}");
        }
    }
}
