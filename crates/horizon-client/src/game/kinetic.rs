//! branch and `ItemStack.onUseTick` -> `KineticWeapon.damageEntities`.
//!
//! Holding use starts the charge, and every tick from `delay_ticks` on the
//! spear reaches along the look line (`PiercingWeapon`'s `getHitEntitiesAlong`
//! with the collider clip, so grass and other non-colliding blocks do not
//! stop it). Each reached mob is hit once per `contact_cooldown_ticks`, its
//! damage/knockback/dismount decided by the weapon's conditions against the
//! attacker's speed along the look line and the closing speed.
use super::{Game, view_vector};
use horizon_client::collisions::SceneCollisions;
use horizon_client::items::{AttackRange, ItemInfo, KineticWeapon};
use horizon_core::geometry::Vec3;

/// `LivingEntity.MOTION_SCALE`: `getMotion` reports speed in blocks a second.
const MOTION_SCALE: f64 = 20.0;
/// `Player`'s share of a condition's thresholds (`LivingEntity` charges at a
/// fifth of a player's).
const PLAYER_FACTOR: f64 = 1.0;

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
        let motion = |velocity: Vec3| [velocity.x, velocity.y, velocity.z];
        let projection =
            |m: [f64; 3]| (m[0] * look[0] + m[1] * look[1] + m[2] * look[2]) * MOTION_SCALE;
        // `Entity.getKnownMovement`: over the charge's last few ticks, since
        // the server copy of the window's player takes its movement from
        // batched position packets.
        let time = self.world.game_time;
        self.kinetic_trail.push((eye, time));
        if self.kinetic_trail.len() > 6 {
            self.kinetic_trail.remove(0);
        }
        let eye_step =
            self.kinetic_trail
                .first()
                .filter(|(_, at)| *at < time)
                .map(|(oldest, at)| {
                    let ticks = time - at;
                    let step: [f64; 3] =
                        std::array::from_fn(|i| (eye[i] - oldest[i]) / ticks as f64);
                    projection(step)
                });
        let attacker_speed = self
            .player
            .as_ref()
            .map_or(0.0, |p| projection(motion(p.velocity)))
            .max(eye_step.unwrap_or(0.0));
        let range = self.charge_range();
        let (min, max) = range.effective(self.creative);
        let movement = self
            .player
            .as_ref()
            .map_or(0.0, |p| projection(motion(p.velocity)));
        let (min, max) = if min <= 0.0 && max <= 0.0 {
            (2.0, 4.5)
        } else {
            (min, max)
        };
        let from: [f64; 3] = std::array::from_fn(|i| eye[i] + look[i] * f64::from(min));
        let to: [f64; 3] =
            std::array::from_fn(|i| eye[i] + look[i] * (f64::from(max) + movement.max(0.0)));
        let end = self.clip_collider(from, to).unwrap_or(to);
        let margin = f64::from(range.hitbox_margin);
        let damage = self.base_attack_damage();
        let mut affected = false;
        for index in 0..self.mobs.len() {
            let mob = &self.mobs[index];
            if !mob.alive() || mob.death_time > 0 {
                continue;
            }
            let box_ = mob.bounds().inflate(Vec3::new(margin, margin, margin));
            let (from_v, end_v) = (
                Vec3::new(from[0], from[1], from[2]),
                Vec3::new(end[0], end[1], end[2]),
            );
            if box_.clip(from_v, end_v).is_none() {
                continue;
            }
            let id = mob.id;
            let time = self.world.game_time;
            let cooldown = i64::from(kinetic.contact_cooldown_ticks);
            if let Some(&at) = self.kinetic_recent.get(&id)
                && time - at < cooldown
            {
                continue;
            }
            let target_speed = projection(motion(mob.velocity));
            // `KineticWeapon.damageEntities`: how fast the attacker closes in.
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
            let amount = damage + (relative * f64::from(kinetic.damage_multiplier)).floor() as f32;
            let kind = self.mobs[index].kind.id.clone();
            let source = match self.player.as_ref() {
                Some(p) => self.attack_source([p.position.x, p.position.y, p.position.z]),
                None => return,
            };
            let landed = deals_damage && self.hurt_by_player(index, amount, source);
            if knockback || landed {
                let (power, _) = self.extra_knockback(&kind, 1.0);
                if power > 0.0 {
                    let (xd, zd) = self.extra_knockback_direction();
                    self.mobs[index].knockback(f64::from(power), xd, zd);
                }
            }
            affected |= landed || knockback || dismounts;
        }
        if affected && let Some(sound) = &kinetic.hit_sound {
            let at = self.player_feet();
            self.audio.play(sound, at, 1.0, 1.0);
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
