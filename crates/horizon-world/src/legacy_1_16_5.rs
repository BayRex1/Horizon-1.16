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
            block.min_state_id <= block.default_state
                && block.default_state <= block.max_state_id
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
        assert!(stairs.states.iter().any(|property| property.name == "facing"));
        assert!(stairs.states.iter().any(|property| property.name == "waterlogged"));
    }

    #[test]
    fn legacy_biome_registry_contains_1165_dimensions_and_nether() {
        let biomes = biomes().expect("embedded biome data must parse");
        assert_eq!(biomes.len(), 79);
        assert_eq!(biomes[0].id, 0);
        assert_eq!(biomes[0].name, "ocean");
        assert!(biomes.windows(2).all(|pair| pair[0].id < pair[1].id));
        for name in ["plains", "the_void", "soul_sand_valley", "crimson_forest", "basalt_deltas"] {
            assert!(biome_by_name(name).expect("data must parse").is_some(), "missing biome {name}");
        }
    }
}
