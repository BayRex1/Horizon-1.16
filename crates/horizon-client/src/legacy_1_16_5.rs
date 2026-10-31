//! Client-world bridge for decoded Minecraft 1.16.5 chunk columns.
//!
//! Protocol palette IDs are version-local. Callers must resolve each legacy
//! ID through the 1.16.5 registry before it is written into the active
//! renderer's resident world; treating a 1.16.5 numeric ID as a modern ID is
//! deliberately forbidden.
use crate::{chunk::Chunk, world::World};
use horizon_protocol::legacy_1_16_5::LegacyChunkColumn;
use horizon_world::registry::BlockStateId;
use std::sync::Arc;

/// Apply one decoded protocol-754 chunk to the resident client world.
///
/// resolve_state translates a 1.16.5 registry state ID into the active
/// renderer registry. Return None for unknown states; the whole update then
/// fails instead of rendering incorrect blocks. The caller supplies mapped
/// air. The 1.16.5 build has 16 sections at Y=0..255, while resident storage
/// can represent these coordinates without shifting them.
///
/// Biome IDs, heightmaps, block-entity NBT and light arrays remain available
/// on column; dedicated version-aware adapters must apply those separately.
pub fn apply_chunk(
    world: &mut World,
    column: &LegacyChunkColumn,
    air: BlockStateId,
    mut resolve_state: impl FnMut(i32) -> Option<BlockStateId>,
) -> crate::Result<()> {
    let key = (column.x, column.z);
    if column.full_chunk || !world.chunks.contains_key(&key) {
        world.chunks.insert(key, Arc::new(Chunk::empty(column.x, column.z)));
    }
    world.air = air;
    for (section_y, section) in column.sections.iter().enumerate() {
        let Some(section) = section else { continue };
        if section_y >= 16 {
            return Err("1.16.5 chunk contains a section outside build height".into());
        }
        let mut resolved = Vec::with_capacity(4096);
        for index in 0..4096 {
            let legacy_id = section.block_state(index)
                .ok_or("invalid 1.16.5 block-state palette index")?;
            let state = resolve_state(legacy_id)
                .ok_or_else(|| format!("unmapped 1.16.5 block-state ID {legacy_id}"))?;
            resolved.push(state);
        }
        for (index, state) in resolved.into_iter().enumerate() {
            let x = column.x * 16 + (index & 15) as i32;
            let z = column.z * 16 + ((index >> 4) & 15) as i32;
            let y = section_y as i32 * 16 + (index >> 8) as i32;
            world.set(x, y, z, state)?;
        }
    }
    if !column.block_entities.is_empty() {
        world.loaded_entities.push((key, column.block_entities.clone()));
    }
    world.dirty_light.insert(key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_protocol::legacy_1_16_5::LegacyChunkSection;

    #[test]
    fn applies_legacy_section_blocks_without_y_offset() {
        let mut world = World::new(BlockStateId(0), 0);
        let section = LegacyChunkSection { non_air_blocks: 1, bits_per_block: 0, palette: vec![1], data: Vec::new() };
        let column = LegacyChunkColumn {
            x: 2, z: -1, full_chunk: true, section_mask: 1,
            heightmaps: horizon_world::nbt::Tag::End, biomes: Vec::new(),
            sections: vec![Some(section)], block_entities: Vec::new(),
        };
        apply_chunk(&mut world, &column, BlockStateId(0), |id| Some(BlockStateId(id as u32))).unwrap();
        assert_eq!(world.block(32, 0, -16), BlockStateId(1));
        assert_eq!(world.block(32, 15, -16), BlockStateId(1));
        assert_eq!(world.block(32, 16, -16), BlockStateId(0));
        assert!(world.dirty_light.contains(&(2, -1)));
    }

    #[test]
    fn rejects_unmapped_legacy_registry_states() {
        let mut world = World::new(BlockStateId(0), 0);
        let section = LegacyChunkSection { non_air_blocks: 1, bits_per_block: 0, palette: vec![999], data: Vec::new() };
        let column = LegacyChunkColumn {
            x: 0, z: 0, full_chunk: true, section_mask: 1,
            heightmaps: horizon_world::nbt::Tag::End, biomes: Vec::new(),
            sections: vec![Some(section)], block_entities: Vec::new(),
        };
        assert!(apply_chunk(&mut world, &column, BlockStateId(0), |_| None).is_err());
    }
}