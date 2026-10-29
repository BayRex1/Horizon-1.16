//! Version-pinned reference registries for the Minecraft Java 1.16.5 port.
//!
//! The records originate from PrismarineJS minecraft-data's Java 1.16.2
//! registry dump, which is the nearest checked-in version in that project.
//! Minecraft 1.16.3–1.16.5 did not add block types, but callers must not treat
//! these protocol IDs as Horizon's modern registry IDs or as worldgen parity.
//! Keep this module isolated from the 26.3 registry until the port is wired in.
use serde::Deserialize;

const BLOCKS_JSON: &str = include_str!("../data/legacy_1_16_5/blocks.json");
const BIOMES_JSON: &str = include_str!("../data/legacy_1_16_5/biomes.json");

/// Overworld vertical geometry in Minecraft Java 1.16.5. Keep this separate
/// from the modern world's -64..319 build range.
pub const MIN_BUILD_Y: i32 = 0;
pub const BUILD_HEIGHT: i32 = 256;
pub const MAX_BUILD_Y: i32 = MIN_BUILD_Y + BUILD_HEIGHT - 1;
pub const SECTION_HEIGHT: i32 = 16;
pub const SECTION_COUNT: usize = (BUILD_HEIGHT / SECTION_HEIGHT) as usize;
pub const SEA_LEVEL: i32 = 63;

/// Convert a block Y coordinate into the 1.16.5 section slot (0..15).
pub const fn section_index(y: i32) -> Option<usize> {
    if y < MIN_BUILD_Y || y > MAX_BUILD_Y {
        None
    } else {
        Some(((y - MIN_BUILD_Y) / SECTION_HEIGHT) as usize)
    }
}

