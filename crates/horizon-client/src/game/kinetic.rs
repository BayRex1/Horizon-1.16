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
        let movement = velocity
            .map_or(0.0, projection)
            .max(travelled.map_or(0.0, projection));
        let attacker_speed = movement;
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
