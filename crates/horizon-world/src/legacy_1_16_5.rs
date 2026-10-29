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
pub fn block_by_state_id(
    state_id: u32,
) -> Result<Option<(LegacyBlock, u32)>, serde_json::Error> {
    Ok(blocks()?.into_iter().find_map(|block| {
        (block.min_state_id <= state_id && state_id <= block.max_state_id)
            .then_some((block, state_id - block.min_state_id))
    }))
}

/// Resolve a biome's protocol-era numeric registry ID.
pub fn biome_by_id(id: u32) -> Result<Option<LegacyBiome>, serde_json::Error> {
    Ok(biomes()?.into_iter().find(|biome| biome.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(block_by_state_id(17_112)
            .expect("embedded block data must parse")
            .is_none());
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
