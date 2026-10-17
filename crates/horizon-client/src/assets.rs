use crate::Result;
use horizon_world::{
    DATA_VERSION,
    heightmap::HeightmapState,
    nbt::{self, Tag},
    registry::{BlockRegistry, BlockStateId},
    worldgen::{tags::TagDefinitions, terrain::TerrainBlockStates},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, BufReader},
    path::Path,
    sync::Arc,
};

pub const MATERIALS: [&str; 6] = ["bedrock", "dirt", "stone", "sand", "oak_planks", "bricks"];
#[derive(Clone, Copy)]
pub struct StateFacts {
    air: bool,
    fluid: bool,
    air_block: bool,
    motion: bool,
    no_leaves: bool,
}
impl HeightmapState for StateFacts {
    fn is_air(self) -> bool {
        self.air
    }
    fn is_air_block(self) -> bool {
        self.air_block
    }
    fn has_fluid(self) -> bool {
        self.fluid
    }
    fn blocks_motion_in_heightmap(self) -> bool {
        self.motion
    }
    fn blocks_motion_in_heightmap_no_leaves(self) -> bool {
        self.no_leaves
    }
}
#[derive(Clone)]
pub struct Properties(Arc<Vec<StateFacts>>);
impl TerrainBlockStates for Properties {
    type State = StateFacts;
    fn heightmap_state(&self, id: BlockStateId) -> StateFacts {
        self.0[id.0 as usize]
    }
}
pub struct Assets {
    /// Leaked once per process: shared by world generation threads.
    pub registry: &'static BlockRegistry,
    pub properties: Properties,
    pub material_states: [BlockStateId; 6],
    pub sprite_size: u32,
    pub sprite_minimum_size: u32,
    pub pixels: Vec<u8>,
    pub texture_layers: u32,
    pub model_catalog: crate::models::StateModels,
    /// Shared parsed resources; item-only validation errors surface at item preparation.
    pub model_library: std::result::Result<Arc<horizon_assets::resolve::ModelLibrary>, String>,
    pub model_placements: Vec<(horizon_core::position::BlockPos, BlockStateId)>,
    pub model_gallery: Vec<Value>,
    pub animations: Vec<crate::sprites::SpriteAnimation>,
    pub sprite_layers: std::collections::BTreeMap<String, u32>,
    pub translucent_layers: Vec<bool>,
    /// Each layer's `MipmapGenerator` settings.
    pub sprite_mips: Vec<crate::mipmap::Settings>,
    /// Block names in `#minecraft:blocks_fluid_flow` (fluid flow vectors).
    pub blocks_fluid_flow: BTreeSet<String>,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn tag(value: &Value) -> Result<Tag> {
    Ok(match value {
        Value::String(value) => Tag::String(value.as_str().into()),
        Value::Number(value) => Tag::Int(i32::try_from(
            value
                .as_i64()
                .ok_or_else(|| invalid("invalid registry number"))?,
        )?),
        Value::Array(values) => Tag::List(values.iter().map(tag).collect::<Result<_>>()?),
        Value::Object(values) => Tag::Compound(
            values
                .iter()
                .map(|(name, value)| Ok((name.as_str().into(), tag(value)?)))
                .collect::<Result<_>>()?,
        ),
        _ => return Err(invalid("unsupported registry value").into()),
    })
}
/// Cache the validated NBT representation alongside the source facts. A digest
/// of the complete source makes modified content roots and stale caches safe.
fn load_registry(root: &Path, facts_bytes: &[u8], facts: &Value) -> Result<BlockRegistry> {
    let digest = Sha256::digest(facts_bytes);
    let cache = root.join("block-registry-cache.nbt");
    if let Ok(bytes) = fs::read(&cache)
        && let Ok((saved, used)) = nbt::read_network(&bytes, nbt::Limits::default())
        && used == bytes.len()
        && saved.get("source_sha256") == Some(&Tag::ByteArray(digest.to_vec()))
        && let Some(registry) = saved.get("registry")
        && let Ok(decoded) = BlockRegistry::from_nbt(registry)
    {
        return Ok(decoded);
    }
    let states = facts["states"]
        .as_array()
        .ok_or_else(|| invalid("missing registry state facts"))?;
    let registry = Tag::compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        (
            "states",
            Tag::List(
                states
                    .iter()
                    .map(|state| tag(&state[4]))
                    .collect::<Result<_>>()?,
            ),
        ),
        ("defaults", tag(&facts["defaults"])?),
    ]);
    // Cache only after the same validation used on the uncached path.
    let validated = BlockRegistry::from_nbt(&registry)?;
    let saved = Tag::compound([
        ("source_sha256", Tag::ByteArray(digest.to_vec())),
        ("registry", registry.clone()),
    ]);
    if let Ok(bytes) = nbt::write_network(&saved, nbt::Limits::default()) {
        let temporary = root.join(format!("block-registry-cache-{}.tmp", std::process::id()));
        if fs::write(&temporary, bytes).is_ok() {
            let _ = fs::rename(&temporary, &cache);
        }
        let _ = fs::remove_file(&temporary);
    }
    Ok(validated)
}
impl Assets {
    pub fn load(root: &Path) -> Result<Self> {
        let manifest: Value = serde_json::from_reader(BufReader::new(File::open(
            root.join("horizon-resources.json"),
        )?))?;
        if manifest["data_version"].as_i64() != Some(i64::from(DATA_VERSION)) {
            return Err(invalid("resources do not match Horizon's pinned data version").into());
        }
        let facts_bytes = fs::read(root.join("facts.json"))?;
        let facts: Value = serde_json::from_slice(&facts_bytes)?;
        let states = facts["states"]
            .as_array()
            .ok_or_else(|| invalid("missing registry state facts"))?;
        let registry = load_registry(root, &facts_bytes, &facts)?;
        let bundle: Value =
            serde_json::from_reader(BufReader::new(File::open(root.join("block-tags.json"))?))?;
        let mut definitions = TagDefinitions::default();
        for (name, text) in bundle
            .as_object()
            .ok_or_else(|| invalid("invalid block tag bundle"))?
        {
            definitions.apply_json(
                name,
                "vanilla",
                text.as_str()
                    .ok_or_else(|| invalid("invalid block tag text"))?,
            )?;
        }
        let built =
            definitions.build(|name, _| facts["defaults"].get(name).map(|_| name.to_owned()))?;
        if !built.missing.is_empty() {
            return Err(invalid("unresolved block tags").into());
        }
        let motion: BTreeSet<_> = built
            .tags
            .get("minecraft:blocks_motion_in_heightmap")
            .ok_or_else(|| invalid("missing motion tag"))?
            .iter()
            .map(String::as_str)
            .collect();
        let no_leaves: BTreeSet<_> = built
            .tags
            .get("minecraft:blocks_motion_in_heightmap_no_leaves")
            .ok_or_else(|| invalid("missing no-leaves motion tag"))?
            .iter()
            .map(String::as_str)
            .collect();
        let blocks_fluid_flow = built
            .tags
            .get("minecraft:blocks_fluid_flow")
            .ok_or_else(|| invalid("missing fluid flow tag"))?
            .iter()
            .cloned()
            .collect();
        let properties = Properties(Arc::new(
            states
                .iter()
                .map(|state| {
                    Ok(StateFacts {
                        air: state[1]
                            .as_bool()
                            .ok_or_else(|| invalid("missing air fact"))?,
                        fluid: state[2]
                            .as_bool()
                            .ok_or_else(|| invalid("missing fluid fact"))?,
                        air_block: state[0].as_str() == Some("minecraft:air"),
                        motion: motion.contains(
                            state[0]
                                .as_str()
                                .ok_or_else(|| invalid("missing state name"))?,
                        ),
                        no_leaves: no_leaves.contains(
                            state[0]
                                .as_str()
                                .ok_or_else(|| invalid("missing state name"))?,
                        ),
                    })
                })
                .collect::<Result<_>>()?,
        ));
        let mut material_states = [BlockStateId(0); 6];
        for (layer, name) in MATERIALS.iter().enumerate() {
            material_states[layer] = registry
                .default_state(&format!("minecraft:{name}"))
                .ok_or_else(|| invalid("missing test-scene material"))?;
        }
        let registry: &'static BlockRegistry = Box::leak(Box::new(registry));
        Self::with_models(
            root,
            registry,
            properties,
            material_states,
            blocks_fluid_flow,
        )
    }
    /// The same blocks with the models and sprites of another resource root
    /// (resource packs changed): the registry and block facts come from the
    /// game data, which resource packs do not touch.
    pub fn reload(&self, root: &Path) -> Result<Self> {
        Self::with_models(
            root,
            self.registry,
            self.properties.clone(),
            self.material_states,
            self.blocks_fluid_flow.clone(),
        )
    }
    fn with_models(
        root: &Path,
        registry: &'static BlockRegistry,
        properties: Properties,
        material_states: [BlockStateId; 6],
        blocks_fluid_flow: BTreeSet<String>,
    ) -> Result<Self> {
        let mut sprites = crate::sprites::Sprites::default();
        for name in MATERIALS {
            sprites.insert(root, &format!("minecraft:block/{name}"))?;
        }
        let parsed = crate::model_library::ParsedModels::load(root);
        let models = crate::models::prepare_with_models(root, registry, &mut sprites, &parsed)?;
        let model_library = parsed.into_item_library();
        // Fluids are drawn by the fluid renderer, not block models; breaking
        // progress uses the destroy-stage overlays.
        for name in [
            "water_still",
            "water_flow",
            "water_overlay",
            "lava_still",
            "lava_flow",
        ] {
            sprites.insert(root, &format!("minecraft:block/{name}"))?;
        }
        for stage in 0..10 {
            sprites.insert(root, &format!("minecraft:block/destroy_stage_{stage}"))?;
        }
        sprites.finish();
        Ok(Self {
            registry,
            properties,
            material_states,
            sprite_size: sprites.size,
            sprite_minimum_size: sprites.minimum_size,
            texture_layers: u32::try_from(sprites.layers.len())?,
            pixels: sprites.pixels,
            model_catalog: models.catalog,
            model_library,
            model_placements: models.placements,
            model_gallery: models.states,
            animations: sprites.animations,
            sprite_layers: sprites.layers,
            translucent_layers: sprites.translucent,
            sprite_mips: sprites.mips,
            blocks_fluid_flow,
        })
    }
}

