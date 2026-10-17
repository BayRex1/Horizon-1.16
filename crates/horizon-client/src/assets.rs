use crate::Result;
use horizon_world::{
    DATA_VERSION,
    chunk::BlockState,
    heightmap::HeightmapState,
    nbt::{self, Tag},
    registry::{BlockRegistry, BlockStateId},
    worldgen::{tags::TagDefinitions, terrain::TerrainBlockStates},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
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
// Cache data is versioned, bound to the exact source bytes and checksummed.
// Its typed payload skips intermediate NBT objects on subsequent starts.
const REGISTRY_CACHE_MAGIC: &[u8] = b"HORIZON-REGISTRY-CACHE\x02";
const REGISTRY_CACHE_NAME: &str = "block-registry-cache.bin";
static CACHE_TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn encode_registry(registry: &BlockRegistry) -> Option<Vec<u8>> {
    fn number(out: &mut Vec<u8>, n: usize) -> Option<()> {
        out.extend_from_slice(&u32::try_from(n).ok()?.to_le_bytes());
        Some(())
    }
    fn string(out: &mut Vec<u8>, value: &str) -> Option<()> {
        number(out, value.len())?;
        out.extend_from_slice(value.as_bytes());
        Some(())
    }
    let mut result = Vec::new();
    result.extend_from_slice(&DATA_VERSION.to_le_bytes());
    number(&mut result, registry.len())?;
    let mut names = BTreeSet::new();
    for i in 0..registry.len() {
        let state = registry.state(BlockStateId(i as u32))?;
        string(&mut result, &state.name)?;
        names.insert(state.name.as_str());
        number(&mut result, state.properties.len())?;
        for (key, value) in &state.properties {
            string(&mut result, key)?;
            string(&mut result, value)?;
        }
    }
    number(&mut result, names.len())?;
    for name in names {
        string(&mut result, name)?;
        result.extend_from_slice(&registry.default_state(name)?.0.to_le_bytes());
    }
    Some(result)
}

fn decode_registry(mut payload: &[u8]) -> Option<BlockRegistry> {
    fn number(bytes: &mut &[u8]) -> Option<u32> {
        let (head, rest) = bytes.split_at_checked(4)?;
        *bytes = rest;
        Some(u32::from_le_bytes(head.try_into().ok()?))
    }
    fn string(bytes: &mut &[u8]) -> Option<String> {
        let size = number(bytes)? as usize;
        let (head, rest) = bytes.split_at_checked(size)?;
        *bytes = rest;
        Some(std::str::from_utf8(head).ok()?.to_owned())
    }
    if number(&mut payload)? != DATA_VERSION as u32 {
        return None;
    }
    let count = number(&mut payload)? as usize;
    if count == 0 || count > 1_000_000 || count > payload.len() / 8 {
        return None;
    }
    let mut states = Vec::with_capacity(count);
    for _ in 0..count {
        let name = string(&mut payload)?;
        let num_properties = number(&mut payload)? as usize;
        if num_properties > payload.len() / 8 {
            return None;
        }
        let mut properties = BTreeMap::new();
        for _ in 0..num_properties {
            let key = string(&mut payload)?;
            let value = string(&mut payload)?;
            if properties.insert(key, value).is_some() {
                return None;
            }
        }
        states.push(BlockState { name, properties });
    }
    let count_defaults = number(&mut payload)? as usize;
    if count_defaults > states.len() || count_defaults > payload.len() / 8 {
        return None;
    }
    let mut defaults = BTreeMap::new();
    for _ in 0..count_defaults {
        let name = string(&mut payload)?;
        let id = BlockStateId(number(&mut payload)?);
        if defaults.insert(name, id).is_some() {
            return None;
        }
    }
    if !payload.is_empty() {
        return None;
    }
    BlockRegistry::from_parts(states, defaults).ok()
}

fn registry_cache_path(root: &Path) -> std::path::PathBuf {
    // Content-mod overlays are rebuilt by Overlay::new at every launch. Put
    // their derived registry alongside that disposable directory instead.
    if root.file_name().is_some_and(|name| name == "mod-content")
        && root.join("horizon-mod-content.json").is_file()
    {
        return root.with_file_name("mod-content-block-registry-cache.bin");
    }
    root.join(REGISTRY_CACHE_NAME)
}

fn cached_registry(root: &Path, digest: &[u8]) -> Option<BlockRegistry> {
    let path = registry_cache_path(root);
    // Reject unreasonable cache files before allocating from untrusted input.
    let header = REGISTRY_CACHE_MAGIC.len() + 64;
    let bound = (nbt::Limits::default().max_bytes + header) as u64;
    if fs::metadata(&path).ok()?.len() > bound {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(bound + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > bound {
        return None;
    }
    let (source_hash, rest) = bytes
        .strip_prefix(REGISTRY_CACHE_MAGIC)?
        .split_at_checked(32)?;
    let (payload_hash, payload) = rest.split_at_checked(32)?;
    if source_hash != digest || Sha256::digest(payload).as_slice() != payload_hash {
        return None;
    }
    decode_registry(payload)
}

fn persist_registry(root: &Path, digest: &[u8], registry: &BlockRegistry) {
    let Some(payload) = encode_registry(registry) else {
        return;
    };
    let mut bytes = Vec::with_capacity(REGISTRY_CACHE_MAGIC.len() + 64 + payload.len());
    bytes.extend_from_slice(REGISTRY_CACHE_MAGIC);
    bytes.extend_from_slice(digest);
    bytes.extend_from_slice(Sha256::digest(&payload).as_slice());
    bytes.extend_from_slice(&payload);
    let cache = registry_cache_path(root);
    // create_new prevents clobbering a competing writer's temporary data. The
    // same-directory rename publishes complete contents in one atomic step.
    let temp = cache.with_file_name(format!(
        ".{REGISTRY_CACHE_NAME}-{}-{}.tmp",
        std::process::id(),
        CACHE_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    if let Ok(mut file) = OpenOptions::new().write(true).create_new(true).open(&temp) {
        let written = file.write_all(&bytes).and_then(|_| file.sync_all()).is_ok();
        drop(file);
        if written {
            if fs::rename(&temp, &cache).is_ok() {
                // Best-effort directory sync: a crash may otherwise discard
                // the rename, in which case the next load safely rebuilds.
                if let Some(parent) = cache.parent()
                    && let Ok(directory) = File::open(parent)
                {
                    let _ = directory.sync_all();
                }
            }
        }
        let _ = fs::remove_file(temp);
    }
}

/// Load from the precomputed representation or reconstruct using the original
/// JSON-to-NBT conversion, retaining its errors and state ID ordering.
fn load_registry(root: &Path, facts_bytes: &[u8], facts: &Value) -> Result<BlockRegistry> {
    let digest = Sha256::digest(facts_bytes);
    if let Some(registry) = cached_registry(root, &digest) {
        return Ok(registry);
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
    persist_registry(root, &digest, &validated);
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
    use horizon_world::chunk::BlockState;
    use std::{collections::BTreeMap, path::PathBuf, sync::Barrier};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "horizon-registry-cache-{}-{}",
                std::process::id(),
                TEST_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn from_source(root: &Path, source: &[u8]) -> Result<BlockRegistry> {
        let facts: Value = serde_json::from_slice(source)?;
        load_registry(root, source, &facts)
    }

    // The original uncached algorithm is kept here as a behavioural oracle.
    fn original(source: &[u8]) -> Result<BlockRegistry> {
        let facts: Value = serde_json::from_slice(source)?;
        original_from_facts(&facts)
    }

    fn original_from_facts(facts: &Value) -> Result<BlockRegistry> {
        let states = facts["states"]
            .as_array()
            .ok_or_else(|| invalid("missing registry state facts"))?;
        Ok(BlockRegistry::from_nbt(&Tag::compound([
            ("DataVersion", Tag::Int(DATA_VERSION)),
            (
                "states",
                Tag::List(states.iter().map(|s| tag(&s[4])).collect::<Result<_>>()?),
            ),
            ("defaults", tag(&facts["defaults"])?),
        ]))?)
    }

    fn same_registry(a: &BlockRegistry, b: &BlockRegistry) {
        assert_eq!(a.len(), b.len());
        for i in 0..a.len() {
            let id = BlockStateId(i as u32);
            assert_eq!(a.state(id), b.state(id));
            assert_eq!(a.id(a.state(id).unwrap()), Some(id));
            assert_eq!(b.id(b.state(id).unwrap()), Some(id));
            assert_eq!(
                a.default_state(&a.state(id).unwrap().name),
                b.default_state(&b.state(id).unwrap().name)
            );
        }
    }

    const FIRST: &[u8] = br#"{"states":[["minecraft:stone",false,false,0,{"id":"minecraft:stone"}]],"defaults":{"minecraft:stone":0}}"#;
    const SECOND: &[u8] = br#"{"states":[["minecraft:dirt",false,false,0,{"id":"minecraft:dirt"}]],"defaults":{"minecraft:dirt":0}}"#;

    #[test]
    fn cache_preserves_registry_ids_and_invalidates_modified_facts() {
        let root = Scratch::new();
        for source in [FIRST, FIRST, SECOND] {
            let registry = from_source(&root.0, source).unwrap();
            let expected = if source == SECOND {
                "minecraft:dirt"
            } else {
                "minecraft:stone"
            };
            assert_eq!(registry.default_state(expected), Some(BlockStateId(0)));
            same_registry(&registry, &original(source).unwrap());
        }
        fs::write(root.0.join(REGISTRY_CACHE_NAME), b"invalid cache").unwrap();
        assert_eq!(
            from_source(&root.0, SECOND)
                .unwrap()
                .default_state("minecraft:dirt"),
            Some(BlockStateId(0))
        );
        let invalid = br#"{"states":[["minecraft:stone",false,false,0,{"id":"minecraft:stone"}]],"defaults":{"minecraft:stone":1}}"#;
        assert_eq!(
            from_source(&root.0, invalid).err().unwrap().to_string(),
            original(invalid).err().unwrap().to_string()
        );
    }

    #[test]
    fn cache_preserves_state_properties_defaults_and_malformed_source_errors() {
        let root = Scratch::new();
        let properties = br#"{"states":[["mod:door",false,false,0,{"id":"mod:door","properties":{"facing":"north","open":"false"}}],["mod:door",false,false,0,{"id":"mod:door","properties":{"open":"true","facing":"north"}}],["minecraft:stone",false,false,0,{"id":"minecraft:stone"}]],"defaults":{"mod:door":1,"minecraft:stone":2}}"#;
        let expected = original(properties).unwrap();
        for _ in 0..2 {
            let registry = from_source(&root.0, properties).unwrap();
            same_registry(&registry, &expected);
            assert_eq!(registry.default_state("mod:door"), Some(BlockStateId(1)));
            assert_eq!(
                registry
                    .resolve(&BlockState {
                        name: "mod:door".into(),
                        properties: BTreeMap::from([
                            ("facing".into(), "north".into()),
                            ("open".into(), "false".into())
                        ]),
                    })
                    .unwrap(),
                BlockStateId(0)
            );
        }
        let malformed: &[&[u8]] = &[
            br#"{"states":null,"defaults":{}}"#,
            br#"{"states":[],"defaults":{}}"#,
            br#"{"states":[[0,0,0,0,{"id":"mod:a"}],[0,0,0,0,{"id":"mod:a"}]],"defaults":{"mod:a":0}}"#,
            br#"{"states":[[0,0,0,0,{"id":"mod:a"}]],"defaults":{"mod:a":-1}}"#,
            br#"{"states":[[0,0,0,0,{"id":"mod:a"}]],"defaults":{"mod:a":1}}"#,
            br#"{"states":[[0,0,0,0,{"id":"mod:a"}]],"defaults":{}}"#,
            br#"{"states":[[0,0,0,0,{"id":"mod:a","properties":{"foo":true}}]],"defaults":{"mod:a":0}}"#,
            br#"{"states":[[0,0,0,0,{"id":"mod:a"}]],"defaults":{"mod:a":1.25}}"#,
            br#"{"states":[[0,0,0,0,{"id":"mod:a"}]],"defaults":{"mod:a":2147483648}}"#,
            b"{bad json",
        ];
        for source in malformed {
            assert_eq!(
                from_source(&root.0, source).err().unwrap().to_string(),
                original(source).err().unwrap().to_string()
            );
        }
        same_registry(&from_source(&root.0, properties).unwrap(), &expected);
    }

    #[test]
    fn cache_detects_valid_looking_tampering_truncation_and_excess_bytes() {
        let root = Scratch::new();
        from_source(&root.0, FIRST).unwrap();
        let cache = root.0.join(REGISTRY_CACHE_NAME);
        let original_bytes = fs::read(&cache).unwrap();
        // Replacing a name with another equally long name may leave valid NBT.
        let mut changed = original_bytes.clone();
        for match_at in changed
            .windows(5)
            .enumerate()
            .filter_map(|(i, w)| (w == b"stone").then_some(i))
            .collect::<Vec<_>>()
        {
            changed[match_at..match_at + 5].copy_from_slice(b"stove");
        }
        for bad in [
            changed,
            original_bytes[..original_bytes.len() - 1].to_vec(),
            [original_bytes.as_slice(), b"trailing"].concat(),
        ] {
            fs::write(&cache, bad).unwrap();
            assert!(cached_registry(&root.0, &Sha256::digest(FIRST)).is_none());
            same_registry(
                &from_source(&root.0, FIRST).unwrap(),
                &original(FIRST).unwrap(),
            );
        }
    }

    #[test]
    fn concurrent_cache_creation_publishes_complete_data_and_cleans_temporaries() {
        let root = Scratch::new();
        let gate = Arc::new(Barrier::new(8));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let gate = Arc::clone(&gate);
                let path = root.0.clone();
                scope.spawn(move || {
                    gate.wait();
                    for _ in 0..3 {
                        assert_eq!(
                            from_source(&path, FIRST)
                                .unwrap()
                                .default_state("minecraft:stone"),
                            Some(BlockStateId(0))
                        );
                    }
                });
            }
        });
        assert!(cached_registry(&root.0, &Sha256::digest(FIRST)).is_some());
        assert!(
            fs::read_dir(&root.0)
                .unwrap()
                .all(|f| f.unwrap().file_name() == REGISTRY_CACHE_NAME)
        );
    }

    #[test]
    fn cache_write_failure_still_loads_original_registry() {
        let root = Scratch::new();
        fs::create_dir(root.0.join(REGISTRY_CACHE_NAME)).unwrap();
        same_registry(
            &from_source(&root.0, FIRST).unwrap(),
            &original(FIRST).unwrap(),
        );
        same_registry(
            &from_source(&root.0, SECOND).unwrap(),
            &original(SECOND).unwrap(),
        );
        assert!(
            fs::read_dir(&root.0).unwrap().all(|f| !f
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }

    #[test]
    fn mod_overlay_rebuild_retains_cache_outside_disposable_resource_root() {
        let scratch = Scratch::new();
        let overlay = scratch.0.join("mod-content");
        fs::create_dir(&overlay).unwrap();
        fs::write(overlay.join("horizon-mod-content.json"), br#"["ruby"]"#).unwrap();
        let cache = scratch.0.join("mod-content-block-registry-cache.bin");
        assert_eq!(registry_cache_path(&overlay), cache);
        same_registry(
            &from_source(&overlay, FIRST).unwrap(),
            &original(FIRST).unwrap(),
        );
        assert!(cache.exists());
        fs::remove_dir_all(&overlay).unwrap();
        fs::create_dir(&overlay).unwrap();
        fs::write(overlay.join("horizon-mod-content.json"), br#"["ruby"]"#).unwrap();
        assert!(cached_registry(&overlay, &Sha256::digest(FIRST)).is_some());
        same_registry(
            &from_source(&overlay, SECOND).unwrap(),
            &original(SECOND).unwrap(),
        );
        assert!(cached_registry(&overlay, &Sha256::digest(FIRST)).is_none());
        assert!(cached_registry(&overlay, &Sha256::digest(SECOND)).is_some());
    }

    #[test]
    fn missing_or_invalid_source_files_still_fail_before_using_cache() {
        let root = Scratch::new();
        fs::write(
            root.0.join("horizon-resources.json"),
            format!(r#"{{"data_version":{DATA_VERSION}}}"#),
        )
        .unwrap();
        from_source(&root.0, FIRST).unwrap();
        let missing = Assets::load(&root.0).err().unwrap();
        assert_eq!(
            missing.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::NotFound
        );
        fs::write(root.0.join("facts.json"), b"invalid JSON").unwrap();
        let invalid_json = Assets::load(&root.0).err().unwrap();
        assert!(invalid_json.downcast_ref::<serde_json::Error>().is_some());
    }

    #[cfg(unix)]
    #[test]
    fn read_only_resource_root_does_not_require_cache_persistence() {
        use std::os::unix::fs::PermissionsExt;
        let root = Scratch::new();
        fs::set_permissions(&root.0, fs::Permissions::from_mode(0o555)).unwrap();
        same_registry(
            &from_source(&root.0, FIRST).unwrap(),
            &original(FIRST).unwrap(),
        );
        fs::set_permissions(&root.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!root.0.join(REGISTRY_CACHE_NAME).exists());
    }

    #[test]
    #[ignore = "run explicitly with HORIZON_REGISTRY_BENCH_FACTS and an idle host"]
    fn benchmark_registry_conversion_only() {
        use std::{
            hint::black_box,
            time::{Duration, Instant},
        };
        let input = std::env::var("HORIZON_REGISTRY_BENCH_FACTS")
            .expect("set HORIZON_REGISTRY_BENCH_FACTS to the pinned facts.json");
        let bytes = fs::read(input).unwrap();
        let facts: Value = serde_json::from_slice(&bytes).unwrap();
        let root = Scratch::new();
        let reference = original_from_facts(&facts).unwrap();
        same_registry(&reference, &load_registry(&root.0, &bytes, &facts).unwrap());

        let mut legacy = Vec::<f64>::new();
        let mut cold = Vec::<f64>::new();
        let mut warm = Vec::<f64>::new();
        let measure = |f: &mut dyn FnMut() -> BlockRegistry| -> f64 {
            let start = Instant::now();
            black_box(f());
            start.elapsed().as_secs_f64() * 1000.0
        };
        for i in 0..20 {
            // Alternate path order to minimise cache-temperature bias.
            if i % 2 == 0 {
                legacy.push(measure(&mut || {
                    original_from_facts(black_box(&facts)).unwrap()
                }));
                warm.push(measure(&mut || {
                    load_registry(&root.0, black_box(&bytes), black_box(&facts)).unwrap()
                }));
            } else {
                warm.push(measure(&mut || {
                    load_registry(&root.0, black_box(&bytes), black_box(&facts)).unwrap()
                }));
                legacy.push(measure(&mut || {
                    original_from_facts(black_box(&facts)).unwrap()
                }));
            }
            fs::remove_file(root.0.join(REGISTRY_CACHE_NAME)).unwrap();
            cold.push(measure(&mut || {
                load_registry(&root.0, black_box(&bytes), black_box(&facts)).unwrap()
            }));
            std::thread::sleep(Duration::from_millis(5));
        }
        fn report(name: &str, mut samples: Vec<f64>) {
            samples.sort_by(f64::total_cmp);
            let median = (samples[9] + samples[10]) / 2.0;
            println!(
                "{name}_median_ms={median:.3} {name}_min_ms={:.3} {name}_max_ms={:.3} {name}_samples_ms={samples:?}",
                samples[0], samples[19]
            );
        }
        report("registry_original", legacy);
        report("registry_cold", cold);
        report("registry_warm", warm);
    }
}