/// Convert a section slot (0..15) to its minimum block Y coordinate.
pub const fn section_min_y(index: usize) -> Option<i32> {
    if index >= SECTION_COUNT {
        None
    } else {
        Some(MIN_BUILD_Y + index as i32 * SECTION_HEIGHT)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct LegacyBlockProperty {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub num_values: u32,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyBlock {
    pub id: u32,
    pub name: String,
    pub display_name: String,
    pub min_state_id: u32,
    pub max_state_id: u32,
    pub default_state: u32,
    #[serde(default)]
    pub states: Vec<LegacyBlockProperty>,
    pub hardness: Option<f32>,
    pub resistance: Option<f32>,
    pub diggable: Option<bool>,
    pub transparent: Option<bool>,
    pub filter_light: Option<u8>,
    pub emit_light: Option<u8>,
    pub bounding_box: Option<String>,
    pub material: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyBiome {
    pub id: u32,
    pub name: String,
    pub category: Option<String>,
    pub temperature: f32,
    pub precipitation: Option<String>,
    pub depth: Option<f32>,
    pub dimension: Option<String>,
    pub display_name: Option<String>,
    pub color: Option<u32>,
    pub rainfall: Option<f32>,
}

pub fn blocks() -> Result<Vec<LegacyBlock>, serde_json::Error> {
    serde_json::from_str(BLOCKS_JSON)
}

pub fn biomes() -> Result<Vec<LegacyBiome>, serde_json::Error> {
    serde_json::from_str(BIOMES_JSON)
}

pub fn block_by_name(name: &str) -> Result<Option<LegacyBlock>, serde_json::Error> {
    Ok(blocks()?.into_iter().find(|block| block.name == name))
}

pub fn biome_by_name(name: &str) -> Result<Option<LegacyBiome>, serde_json::Error> {
    Ok(biomes()?.into_iter().find(|biome| biome.name == name))
}

/// Resolve a 1.16.5 block-state registry ID to its block and per-block state
/// offset. The offset is relative to min_state_id, not a modern Horizon ID.
pub fn block_by_state_id(state_id: u32) -> Result<Option<(LegacyBlock, u32)>, serde_json::Error> {
    Ok(blocks()?.into_iter().find_map(|block| {
        if block.min_state_id <= state_id && state_id <= block.max_state_id {
            let offset = state_id - block.min_state_id;
            Some((block, offset))
        } else {
            None
        }
    }))
}

/// Resolve a biome's protocol-era numeric registry ID.
pub fn biome_by_id(id: u32) -> Result<Option<LegacyBiome>, serde_json::Error> {
    Ok(biomes()?.into_iter().find(|biome| biome.id == id))
}

/// One Java Edition 1.16.5 chunk section (16×16×16 blocks).
///
/// Serialized fields use the legacy Anvil format: Palette entries use Name/Properties,
/// and BlockStates stores padded 64-bit words. This does not use the modern
/// chunk::PalettedContainer, whose NBT schema and minimum widths differ.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LegacyBlockState {
    pub name: String,
    pub properties: std::collections::BTreeMap<String, String>,
}

impl LegacyBlockState {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            properties: std::collections::BTreeMap::new(),
        }
    }

    fn to_nbt(&self) -> crate::nbt::Tag {
        use crate::nbt::{JavaString, Tag};
        let mut map = crate::nbt::Compound::new();
        let name = if self.name.contains(':') {
            self.name.clone()
        } else {
            format!("minecraft:{}", self.name)
        };
        map.insert(
            JavaString::from("Name"),
            Tag::String(JavaString::from(name.as_str())),
        );
        if !self.properties.is_empty() {
            map.insert(
                JavaString::from("Properties"),
                Tag::Compound(
                    self.properties
                        .iter()
                        .map(|(key, value)| {
                            (
                                JavaString::from(key.as_str()),
                                Tag::String(JavaString::from(value.as_str())),
                            )
                        })
                        .collect(),
                ),
            );
        }
        Tag::Compound(map)
    }

    fn from_nbt(tag: &crate::nbt::Tag) -> std::io::Result<Self> {
        use crate::compression::invalid;
        use crate::nbt::Tag;
        let Some(Tag::String(name)) = tag.get("Name") else {
            return Err(invalid("legacy block palette entry is missing Name"));
        };
        let mut name = name.to_string().map_err(|e| invalid(e.to_string()))?;
        if let Some(stripped) = name.strip_prefix("minecraft:") {
            name = stripped.to_owned();
        }
        if name.is_empty() || name.contains(':') {
            return Err(invalid("invalid legacy block name"));
        }
        let mut properties = std::collections::BTreeMap::new();
        if let Some(value) = tag.get("Properties") {
            let Tag::Compound(entries) = value else {
                return Err(invalid("legacy block Properties is not a compound"));
            };
            for (key, value) in entries {
                let Tag::String(value) = value else {
                    return Err(invalid("legacy block property is not a string"));
                };
                properties.insert(
                    key.to_string().map_err(|e| invalid(e.to_string()))?,
                    value.to_string().map_err(|e| invalid(e.to_string()))?,
                );
            }
        }
        Ok(Self { name, properties })
    }
}

/// Legacy 1.16.5 palette-backed block storage for a single section.
///
/// Index order follows vanilla: x | (z << 4) | (y << 8). Values never cross
/// a long boundary, matching Java 1.16.5 SimpleBitStorage.
#[derive(Clone, Debug)]
pub struct LegacyBlockSection {
    palette: Vec<LegacyBlockState>,
    storage: Option<horizon_core::storage::PackedStorage>,
}

impl LegacyBlockSection {
    pub const BLOCK_COUNT: usize = 4096;

    pub fn uniform(state: LegacyBlockState) -> Self {
        Self {
            palette: vec![state],
            storage: None,
        }
    }

    pub fn air() -> Self {
        Self::uniform(LegacyBlockState::new("air"))
    }

    pub fn palette(&self) -> &[LegacyBlockState] {
        &self.palette
    }

    pub fn get_index(&self, index: usize) -> std::io::Result<&LegacyBlockState> {
        use crate::compression::invalid;
        if index >= Self::BLOCK_COUNT {
            return Err(invalid("legacy section block index out of bounds"));
        }
        let palette_index = self
            .storage
            .as_ref()
            .map_or(0, |storage| storage.read(index) as usize);
        self.palette
            .get(palette_index)
            .ok_or_else(|| invalid("legacy palette index outside palette"))
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> std::io::Result<&LegacyBlockState> {
        if x >= 16 || y >= 16 || z >= 16 {
            return Err(crate::compression::invalid(
                "legacy section coordinates out of bounds",
            ));
        }
        self.get_index(x | (z << 4) | (y << 8))
    }

    pub fn set_index(&mut self, index: usize, state: LegacyBlockState) -> std::io::Result<()> {
        use crate::compression::invalid;
        use horizon_core::storage::PackedStorage;
        if index >= Self::BLOCK_COUNT {
            return Err(invalid("legacy section block index out of bounds"));
        }
        let palette_index = match self.palette.iter().position(|entry| entry == &state) {
            Some(index) => index,
            None => {
                let next_len = self.palette.len() + 1;
                let next_bits = ((usize::BITS - (next_len - 1).leading_zeros()) as u8).max(4);
                if self
                    .storage
                    .as_ref()
                    .is_none_or(|storage| storage.bits() < next_bits)
                {
                    let mut next = PackedStorage::new(next_bits, Self::BLOCK_COUNT)
                        .map_err(|e| invalid(e.to_string()))?;
                    if let Some(previous) = &self.storage {
                        for i in 0..Self::BLOCK_COUNT {
                            next.set(i, previous.read(i))
                                .map_err(|e| invalid(e.to_string()))?;
                        }
                    }
                    self.storage = Some(next);
                }
                self.palette.push(state);
                self.palette.len() - 1
            }
        };
        if let Some(storage) = &mut self.storage {
            storage
                .set(index, palette_index as u32)
                .map_err(|e| invalid(e.to_string()))?;
        }
        Ok(())
    }

    pub fn set(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        state: LegacyBlockState,
    ) -> std::io::Result<()> {
        if x >= 16 || y >= 16 || z >= 16 {
            return Err(crate::compression::invalid(
                "legacy section coordinates out of bounds",
            ));
        }
        self.set_index(x | (z << 4) | (y << 8), state)
    }

    /// Decode the legacy section's Palette and optional BlockStates fields.
    pub fn from_nbt(tag: &crate::nbt::Tag) -> std::io::Result<Self> {
        use crate::compression::invalid;
        use crate::nbt::Tag;
        let Some(Tag::List(entries)) = tag.get("Palette") else {
            return Err(invalid("legacy section is missing Palette list"));
        };
        if entries.is_empty() {
            return Err(invalid("legacy section palette is empty"));
        }
        let palette = entries
            .iter()
            .map(LegacyBlockState::from_nbt)
            .collect::<std::io::Result<Vec<_>>>()?;
        if palette.len() > Self::BLOCK_COUNT {
            return Err(invalid("legacy section palette exceeds block count"));
        }
        if palette.len() == 1 {
            if let Some(Tag::LongArray(words)) = tag.get("BlockStates") {
                if !words.is_empty() {
                    return Err(invalid(
                        "single-entry legacy palette must not contain BlockStates",
                    ));
                }
            }
            return Ok(Self {
                palette,
                storage: None,
            });
        }
        let bits = ((usize::BITS - (palette.len() - 1).leading_zeros()) as u8).max(4);
        let Some(Tag::LongArray(words)) = tag.get("BlockStates") else {
            return Err(invalid("nonuniform legacy palette is missing BlockStates"));
        };
        let storage = horizon_core::storage::PackedStorage::from_words(
            bits,
            Self::BLOCK_COUNT,
            words.iter().map(|word| *word as u64).collect(),
        )
        .map_err(|e| invalid(e.to_string()))?;
        for index in 0..Self::BLOCK_COUNT {
            if storage.read(index) as usize >= palette.len() {
                return Err(invalid("legacy palette index outside palette"));
            }
        }
        Ok(Self {
            palette,
            storage: Some(storage),
        })
    }

    /// Encode the section fields to embed into a 1.16.5 section compound.
    pub fn to_nbt(&self) -> crate::nbt::Tag {
        use crate::nbt::{JavaString, Tag};
        let mut map = crate::nbt::Compound::new();
        map.insert(
            JavaString::from("Palette"),
            Tag::List(self.palette.iter().map(LegacyBlockState::to_nbt).collect()),
        );
        if let Some(storage) = &self.storage {
            map.insert(
                JavaString::from("BlockStates"),
                Tag::LongArray(storage.words().iter().map(|word| *word as i64).collect()),
            );
        }
        Tag::Compound(map)
    }

    /// Translate this legacy section to the current runtime palette schema.
    /// The section is not written back to the legacy save by this conversion.
    pub fn to_runtime_nbt(&self) -> crate::nbt::Tag {
        use crate::nbt::{JavaString, Tag};
        let palette = self
            .palette
            .iter()
            .map(|state| {
                let name = if state.name.contains(':') {
                    state.name.clone()
                } else {
                    format!("minecraft:{}", state.name)
                };
                crate::chunk::BlockState {
                    name,
                    properties: state.properties.clone(),
                }
                .to_nbt()
            })
            .collect();
        let mut map = crate::nbt::Compound::new();
        map.insert(JavaString::from("palette"), Tag::List(palette));
        if let Some(storage) = &self.storage {
            map.insert(
                JavaString::from("data"),
                Tag::LongArray(storage.words().iter().map(|word| *word as i64).collect()),
            );
        }
        Tag::Compound(map)
    }
}

/// Adapt a 1.16.5 saved chunk for the current Horizon runtime's block access.
///
/// This is an in-memory schema bridge only: it does not alter the source NBT or
/// claim that 1.16.5 chunks can be written back as modern chunks unchanged.
pub fn to_runtime_chunk(root: &crate::nbt::NamedTag) -> std::io::Result<crate::chunk::ChunkData> {
    use crate::{
        compression::invalid,
        nbt::{Compound, JavaString, NamedTag, Tag},
    };

    let Some(Tag::Int(version)) = root.tag.get("DataVersion") else {
        return Err(invalid("legacy chunk is missing DataVersion"));
    };
    if *version != 2586 {
        return Err(invalid(format!(
            "expected legacy DataVersion 2586, got {version}"
        )));
    }
    let Some(level) = root.tag.get("Level") else {
        return Err(invalid("legacy chunk is missing Level compound"));
    };
    let Some(Tag::Int(x)) = level.get("xPos") else {
        return Err(invalid("legacy chunk is missing xPos"));
    };
    let Some(Tag::Int(z)) = level.get("zPos") else {
        return Err(invalid("legacy chunk is missing zPos"));
    };
    let Some(Tag::List(legacy_sections)) = level.get("Sections") else {
        return Err(invalid("legacy chunk is missing Sections list"));
    };

    let mut sections = Vec::with_capacity(legacy_sections.len());
    for legacy in legacy_sections {
        let Some(Tag::Byte(y)) = legacy.get("Y") else {
            return Err(invalid("legacy section is missing Y"));
        };
        let block_section = LegacyBlockSection::from_nbt(legacy)?;
        let mut fields = Compound::new();
        fields.insert(JavaString::from("Y"), Tag::Byte(*y));
        fields.insert(
            JavaString::from("block_states"),
            block_section.to_runtime_nbt(),
        );
        // Modern runtime section loading still uses these vanilla light arrays.
        for field in ["BlockLight", "SkyLight"] {
            if let Some(value) = legacy.get(field) {
                fields.insert(JavaString::from(field), value.clone());
            }
        }
        fields.insert(
            JavaString::from("biomes"),
            Tag::compound([(
                "palette",
                Tag::List(vec![Tag::String(JavaString::from("minecraft:plains"))]),
            )]),
        );
        sections.push(Tag::Compound(fields));
    }

    let mut fields = Compound::new();
    fields.insert(
        JavaString::from("DataVersion"),
        Tag::Int(crate::DATA_VERSION),
    );
    fields.insert(JavaString::from("xPos"), Tag::Int(*x));
    fields.insert(JavaString::from("zPos"), Tag::Int(*z));
    fields.insert(JavaString::from("sections"), Tag::List(sections));
    for field in [
        "Heightmaps",
        "Status",
        "LastUpdate",
        "InhabitedTime",
        "isLightOn",
    ] {
        if let Some(value) = level.get(field) {
            fields.insert(JavaString::from(field), value.clone());
        }
    }
    if let Some(Tag::List(entities)) = level.get("TileEntities") {
        fields.insert(
            JavaString::from("block_entities"),
            Tag::List(entities.clone()),
        );
    } else {
        fields.insert(JavaString::from("block_entities"), Tag::List(Vec::new()));
    }
    fields.insert(JavaString::from("block_ticks"), Tag::List(Vec::new()));
    fields.insert(JavaString::from("fluid_ticks"), Tag::List(Vec::new()));

    crate::chunk::ChunkData::new(NamedTag {
        name: root.name.clone(),
        tag: Tag::Compound(fields),
    })
}

fn terrain_hash(seed: i64, x: i32, z: i32) -> u64 {
    let mut value = (seed as u64)
        ^ (x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (z as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn lattice_noise(seed: i64, x: i32, z: i32) -> f64 {
    (terrain_hash(seed, x, z) as f64 / u64::MAX as f64) * 2.0 - 1.0
}

fn smooth_noise(seed: i64, x: i32, z: i32, scale: i32) -> f64 {
    let gx = x.div_euclid(scale);
    let gz = z.div_euclid(scale);
    let tx = x.rem_euclid(scale) as f64 / scale as f64;
    let tz = z.rem_euclid(scale) as f64 / scale as f64;
    let sx = tx * tx * (3.0 - 2.0 * tx);
    let sz = tz * tz * (3.0 - 2.0 * tz);
    let a = lattice_noise(seed, gx, gz);
    let b = lattice_noise(seed, gx + 1, gz);
    let c = lattice_noise(seed, gx, gz + 1);
    let d = lattice_noise(seed, gx + 1, gz + 1);
    let north = a + (b - a) * sx;
    let south = c + (d - c) * sx;
    north + (south - north) * sz
}

fn terrain_height(seed: i64, x: i32, z: i32) -> i32 {
    (64.0 + smooth_noise(seed, x, z, 48) * 6.0 + smooth_noise(seed ^ 0x5deece66d, x, z, 16) * 2.0)
        .round()
        .clamp(55.0, 73.0) as i32
}

/// Build a deterministic, playable-bootstrap terrain chunk in the Java 1.16.5
/// Anvil NBT schema. This is intentionally a simple seeded height field, not
/// claimed to match vanilla's multi-noise terrain generator bit-for-bit.
pub fn generate_bootstrap_chunk(seed: i64, chunk_x: i32, chunk_z: i32) -> crate::nbt::NamedTag {
    use crate::nbt::{JavaString, NamedTag, Tag};
    use horizon_core::storage::PackedStorage;

    fn compound(entries: impl IntoIterator<Item = (String, Tag)>) -> Tag {
        Tag::Compound(
            entries
                .into_iter()
                .map(|(key, value)| (JavaString::from(key.as_str()), value))
                .collect(),
        )
    }
    let air = LegacyBlockState::new("air");
    let bedrock = LegacyBlockState::new("bedrock");
    let stone = LegacyBlockState::new("stone");
    let dirt = LegacyBlockState::new("dirt");
    let grass = LegacyBlockState::new("grass_block");
    let water = LegacyBlockState::new("water");
    let mut sections: Vec<LegacyBlockSection> = (0..SECTION_COUNT)
        .map(|_| LegacyBlockSection::uniform(air.clone()))
        .collect();
    let mut terrain_heights = vec![0i32; 256];
    let mut surface = vec![0i32; 256];

    for z in 0..16 {
        for x in 0..16 {
            let world_x = chunk_x.wrapping_mul(16).wrapping_add(x as i32);
            let world_z = chunk_z.wrapping_mul(16).wrapping_add(z as i32);
            let height = terrain_height(seed, world_x, world_z);
            let column = x | (z << 4);
            terrain_heights[column] = height;
            surface[column] = height.max(SEA_LEVEL) + 1;
            for y in 0..=height {
                let state = if y == 0 {
                    &bedrock
                } else if y == height {
                    &grass
                } else if y >= height - 2 {
                    &dirt
                } else {
                    &stone
                };
                sections[(y / 16) as usize]
                    .set(x, (y % 16) as usize, z, state.clone())
                    .expect("generated section coordinates are in bounds");
            }
            for y in height + 1..=SEA_LEVEL {
                sections[(y / 16) as usize]
                    .set(x, (y % 16) as usize, z, water.clone())
                    .expect("generated water coordinates are in bounds");
            }
        }
    }

    let mut section_tags = Vec::with_capacity(SECTION_COUNT);
    for (index, section) in sections.iter().enumerate() {
        let Tag::Compound(mut fields) = section.to_nbt() else {
            unreachable!()
        };
        fields.insert(JavaString::from("Y"), Tag::Byte(index as i8));
        fields.insert(
            JavaString::from("BlockLight"),
            Tag::ByteArray(vec![0; 2048]),
        );
        let mut sky_light = vec![0u8; 2048];
        for local_y in 0..16usize {
            let world_y = index as i32 * SECTION_HEIGHT + local_y as i32;
            for z in 0..16usize {
                for x in 0..16usize {
                    let column_height = terrain_heights[x | (z << 4)];
                    let light = if world_y <= column_height {
                        0
                    } else if world_y <= SEA_LEVEL {
                        (world_y - SEA_LEVEL + 14).clamp(0, 15) as u8
                    } else {
                        15
                    };
                    let block_index = x | (z << 4) | (local_y << 8);
                    if block_index & 1 == 0 {
                        sky_light[block_index >> 1] |= light;
                    } else {
                        sky_light[block_index >> 1] |= light << 4;
                    }
                }
            }
        }
        fields.insert(JavaString::from("SkyLight"), Tag::ByteArray(sky_light));
        section_tags.push(Tag::Compound(fields));
    }

    let mut heightmap = PackedStorage::new(9, 256).expect("valid legacy heightmap storage");
    for (index, height) in surface.iter().enumerate() {
        heightmap
            .set(index, *height as u32)
            .expect("height fits 9 bits");
    }
    let heightmap_data =
        Tag::LongArray(heightmap.words().iter().map(|word| *word as i64).collect());
    let heightmaps = compound([
        ("MOTION_BLOCKING".to_owned(), heightmap_data.clone()),
        ("WORLD_SURFACE".to_owned(), heightmap_data),
    ]);
    let post_processing = Tag::List((0..SECTION_COUNT).map(|_| Tag::List(Vec::new())).collect());
    let structures = compound([
        ("Starts".to_owned(), compound(std::iter::empty())),
        ("References".to_owned(), compound(std::iter::empty())),
    ]);
    let level = compound([
        ("xPos".to_owned(), Tag::Int(chunk_x)),
        ("zPos".to_owned(), Tag::Int(chunk_z)),
        ("LastUpdate".to_owned(), Tag::Long(0)),
        ("InhabitedTime".to_owned(), Tag::Long(0)),
        (
            "Status".to_owned(),
            Tag::String(JavaString::from("minecraft:full")),
        ),
        ("Biomes".to_owned(), Tag::IntArray(vec![1; 256])),
        ("Heightmaps".to_owned(), heightmaps),
        ("Sections".to_owned(), Tag::List(section_tags)),
        ("Entities".to_owned(), Tag::List(Vec::new())),
        ("TileEntities".to_owned(), Tag::List(Vec::new())),
        ("TileTicks".to_owned(), Tag::List(Vec::new())),
        ("LiquidTicks".to_owned(), Tag::List(Vec::new())),
        ("PostProcessing".to_owned(), post_processing),
        ("Structures".to_owned(), structures),
        ("LightPopulated".to_owned(), Tag::Byte(1)),
        ("TerrainPopulated".to_owned(), Tag::Byte(1)),
        ("isLightOn".to_owned(), Tag::Byte(1)),
    ]);
    NamedTag {
        name: JavaString::default(),
        tag: compound([
            ("DataVersion".to_owned(), Tag::Int(2586)),
            ("Level".to_owned(), level),
        ]),
    }
}

/// Create a self-contained Java 1.16.5 bootstrap world folder.
///
/// This is a deliberately small bootstrap generator, not vanilla's exact
/// multi-noise terrain algorithm. It writes level.dat, session.lock, and a
/// 5×5 spawn area of legacy-format chunks so the world has room around spawn.
pub fn create_bootstrap_world(
    directory: impl AsRef<std::path::Path>,
    name: &str,
    seed: i64,
) -> std::io::Result<()> {
    use crate::{
        compression::Compression,
        nbt::{self, JavaString, Limits, NamedTag, Tag},
    };
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        time::{SystemTime, UNIX_EPOCH},
    };

    let directory = directory.as_ref();
    if name.trim().is_empty() {
        return Err(crate::compression::invalid("world name must not be empty"));
    }
    fs::create_dir(directory)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| crate::compression::invalid(e.to_string()))?
        .as_millis()
        .min(i64::MAX as u128) as i64;

    let string = |value: &str| Tag::String(JavaString::from(value));
    let compound = |entries: Vec<(&str, Tag)>| Tag::compound(entries);
    let dimension = |dimension_type: &str, settings: &str, biome: &str| {
        compound(vec![
            ("type", string(dimension_type)),
            (
                "generator",
                compound(vec![
                    ("type", string("minecraft:noise")),
                    ("seed", Tag::Long(seed)),
                    ("settings", string(settings)),
                    (
                        "biome_source",
                        compound(vec![
                            ("type", string("minecraft:fixed")),
                            ("biome", string(biome)),
                        ]),
                    ),
                ]),
            ),
        ])
    };
    let dimensions = compound(vec![
        (
            "minecraft:overworld",
            compound(vec![
                ("type", string("minecraft:overworld")),
                (
                    "generator",
                    compound(vec![
                        ("type", string("minecraft:noise")),
                        ("seed", Tag::Long(seed)),
                        ("settings", string("minecraft:overworld")),
                        (
                            "biome_source",
                            compound(vec![
                                ("type", string("minecraft:vanilla_layered")),
                                ("seed", Tag::Long(seed)),
                                ("large_biomes", Tag::Byte(0)),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        (
            "minecraft:the_nether",
            dimension(
                "minecraft:the_nether",
                "minecraft:nether",
                "minecraft:nether_wastes",
            ),
        ),
        (
            "minecraft:the_end",
            dimension("minecraft:the_end", "minecraft:end", "minecraft:the_end"),
        ),
    ]);
    let game_rules = compound(vec![
        ("doDaylightCycle", string("true")),
        ("doWeatherCycle", string("true")),
        ("doMobSpawning", string("true")),
        ("mobGriefing", string("true")),
        ("keepInventory", string("false")),
        ("doFireTick", string("true")),
        ("randomTickSpeed", string("3")),
        ("spawnRadius", string("10")),
        ("reducedDebugInfo", string("false")),
        ("naturalRegeneration", string("true")),
        ("doImmediateRespawn", string("false")),
        ("announceAdvancements", string("true")),
    ]);
    let world_gen = compound(vec![
        ("seed", Tag::Long(seed)),
        ("generate_features", Tag::Byte(1)),
        ("bonus_chest", Tag::Byte(0)),
        ("dimensions", dimensions),
    ]);
    let data_packs = compound(vec![
        ("Enabled", Tag::List(vec![string("vanilla")])),
        ("Disabled", Tag::List(Vec::new())),
    ]);
    let spawn_y = terrain_height(seed, 8, 8).max(SEA_LEVEL) + 1;
    let data = compound(vec![
        ("DataVersion", Tag::Int(2586)),
        ("LevelName", string(name)),
        ("version", Tag::Int(19133)),
        (
            "Version",
            compound(vec![
                ("Id", Tag::Int(2586)),
                ("Name", string("1.16.5")),
                ("Snapshot", Tag::Byte(0)),
            ]),
        ),
        ("GameType", Tag::Int(0)),
        ("MapFeatures", Tag::Byte(1)),
        ("hardcore", Tag::Byte(0)),
        ("allowCommands", Tag::Byte(0)),
        ("Difficulty", Tag::Byte(2)),
        ("DifficultyLocked", Tag::Byte(0)),
        ("initialized", Tag::Byte(1)),
        ("SpawnX", Tag::Int(8)),
        ("SpawnY", Tag::Int(spawn_y)),
        ("SpawnZ", Tag::Int(8)),
        ("Time", Tag::Long(0)),
        ("DayTime", Tag::Long(0)),
        ("LastPlayed", Tag::Long(now)),
        ("SizeOnDisk", Tag::Long(0)),
        ("RandomSeed", Tag::Long(seed)),
        ("raining", Tag::Byte(0)),
        ("rainTime", Tag::Int(0)),
        ("thundering", Tag::Byte(0)),
        ("thunderTime", Tag::Int(0)),
        ("clearWeatherTime", Tag::Int(0)),
        ("GameRules", game_rules),
        ("WorldGenSettings", world_gen),
        ("DataPacks", data_packs),
        ("WanderingTraderSpawnDelay", Tag::Int(0)),
        ("WanderingTraderSpawnChance", Tag::Int(25)),
    ]);
    let root = NamedTag {
        name: JavaString::default(),
        tag: compound(vec![("DataVersion", Tag::Int(2586)), ("Data", data)]),
    };
    let encoded = Compression::Gzip.encode(
        &nbt::write_named(&root, Limits::default())
            .map_err(|e| crate::compression::invalid(e.to_string()))?,
    )?;
    let mut level = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("level.dat"))?;
    level.write_all(&encoded)?;
    level.sync_all()?;

    let mut lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("session.lock"))?;
    lock.write_all(&now.to_be_bytes())?;
    lock.sync_all()?;

    let region_directory = directory.join("region");
    fs::create_dir(&region_directory)?;
    for chunk_z in -2..=2 {
        for chunk_x in -2..=2 {
            save_bootstrap_chunk(&region_directory, seed, chunk_x, chunk_z)?;
        }
    }
    Ok(())
}

/// Persist one generated chunk to its Anvil region file using the 1.16.5
/// gzip/zlib-compatible region container. Existing chunk records are replaced
/// by RegionFile's normal in-place write path.
pub fn save_bootstrap_chunk(
    directory: impl AsRef<std::path::Path>,
    seed: i64,
    chunk_x: i32,
    chunk_z: i32,
) -> std::io::Result<()> {
    use crate::{
        compression::Compression,
        nbt::Limits,
        region::{ChunkRecord, RegionFile},
    };
    use horizon_core::position::ChunkPos;
    let chunk_pos = ChunkPos::new(chunk_x, chunk_z);
    let region_pos = chunk_pos.region();
    let directory = directory.as_ref();
    std::fs::create_dir_all(directory)?;
    let path = directory.join(format!("r.{}.{}.mca", region_pos.x, region_pos.z));
    let root = generate_bootstrap_chunk(seed, chunk_x, chunk_z);
    let record = ChunkRecord::from_nbt(&root, Compression::Zlib, 0, Limits::default())?;
    let mut region = RegionFile::open_writable(path, true)?;
    region.write(chunk_pos, &record)?;
    region.sync()
}

/// Load one saved 1.16.5 chunk from a world folder and adapt it for runtime block access.
pub fn load_runtime_chunk(
    world_directory: impl AsRef<std::path::Path>,
    chunk_x: i32,
    chunk_z: i32,
) -> std::io::Result<crate::chunk::ChunkData> {
    use crate::{compression::invalid, nbt::Limits, region::RegionFile};
    use horizon_core::position::ChunkPos;

    let chunk_pos = ChunkPos::new(chunk_x, chunk_z);
    let region_pos = chunk_pos.region();
    let path = world_directory
        .as_ref()
        .join("region")
        .join(format!("r.{}.{}.mca", region_pos.x, region_pos.z));
    let mut region = RegionFile::open(path)?;
    let record = region
        .read(chunk_pos)?
        .ok_or_else(|| invalid("legacy chunk is not present in world region"))?;
    let root = record.nbt(Limits::default())?;
    to_runtime_chunk(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_chunk_is_seeded_and_uses_1165_root_schema() {
        use crate::nbt::{self, Limits, Tag};
        let first = generate_bootstrap_chunk(1234, 0, 0);
        let same = generate_bootstrap_chunk(1234, 0, 0);
        let other = generate_bootstrap_chunk(5678, 0, 0);
        assert_eq!(first, same);
        assert_ne!(first, other);
        assert_eq!(first.tag.get("DataVersion"), Some(&Tag::Int(2586)));
        let level = first.tag.get("Level").expect("chunk Level");
        assert_eq!(level.get("xPos"), Some(&Tag::Int(0)));
        assert_eq!(level.get("zPos"), Some(&Tag::Int(0)));
        assert_eq!(
            level.get("Status"),
            Some(&Tag::String(crate::nbt::JavaString::from("minecraft:full")))
        );
        assert_eq!(
            level.get("Biomes").and_then(|tag| match tag {
                Tag::IntArray(values) => Some(values.len()),
                _ => None,
            }),
            Some(256)
        );
        assert_eq!(
            level.get("Sections").and_then(|tag| match tag {
                Tag::List(values) => Some(values.len()),
                _ => None,
            }),
            Some(16)
        );
        let Tag::List(sections) = level.get("Sections").unwrap() else {
            panic!("sections must be a list")
        };
        let Tag::ByteArray(top_sky) = sections[15].get("SkyLight").unwrap() else {
            panic!("sky light must be a byte array")
        };
        assert_eq!(top_sky.len(), 2048);
        assert!(top_sky.iter().all(|value| *value == 0xff));
        let Tag::ByteArray(bottom_sky) = sections[0].get("SkyLight").unwrap() else {
            panic!("sky light must be a byte array")
        };
        assert!(bottom_sky.iter().all(|value| *value == 0));

        let encoded = nbt::write_named(&first, Limits::default()).unwrap();
        let (decoded, used) = nbt::read_named(&encoded, Limits::default()).unwrap();
        assert_eq!(used, encoded.len());
        assert_eq!(decoded, first);
    }

    #[test]
    fn legacy_chunk_adapter_exposes_blocks_to_current_runtime() {
        use crate::chunk::ChunkData;
        use horizon_core::position::{BlockPos, ChunkPos};

        let legacy = generate_bootstrap_chunk(1234, 0, 0);
        let runtime = to_runtime_chunk(&legacy).unwrap();
        assert_eq!(runtime.position(), ChunkPos::new(0, 0));
        assert_eq!(
            runtime.block(BlockPos::new(8, 0, 8)).unwrap().unwrap().name,
            "minecraft:bedrock"
        );
        let height = terrain_height(1234, 8, 8);
        assert_eq!(
            runtime
                .block(BlockPos::new(8, height, 8))
                .unwrap()
                .unwrap()
                .name,
            "minecraft:grass_block"
        );
        let above = if height < SEA_LEVEL {
            "minecraft:water"
        } else {
            "minecraft:air"
        };
        assert_eq!(
            runtime
                .block(BlockPos::new(8, height + 1, 8))
                .unwrap()
                .unwrap()
                .name,
            above
        );

        let mut wrong_version = legacy;
        if let crate::nbt::Tag::Compound(root) = &mut wrong_version.tag {
            root.insert(
                crate::nbt::JavaString::from("DataVersion"),
                crate::nbt::Tag::Int(2585),
            );
        }
        assert!(to_runtime_chunk(&wrong_version).is_err());
        let _: &ChunkData = &runtime;
    }

    #[test]
    fn bootstrap_world_writes_level_dat_and_spawn_region() {
        use crate::{
            compression::Compression,
            nbt::{self, Limits, Tag},
            region::RegionFile,
        };
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "horizon-legacy-world-folder-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let world = base.join("Test World");
        create_bootstrap_world(&world, "Test World", 987654321).unwrap();

        let level_bytes = std::fs::read(world.join("level.dat")).unwrap();
        let uncompressed = Compression::Gzip
            .decode(&level_bytes, Limits::default().max_bytes)
            .unwrap();
        let (level_dat, used) = nbt::read_named(&uncompressed, Limits::default()).unwrap();
        assert_eq!(used, uncompressed.len());
        assert_eq!(level_dat.tag.get("DataVersion"), Some(&Tag::Int(2586)));
        let data = level_dat.tag.get("Data").expect("level.dat Data");
        assert_eq!(
            data.get("LevelName"),
            Some(&Tag::String(crate::nbt::JavaString::from("Test World")))
        );
        assert_eq!(data.get("RandomSeed"), Some(&Tag::Long(987654321)));
        assert!(
            data.get("WorldGenSettings")
                .and_then(|tag| tag.get("dimensions"))
                .is_some()
        );
        assert_eq!(data.get("SpawnX"), Some(&Tag::Int(8)));
        assert_eq!(data.get("SpawnZ"), Some(&Tag::Int(8)));
        assert_eq!(
            std::fs::metadata(world.join("session.lock")).unwrap().len(),
            8
        );

        let region_dir = world.join("region");
        let mut count = 0;
        for rz in -1..=0 {
            for rx in -1..=0 {
                let path = region_dir.join(format!("r.{rx}.{rz}.mca"));
                let region = RegionFile::open(path).unwrap();
                count += region.chunks().count();
            }
        }
        assert_eq!(count, 25);
        let mut spawn_region = RegionFile::open(region_dir.join("r.0.0.mca")).unwrap();
        let spawn_chunk = spawn_region
            .read(horizon_core::position::ChunkPos::new(0, 0))
            .unwrap()
            .unwrap();
        let chunk_nbt = spawn_chunk.nbt(Limits::default()).unwrap();
        assert_eq!(chunk_nbt.tag.get("DataVersion"), Some(&Tag::Int(2586)));
        let runtime = load_runtime_chunk(&world, 0, 0).unwrap();
        assert_eq!(
            runtime
                .block(horizon_core::position::BlockPos::new(8, 0, 8))
                .unwrap()
                .unwrap()
                .name,
            "minecraft:bedrock"
        );

        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn saved_bootstrap_chunk_can_be_reopened_from_anvil_region() {
        use crate::{nbt::Limits, region::RegionFile};
        use horizon_core::position::ChunkPos;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "horizon-legacy-world-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        save_bootstrap_chunk(&directory, 42, -1, 2).unwrap();
        let chunk_pos = ChunkPos::new(-1, 2);
        let path = directory.join(format!(
            "r.{}.{}.mca",
            chunk_pos.region().x,
            chunk_pos.region().z
        ));
        let mut region = RegionFile::open(path).unwrap();
        let record = region.read(chunk_pos).unwrap().expect("saved chunk record");
        let decoded = record.nbt(Limits::default()).unwrap();
        assert_eq!(
            decoded.tag.get("DataVersion"),
            Some(&crate::nbt::Tag::Int(2586))
        );
        assert_eq!(
            decoded.tag.get("Level").and_then(|level| level.get("xPos")),
            Some(&crate::nbt::Tag::Int(-1))
        );
        assert_eq!(
            decoded.tag.get("Level").and_then(|level| level.get("zPos")),
            Some(&crate::nbt::Tag::Int(2))
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_section_palette_round_trips_1165_nbt_schema() {
        use crate::nbt::{JavaString, Tag};

        let mut section = LegacyBlockSection::air();
        let mut properties = std::collections::BTreeMap::new();
        properties.insert("axis".to_owned(), "y".to_owned());
        section
            .set(
                15,
                15,
                15,
                LegacyBlockState {
                    name: "oak_log".to_owned(),
                    properties,
                },
            )
            .unwrap();
        section
            .set(0, 1, 0, LegacyBlockState::new("stone"))
            .unwrap();

        let encoded = section.to_nbt();
        assert_eq!(
            encoded
                .get("Palette")
                .and_then(|tag| match tag {
                    Tag::List(entries) =>
                        entries.first()?.get("Name").and_then(|name| match name {
                            Tag::String(name) => name.to_string().ok(),
                            _ => None,
                        }),
                    _ => None,
                })
                .as_deref(),
            Some("minecraft:air")
        );
        assert!(encoded.get("palette").is_none());
        assert!(encoded.get("BlockStates").is_some());

        let decoded = LegacyBlockSection::from_nbt(&encoded).unwrap();
        assert_eq!(decoded.get(15, 15, 15).unwrap().name, "oak_log");
        assert_eq!(
            decoded
                .get(15, 15, 15)
                .unwrap()
                .properties
                .get("axis")
                .map(String::as_str),
            Some("y")
        );
        assert_eq!(decoded.get(0, 1, 0).unwrap().name, "stone");
        assert_eq!(decoded.get(1, 1, 0).unwrap().name, "air");

        // NBT field casing is version-specific; this catches accidental use of
        // the modern lowercase palette/data format.
        let Tag::Compound(map) = encoded else {
            panic!("section must be a compound")
        };
        assert!(map.contains_key(&JavaString::from("Palette")));
        assert!(map.contains_key(&JavaString::from("BlockStates")));
    }

    #[test]
    fn legacy_uniform_section_omits_block_states_and_checks_bounds() {
        use crate::nbt::{JavaString, Tag};
        let section = LegacyBlockSection::uniform(LegacyBlockState::new("stone"));
        let encoded = section.to_nbt();
        assert!(encoded.get("BlockStates").is_none());
        assert_eq!(
            LegacyBlockSection::from_nbt(&encoded)
                .unwrap()
                .get(0, 0, 0)
                .unwrap()
                .name,
            "stone"
        );
        assert!(section.get(16, 0, 0).is_err());
        assert!(section.get_index(4096).is_err());

        let malformed = Tag::compound([(JavaString::from("Palette"), Tag::List(Vec::new()))]);
        assert!(LegacyBlockSection::from_nbt(&malformed).is_err());
    }

    #[test]
    fn legacy_block_registry_is_complete_and_ordered() {
        let blocks = blocks().expect("embedded 1.16.x block data must parse");
        assert_eq!(blocks.len(), 763);
        assert_eq!(blocks[0].id, 0);
        assert_eq!(blocks[0].name, "air");
        assert_eq!(blocks[0].min_state_id, 0);
        assert_eq!(blocks[0].max_state_id, 0);
        assert!(blocks.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert!(blocks.iter().all(|block| {
            block.min_state_id <= block.default_state && block.default_state <= block.max_state_id
        }));
        assert_eq!(blocks.last().map(|block| block.max_state_id), Some(17111));
    }

    #[test]
    fn legacy_block_state_properties_are_available() {
        let stairs = block_by_name("oak_stairs")
            .expect("embedded data must parse")
            .expect("oak stairs must exist");
        assert_eq!(stairs.min_state_id, 1954);
        assert_eq!(stairs.max_state_id, 2033);
        assert!(
            stairs
                .states
                .iter()
                .any(|property| property.name == "facing")
        );
        assert!(
            stairs
                .states
                .iter()
                .any(|property| property.name == "waterlogged")
        );
    }

    #[test]
    fn legacy_state_ids_resolve_without_using_modern_registry_ids() {
        let (stairs, offset) = block_by_state_id(1954)
            .expect("embedded block data must parse")
            .expect("first oak stairs state must exist");
        assert_eq!(stairs.name, "oak_stairs");
        assert_eq!(offset, 0);

        let (stairs_last, offset) = block_by_state_id(2033)
            .expect("embedded block data must parse")
            .expect("last oak stairs state must exist");
        assert_eq!(stairs_last.name, "oak_stairs");
        assert_eq!(offset, 79);

        assert!(
            block_by_state_id(17_112)
                .expect("embedded block data must parse")
                .is_none()
        );
    }

    #[test]
    fn legacy_dimension_uses_the_1165_build_range() {
        assert_eq!(MIN_BUILD_Y, 0);
        assert_eq!(BUILD_HEIGHT, 256);
        assert_eq!(MAX_BUILD_Y, 255);
        assert_eq!(SECTION_COUNT, 16);
        assert_eq!(SEA_LEVEL, 63);
        assert_eq!(section_index(0), Some(0));
        assert_eq!(section_index(15), Some(0));
        assert_eq!(section_index(16), Some(1));
        assert_eq!(section_index(255), Some(15));
        assert_eq!(section_index(-1), None);
        assert_eq!(section_index(256), None);
        assert_eq!(section_min_y(15), Some(240));
        assert_eq!(section_min_y(16), None);
    }

    #[test]
    fn legacy_biome_registry_contains_1165_dimensions_and_nether() {
        let biomes = biomes().expect("embedded biome data must parse");
        assert_eq!(biomes.len(), 79);
        assert_eq!(biomes[0].id, 0);
        assert_eq!(biomes[0].name, "ocean");
        assert!(biomes.windows(2).all(|pair| pair[0].id < pair[1].id));
        for name in [
            "plains",
            "the_void",
            "soul_sand_valley",
            "crimson_forest",
            "basalt_deltas",
        ] {
            assert!(
                biome_by_name(name).expect("data must parse").is_some(),
                "missing biome {name}"
            );
        }
    }
}