#[cfg(test)]
mod registry_cache_tests {
    use super::*;

    #[test]
    fn cache_preserves_registry_ids_and_invalidates_modified_facts() {
        let root =
            std::env::temp_dir().join(format!("horizon-registry-cache-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let first = br#"{"states":[["minecraft:stone",false,false,0,{"id":"minecraft:stone"}]],"defaults":{"minecraft:stone":0}}"#;
        let second = br#"{"states":[["minecraft:dirt",false,false,0,{"id":"minecraft:dirt"}]],"defaults":{"minecraft:dirt":0}}"#;
        for source in [first.as_slice(), first.as_slice(), second.as_slice()] {
            let facts: Value = serde_json::from_slice(source).unwrap();
            let registry = load_registry(&root, source, &facts).unwrap();
            let expected = if source == second {
                "minecraft:dirt"
            } else {
                "minecraft:stone"
            };
            assert_eq!(registry.default_state(expected), Some(BlockStateId(0)));
        }
        fs::write(root.join("block-registry-cache.nbt"), b"invalid cache").unwrap();
        let facts: Value = serde_json::from_slice(second).unwrap();
        assert_eq!(
            load_registry(&root, second, &facts)
                .unwrap()
                .default_state("minecraft:dirt"),
            Some(BlockStateId(0))
        );
        let invalid = br#"{"states":[["minecraft:stone",false,false,0,{"id":"minecraft:stone"}]],"defaults":{"minecraft:stone":1}}"#;
        let facts: Value = serde_json::from_slice(invalid).unwrap();
        assert!(load_registry(&root, invalid, &facts).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
