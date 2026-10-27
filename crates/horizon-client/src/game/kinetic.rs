//! `KineticWeapon`: a spear's right-click charge, 26.3's `Item.use` kinetic
//! branch and `ItemStack.onUseTick` -> `KineticWeapon.damageEntities`.
//!
//! Holding use starts the charge. From `delay_ticks` on, each tick sweeps
//! `PiercingWeapon`'s line (`ProjectileUtil.getHitEntitiesAlong` with the
//! collider clip, so grass and other non-colliding blocks do not stop it) and
//! hits each reached mob once per `contact_cooldown_ticks`. A hit's damage,
//! knockback and dismount come from the weapon's conditions against the
//! attacker's speed along the look line and the closing speed, which is what
//! divides a charge into its stages.
use super::{Game, view_vector};
use horizon_client::collisions::SceneCollisions;

use horizon_client::items::{AttackRange, ItemInfo, KineticWeapon};
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;

/// `KineticWeapon.getMotion`: speed in blocks a second.
const MOTION_SCALE: f64 = 20.0;
/// `KineticWeapon.damageEntities` charges at a fifth of a player's thresholds.
const PLAYER_FACTOR: f64 = 1.0;
/// `KineticWeapon.HIT_FEEDBACK_TICKS`: how long the hurt pose holds after a
/// charge that only knocked the target back.
const HIT_FEEDBACK_TICKS: i32 = 10;
/// `LivingEntity.stabAttack`'s first knockback: the flat shove before the
/// attacker's knockback attribute.
const STAB_KNOCKBACK: f32 = 0.4;

