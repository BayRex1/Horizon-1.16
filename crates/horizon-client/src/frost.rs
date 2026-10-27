//! Frost Walker's `replace_disk` (`data/minecraft/enchantment/frost_walker.json`)
//! as `ReplaceDisk.apply` runs it: the disk's cells in vanilla's order, the
//! predicate on each, and the frosted ice set. The game supplies its level
//! through [`FrostLevel`]; tools/test_frost_walker_parity.py runs this code
//! against the vanilla effect on the same worlds.

use horizon_core::{geometry::Vec3, position::BlockPos};

/// The `trigger_game_event` of the effect: sent for each cell that is set.
pub const TRIGGER: &str = "minecraft:block_place";

/// The effect's `offset` (`[0, -1, 0]`): the disk is centred on the block under the feet.
const OFFSET_Y: i32 = -1;

/// The `radius` of the `location_changed` effect: `clamped` to 0 through 16 of
/// `linear` with base 3 and one more per level above the first.
pub fn radius(level: u32) -> i32 {
    let extra = i32::try_from(level.saturating_sub(1)).unwrap_or(i32::MAX);
    3i32.saturating_add(extra).clamp(0, 16)
}

/// The world as the effect's predicate and `setBlockAndUpdate` see it.
pub trait FrostLevel {
    /// `matching_blocks` water and `matching_fluids` water: a water source
    /// (flowing water is a different fluid).
    fn is_water_source(&self, at: [i32; 3]) -> bool;
    /// `matching_block_tag` `minecraft:air` on the cell above.
    fn is_air(&self, at: [i32; 3]) -> bool;
    /// `unobstructed`: the full block at `at` against the entities that block building.
    fn is_unobstructed(&self, at: [i32; 3]) -> bool;
    /// `setBlockAndUpdate` of the frosted ice at `at`; whether it wrote.
    fn set_frosted_ice(&mut self, at: [i32; 3]) -> bool;
}

/// `ReplaceDisk.apply` for a player standing at `position` with `radius`:
/// returns the cells it set, in order (each one sends the trigger).
pub fn replace_disk(level: &mut impl FrostLevel, position: Vec3, radius: i32) -> Vec<[i32; 3]> {
    let feet = BlockPos::containing(position);
    let center = [feet.x, feet.y + OFFSET_Y, feet.z];
    let mut set = Vec::new();
    for at in disk(center, radius, [position.x, position.y, position.z]) {
        let above = [at[0], at[1] + 1, at[2]];
        if !level.is_air(above) || !level.is_water_source(at) || !level.is_unobstructed(at) {
            continue;
        }
        if level.set_frosted_ice(at) {
            set.push(at);
        }
    }
    set
}

/// `BlockPos.betweenClosed` of the square around `center` (x fastest, then
/// y, then z) keeping the cells whose centre, at the player's height, is
/// closer than `radius`. The disk is one block high.
pub fn disk(center: [i32; 3], radius: i32, position: [f64; 3]) -> Vec<[i32; 3]> {
    let limit = f64::from(radius * radius);
    let mut cells = Vec::new();
    for z in center[2] - radius..=center[2] + radius {
        for x in center[0] - radius..=center[0] + radius {
            let dx = f64::from(x) + 0.5 - position[0];
            let dz = f64::from(z) + 0.5 - position[2];
            if dx * dx + dz * dz < limit {
                cells.push([x, center[1], z]);
            }
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::{disk, radius};

    #[test]
    fn frost_disk_is_radius_three_at_level_one() {
        // Standing on a block centre: the disk is every cell whose centre is
        // closer than 3, which is 25 cells in a 7x7 square.
        let cells = disk([0, 64, 0], 3, [0.5, 65.0, 0.5]);
        assert_eq!(cells.len(), 25);
        assert_eq!(cells.first(), Some(&[-2, 64, -2]));
        assert_eq!(cells.last(), Some(&[2, 64, 2]));
        assert!(!cells.contains(&[3, 64, 0]));
    }

    #[test]
    fn radius_grows_per_level_and_clamps() {
        assert_eq!(radius(1), 3);
        assert_eq!(radius(2), 4);
        assert_eq!(radius(0), 3);
        assert_eq!(radius(u32::MAX), 16);
    }
}
