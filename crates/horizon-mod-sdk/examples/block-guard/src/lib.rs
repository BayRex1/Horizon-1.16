//! Example Horizon server mod for `block-events` (`docs/mods.md`): the
//! server asks it before it acts on a block. Nobody breaks a diamond block,
//! and a right click with a stick tells the player what the block is
//! instead of using it (a door stays shut, a chest closed).
use horizon_mod_sdk::server::{BlockPos, Player, Registration, ServerMod, players};

struct BlockGuard;

/// The block nobody may break.
const GUARDED: &str = "minecraft:diamond_block";

impl ServerMod for BlockGuard {
    fn init() -> Result<(Self, Registration), String> {
        Ok((BlockGuard, Registration::new().on_blocks()))
    }

    fn break_block(&mut self, player: &Player, _pos: BlockPos, state: &str) -> bool {
        let guarded = state == GUARDED;
        if guarded {
            players::tell(&player.id, "This block is guarded");
        }
        guarded
    }

    fn use_block(&mut self, player: &Player, pos: BlockPos, state: &str, item: &str) -> bool {
        if item != "minecraft:stick" {
            return false;
        }
        players::tell(&player.id, &format!("{} {} {}: {state}", pos.x, pos.y, pos.z));
        true
    }
}
horizon_mod_sdk::export_server_mod!(BlockGuard);