impl Game<'_> {
    /// `Item.use`'s kinetic branch: the spear is held like a shield until
    /// released, and the charge sound starts.
    pub(super) fn start_kinetic_use(&mut self, info: &ItemInfo) -> bool {
        let Some(kinetic) = &info.kinetic_weapon else {
            return false;
        };
        if self.using.is_some() {
            return false;
        }
        // `resetRecentKineticEnemies`: a new charge may reach a mob the last
        // one already hit.
        self.kinetic_recent.clear();
        self.kinetic_trail.clear();
        self.using = Some((self.held_slot(), 0));
        if let Some(sound) = &kinetic.sound {
            let at = self.player_feet();
            self.audio.play(sound, at, 1.0, 1.0);
        }
        true
    }
    /// `KineticWeapon.damageEntities` for this tick of the charge.
    pub(super) fn tick_kinetic_weapon(&mut self, ticks_used: u32, kinetic: &KineticWeapon) {
        if ticks_used < kinetic.delay_ticks {
            return;
        }
        let ticks = ticks_used - kinetic.delay_ticks;
        let Some(eye) = self.eye_position() else {
            return;
        };
        let look = view_vector(self.yaw, self.pitch);
        let projection =
            |m: [f64; 3]| (m[0] * look[0] + m[1] * look[1] + m[2] * look[2]) * MOTION_SCALE;
        // `Entity.getKnownMovement`: the server copy of the window's player
        // takes its movement from batched position packets, so one tick's step
        // understates it; the charge's own recent eye positions stand in.
        let time = self.world.game_time;
        self.kinetic_trail.push((eye, time));
        if self.kinetic_trail.len() > 6 {
            self.kinetic_trail.remove(0);
        }
        let travelled =
            self.kinetic_trail
                .first()
                .filter(|(_, at)| *at < time)
                .map(|(oldest, at)| {
                    let ticks = time - at;
                    std::array::from_fn::<f64, 3, _>(|i| (eye[i] - oldest[i]) / ticks as f64)
                });
        let velocity = self
            .player
            .as_ref()
            .map(|p| [p.velocity.x, p.velocity.y, p.velocity.z]);
        // `getKnownMovement` is the per-tick step; the conditions read it at
        // `getMotion`'s twenty ticks a second.
        let step = velocity
            .map_or(0.0, projection)
            .max(travelled.map_or(0.0, projection))
            / MOTION_SCALE;
        let attacker_speed = step * MOTION_SCALE;
        let movement = step;
        let range = self.charge_range();
        let (min, max) = range.effective(self.creative);
        let from: [f64; 3] = std::array::from_fn(|i| eye[i] + look[i] * f64::from(min));
        let to: [f64; 3] =
            std::array::from_fn(|i| eye[i] + look[i] * (f64::from(max) + movement.max(0.0)));
        // `getHitEntitiesAlong` clips from the eye: a wall before the spear tip
        // reaches nothing at all.
        let end = self.clip_collider(eye, to).unwrap_or(to);
        let distance = |p: [f64; 3]| {
            ((p[0] - eye[0]).powi(2) + (p[1] - eye[1]).powi(2) + (p[2] - eye[2]).powi(2)).sqrt()
        };
        if distance(end) < distance(from) {
            return;
        }
        let margin = f64::from(range.hitbox_margin);
        let (from_v, end_v) = (
            Vec3::new(from[0], from[1], from[2]),
            Vec3::new(end[0], end[1], end[2]),
        );
        let base_damage = self.base_attack_damage();
        for index in 0..self.mobs.len() {
            let mob = &self.mobs[index];
            // `PiercingWeapon.canHitEntity`.
            if !mob.alive() || mob.removed || mob.death_time > 0 {
                continue;
            }
            let box_ = mob.bounds().inflate(Vec3::new(margin, margin, margin));
            if box_.clip(from_v, end_v).is_none() {
                continue;
            }
            let id = mob.id;
            let cooldown = i64::from(kinetic.contact_cooldown_ticks);
            if let Some(&at) = self.kinetic_recent.get(&id)
                && time - at < cooldown
            {
                continue;
            }
            let target_motion = [mob.velocity.x, mob.velocity.y, mob.velocity.z];
            let target_speed = projection(target_motion);
            // The closing speed `KineticWeapon` tests and scales damage by.
            let relative = (attacker_speed - target_speed).max(0.0);
            let test = |condition: Option<horizon_client::items::KineticCondition>| {
                condition.is_some_and(|c| c.test(ticks, attacker_speed, relative, PLAYER_FACTOR))
            };
            let dismounts = test(kinetic.dismount);
            let knockback = test(kinetic.knockback);
            let deals_damage = test(kinetic.damage);
            if !(dismounts || knockback || deals_damage) {
                continue;
            }
            self.kinetic_recent.insert(id, time);
            // `Mth.floor(relativeSpeed * damageMultiplier)` over the hand's
            // base attack damage.
            let amount =
                base_damage + (relative * f64::from(kinetic.damage_multiplier)).floor() as f32;
            let kind = self.mobs[index].kind.id.clone();
            if deals_damage {
                let source = match self.player.as_ref() {
                    Some(p) => self.attack_source([p.position.x, p.position.y, p.position.z]),
                    None => return,
                };
                let _ = self.hurt_by_player(index, amount, source);
            }
            if knockback {
                // `stabAttack`: a flat shove, then the attacker's knockback.
                let (enchanted, _) = self.extra_knockback(&kind, 0.0);
                let (xd, zd) = self.extra_knockback_direction();
                if STAB_KNOCKBACK > 0.0 {
                    self.mobs[index].knockback(f64::from(STAB_KNOCKBACK), xd, zd);
                }
                if enchanted > 0.0 {
                    self.mobs[index].knockback(f64::from(enchanted), xd, zd);
                }
                // `causeExtraKnockback` slows the attacker down.
                if let Some(p) = self.player.as_mut() {
                    p.velocity.x *= 0.6;
                    p.velocity.z *= 0.6;
                }
            }
            if dismounts && self.mobs[index].extra.passenger {
                // `stopRiding`, but for what `cannot_be_dismounted_by_item_usage`
                // keeps on (an interaction entity).
                let immune = self.mobs[index].kind.id.contains("interaction");
                if !immune {
                    self.dismount_mob(id);
                }
            }
            // `broadcastEntityEvent(this, 2)`: the hurt pose, also for a hit
            // that only knocked the target back.
            self.mobs[index].hurt_time = HIT_FEEDBACK_TICKS;
        }
    }
    /// `PiercingWeapon.attack`: the spear's left-click jab sweeps the look
    /// line and stabs every mob along it, then `postPiercingAttack` runs the
    /// weapon's enchantments (Lunge).
    pub(super) fn piercing_attack(&mut self) {
        let Some(info) = self
            .held()
            .and_then(|stack| self.res.items().get(&stack.item))
            .cloned()
        else {
            return;
        };
        let Some(weapon) = info.piercing_weapon.clone() else {
            return;
        };
        // `getAttributeValue(ATTACK_DAMAGE)`: the hand plus the weapon's
        // modifiers (a jab deals the item's attack damage, unlike a charge).
        let damage = 1.0 + info.attack_damage;
        let Some(eye) = self.eye_position() else {
            return;
        };
        let look = view_vector(self.yaw, self.pitch);
        let range = self.charge_range();
        let (min, max) = range.effective(self.creative);
        let step = self.player.as_ref().map_or(0.0, |p| {
            (p.velocity.x * look[0] + p.velocity.y * look[1] + p.velocity.z * look[2]).max(0.0)
        });
        let from: [f64; 3] = std::array::from_fn(|i| eye[i] + look[i] * f64::from(min));
        let to: [f64; 3] = std::array::from_fn(|i| eye[i] + look[i] * (f64::from(max) + step));
        let end = self.clip_collider(eye, to).unwrap_or(to);
        let distance = |p: [f64; 3]| {
            ((p[0] - eye[0]).powi(2) + (p[1] - eye[1]).powi(2) + (p[2] - eye[2]).powi(2)).sqrt()
        };
        if distance(end) < distance(from) {
            return;
        }
        let margin = f64::from(range.hitbox_margin);
        let (from_v, end_v) = (
            Vec3::new(from[0], from[1], from[2]),
            Vec3::new(end[0], end[1], end[2]),
        );
        let mut hit_something = false;
        for index in 0..self.mobs.len() {
            let mob = &self.mobs[index];
            // `PiercingWeapon.canHitEntity`.
            if !mob.alive() || mob.removed || mob.death_time > 0 {
                continue;
            }
            if mob
                .bounds()
                .inflate(Vec3::new(margin, margin, margin))
                .clip(from_v, end_v)
                .is_none()
            {
                continue;
            }
            let kind = mob.kind.id.clone();
            hit_something = true;
            let source = match self.player.as_ref() {
                Some(p) => self.attack_source([p.position.x, p.position.y, p.position.z]),
                None => return,
            };
            // `stabAttack`: damage first, then the two shoves.
            let _ = self.hurt_by_player(index, damage, source);
            if weapon.deals_knockback {
                let (enchanted, _) = self.extra_knockback(&kind, 0.0);
                let (xd, zd) = self.extra_knockback_direction();
                self.mobs[index].knockback(f64::from(STAB_KNOCKBACK), xd, zd);
                if enchanted > 0.0 {
                    self.mobs[index].knockback(f64::from(enchanted), xd, zd);
                }
            }
            if weapon.dismounts && self.mobs[index].extra.passenger {
                let id = self.mobs[index].id;
                self.dismount_mob(id);
            }
            self.mobs[index].hurt_time = HIT_FEEDBACK_TICKS;
        }
        // `postPiercingAttack`.
        self.post_piercing_attack();
        if hit_something && let Some(sound) = &weapon.hit_sound {
            let at = self.player_feet();
            self.audio.play(sound, at, 1.0, 1.0);
        }
        // `swingAndResetAttackStrength` is the caller's: `first_person::attack`
        // swings with `attack_animation` and resets the strength once the hit
        // lands. Swinging here too starts two swings in one tick, one of them
        // the other animation, and the two poses overlay.
    }
    /// `LivingEntity.postPiercingAttack`: the held weapon's
    /// `post_piercing_attack` effects. 26.3's Lunge moves the attacker ahead,
    /// costs hunger and durability, and sounds.
    fn post_piercing_attack(&mut self) {
        let Some(stack) = self.held().cloned() else {
            return;
        };
        let level = stack.enchantment_level("minecraft:lunge");
        if level == 0 {
            return;
        }
        let level = i32::try_from(level).unwrap_or(i32::MAX);
        // Lunge's requirements: not riding, not gliding, not in water, and
        // not a hungry survival player.
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let in_water = self
            .res
            .facts()
            .get(self.world.block(
                p.position.x.floor() as i32,
                p.position.y.floor() as i32,
                p.position.z.floor() as i32,
            ))
            .fluid
            == horizon_client::blocks::Fluid::Water;
        if self.riding.is_some() || p.fall_flying || in_water {
            return;
        }
        if !self.creative && p.health.food.food < 7 {
            return;
        }
        let (yaw, pitch) = (self.yaw, self.pitch);
        let look = view_vector(yaw, pitch);
        // `apply_impulse`: the local +Z axis, horizontally, `0.458` a level.
        let magnitude = 0.458 * f64::from(level);
        if let Some(p) = self.player.as_mut() {
            p.velocity.x += look[0] * magnitude;
            p.velocity.z += look[2] * magnitude;
            // `apply_exhaustion`: 4 a level.
            p.cause_food_exhaustion(4.0 * level as f32);
        }
        // `change_item_damage`: one durability point.
        self.damage_held(1);
        // `play_sound`: one of the three lunge sounds.
        // `play_sound` with a list picks one: `RandomSource.nextInt`.
        let pick = (RandomSource::next_float(&mut self.world.random) * 3.0) as usize;
        let sound = [
            "minecraft:item.spear.lunge_1",
            "minecraft:item.spear.lunge_2",
            "minecraft:item.spear.lunge_3",
        ][pick.min(2)];
        let at = self.player_feet();
        self.audio.play(sound, at, 1.0, 1.0);
    }
    fn player_feet(&self) -> Option<[f64; 3]> {
        self.player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z])
    }
    /// The `attack_range` component of the held item, or a hand's reach.
    fn charge_range(&self) -> AttackRange {
        self.held()
            .and_then(|stack| self.res.items().get(&stack.item))
            .and_then(|info| info.attack_range)
            .unwrap_or(AttackRange {
                min_reach: 0.0,
                max_reach: self.reach() as f32,
                min_creative_reach: 0.0,
                max_creative_reach: self.reach() as f32,
                hitbox_margin: 0.0,
            })
    }
    /// `Entity.getAttributeBaseValue(ATTACK_DAMAGE)`: the hand's damage, before
    /// the weapon's modifiers (the charge's damage comes from the closing
    /// speed).
    fn base_attack_damage(&self) -> f32 {
        1.0
    }
    /// `Level.clip` with `ClipContext.Block.COLLIDER`: the first block with a
    /// collision shape along the segment.
    fn clip_collider(&self, from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]> {
        use horizon_core::position::BlockPos;
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let (from, to) = (
            Vec3::new(from[0], from[1], from[2]),
            Vec3::new(to[0], to[1], to[2]),
        );
        let hit = horizon_core::geometry::traverse_blocks(from, to, 256, |p: BlockPos| {
            let state = self.world.block(p.x, p.y, p.z);
            if self.res.facts().is_air(state) {
                return None;
            }
            let shape = self
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?;
            shape.clip(from, to, p)
        });
        let hit = hit.ok().flatten()?;
        Some([hit.location.x, hit.location.y, hit.location.z])
    }
}
