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
        Self { name: name.into(), properties: std::collections::BTreeMap::new() }
    }

    fn to_nbt(&self) -> crate::nbt::Tag {
        use crate::nbt::{JavaString, Tag};
        let mut map = crate::nbt::Compound::new();
        let name = if self.name.contains(':') { self.name.clone() } else { format!("minecraft:{}", self.name) };
        map.insert(JavaString::from("Name"), Tag::String(JavaString::from(name.as_str())));
        if !self.properties.is_empty() {
            map.insert(JavaString::from("Properties"), Tag::Compound(
                self.properties.iter().map(|(key, value)| (
                    JavaString::from(key.as_str()), Tag::String(JavaString::from(value.as_str()))
                )).collect()
            ));
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
        Self { palette: vec![state], storage: None }
    }

    pub fn air() -> Self {
        Self::uniform(LegacyBlockState::new("air"))
    }

    pub fn palette(&self) -> &[LegacyBlockState] { &self.palette }

    pub fn get_index(&self, index: usize) -> std::io::Result<&LegacyBlockState> {
        use crate::compression::invalid;
        if index >= Self::BLOCK_COUNT {
            return Err(invalid("legacy section block index out of bounds"));
        }
        let palette_index = self.storage.as_ref().map_or(0, |storage| storage.read(index) as usize);
        self.palette.get(palette_index).ok_or_else(|| invalid("legacy palette index outside palette"))
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> std::io::Result<&LegacyBlockState> {
        if x >= 16 || y >= 16 || z >= 16 {
            return Err(crate::compression::invalid("legacy section coordinates out of bounds"));
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
                if self.storage.as_ref().is_none_or(|storage| storage.bits() < next_bits) {
                    let mut next = PackedStorage::new(next_bits, Self::BLOCK_COUNT)
                        .map_err(|e| invalid(e.to_string()))?;
                    if let Some(previous) = &self.storage {
                        for i in 0..Self::BLOCK_COUNT {
                            next.set(i, previous.read(i)).map_err(|e| invalid(e.to_string()))?;
                        }
                    }
                    self.storage = Some(next);
                }
                self.palette.push(state);
                self.palette.len() - 1
            }
        };
        if let Some(storage) = &mut self.storage {
            storage.set(index, palette_index as u32).map_err(|e| invalid(e.to_string()))?;
        }
        Ok(())
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, state: LegacyBlockState) -> std::io::Result<()> {
        if x >= 16 || y >= 16 || z >= 16 {
            return Err(crate::compression::invalid("legacy section coordinates out of bounds"));
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
        let palette = entries.iter().map(LegacyBlockState::from_nbt).collect::<std::io::Result<Vec<_>>>()?;
        if palette.len() > Self::BLOCK_COUNT {
            return Err(invalid("legacy section palette exceeds block count"));
        }
        if palette.len() == 1 {
            if let Some(Tag::LongArray(words)) = tag.get("BlockStates") {
                if !words.is_empty() {
                    return Err(invalid("single-entry legacy palette must not contain BlockStates"));
                }
            }
            return Ok(Self { palette, storage: None });
        }
        let bits = ((usize::BITS - (palette.len() - 1).leading_zeros()) as u8).max(4);
        let Some(Tag::LongArray(words)) = tag.get("BlockStates") else {
            return Err(invalid("nonuniform legacy palette is missing BlockStates"));
        };
        let storage = horizon_core::storage::PackedStorage::from_words(
            bits, Self::BLOCK_COUNT, words.iter().map(|word| *word as u64).collect()
        ).map_err(|e| invalid(e.to_string()))?;
        for index in 0..Self::BLOCK_COUNT {
            if storage.read(index) as usize >= palette.len() {
                return Err(invalid("legacy palette index outside palette"));
            }
        }
        Ok(Self { palette, storage: Some(storage) })
    }

    /// Encode the section fields to embed into a 1.16.5 section compound.
    pub fn to_nbt(&self) -> crate::nbt::Tag {
        use crate::nbt::{JavaString, Tag};
        let mut map = crate::nbt::Compound::new();
        map.insert(JavaString::from("Palette"), Tag::List(self.palette.iter().map(LegacyBlockState::to_nbt).collect()));
        if let Some(storage) = &self.storage {
            map.insert(JavaString::from("BlockStates"), Tag::LongArray(storage.words().iter().map(|word| *word as i64).collect()));
        }
        Tag::Compound(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_section_palette_round_trips_1165_nbt_schema() {
        use crate::nbt::{JavaString, Tag};

        let mut section = LegacyBlockSection::air();
        let mut properties = std::collections::BTreeMap::new();
        properties.insert("axis".to_owned(), "y".to_owned());
        section.set(15, 15, 15, LegacyBlockState {
            name: "oak_log".to_owned(),
            properties,
        }).unwrap();
        section.set(0, 1, 0, LegacyBlockState::new("stone")).unwrap();

        let encoded = section.to_nbt();
        assert_eq!(encoded.get("Palette").and_then(|tag| match tag {
            Tag::List(entries) => entries.first()?.get("Name").and_then(|name| match name {
                Tag::String(name) => name.to_string().ok(),
                _ => None,
            }),
            _ => None,
        }).as_deref(), Some("minecraft:air"));
        assert!(encoded.get("palette").is_none());
        assert!(encoded.get("BlockStates").is_some());

        let decoded = LegacyBlockSection::from_nbt(&encoded).unwrap();
        assert_eq!(decoded.get(15, 15, 15).unwrap().name, "oak_log");
        assert_eq!(decoded.get(15, 15, 15).unwrap().properties.get("axis").map(String::as_str), Some("y"));
        assert_eq!(decoded.get(0, 1, 0).unwrap().name, "stone");
        assert_eq!(decoded.get(1, 1, 0).unwrap().name, "air");

        // NBT field casing is version-specific; this catches accidental use of
        // the modern lowercase palette/data format.
        let Tag::Compound(map) = encoded else { panic!("section must be a compound") };
        assert!(map.contains_key(&JavaString::from("Palette")));
        assert!(map.contains_key(&JavaString::from("BlockStates")));
    }

    #[test]
    fn legacy_uniform_section_omits_block_states_and_checks_bounds() {
        use crate::nbt::{JavaString, Tag};
        let section = LegacyBlockSection::uniform(LegacyBlockState::new("stone"));
        let encoded = section.to_nbt();
        assert!(encoded.get("BlockStates").is_none());
        assert_eq!(LegacyBlockSection::from_nbt(&encoded).unwrap().get(0, 0, 0).unwrap().name, "stone");
        assert!(section.get(16, 0, 0).is_err());
        assert!(section.get_index(4096).is_err());

        let malformed = Tag::compound([(
            JavaString::from("Palette"),
            Tag::List(Vec::new()),
        )]);
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
