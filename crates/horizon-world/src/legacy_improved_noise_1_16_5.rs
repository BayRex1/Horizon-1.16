//! Legacy ImprovedNoise primitives used by Minecraft Java 1.16.5.
//!
//! This is deliberately separate from the modern noise implementation in
//! horizon-core: the legacy sampler consumes java.util.Random draws and keeps
//! the 1.16-era permutation table and coordinate offsets.

use crate::legacy_java_random_1_16_5::JavaRandom48;

/// Minecraft 1.16.5's improved Perlin noise sampler.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyImprovedNoise1_16_5 {
    x_offset: f64,
    y_offset: f64,
    z_offset: f64,
    permutation: [u8; 512],
}

impl LegacyImprovedNoise1_16_5 {
    /// Builds a sampler, consuming the same random stream as the legacy
    /// ImprovedNoise constructor: three offsets followed by Fisher-Yates.
    pub fn new(random: &mut JavaRandom48) -> Self {
        let x_offset = random.next_f64() * 256.0;
        let y_offset = random.next_f64() * 256.0;
        let z_offset = random.next_f64() * 256.0;

        let mut values = [0_u8; 256];
        for (index, value) in values.iter_mut().enumerate() {
            *value = index as u8;
        }
        for index in 0..256 {
            let swap = index + random.next_int((256 - index) as i32) as usize;
            values.swap(index, swap);
        }

        let mut permutation = [0_u8; 512];
        for index in 0..512 {
            permutation[index] = values[index & 255];
        }

        Self {
            x_offset,
            y_offset,
            z_offset,
            permutation,
        }
    }

    /// Samples the 3D improved Perlin field at the supplied coordinates.
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let x = x + self.x_offset;
        let y = y + self.y_offset;
        let z = z + self.z_offset;

        let floor_x = x.floor();
        let floor_y = y.floor();
        let floor_z = z.floor();
        let ix = floor_x as i32 & 255;
        let iy = floor_y as i32 & 255;
        let iz = floor_z as i32 & 255;

        let local_x = x - floor_x;
        let local_y = y - floor_y;
        let local_z = z - floor_z;

        let fade_x = fade(local_x);
        let fade_y = fade(local_y);
        let fade_z = fade(local_z);

        let a = self.permutation[ix as usize] as usize + iy as usize;
        let aa = self.permutation[a] as usize + iz as usize;
        let ab = self.permutation[a + 1] as usize + iz as usize;
        let b = self.permutation[ix as usize + 1] as usize + iy as usize;
        let ba = self.permutation[b] as usize + iz as usize;
        let bb = self.permutation[b + 1] as usize + iz as usize;

        let x00 = lerp(
            fade_x,
            grad(self.permutation[aa], local_x, local_y, local_z),
            grad(self.permutation[ba], local_x - 1.0, local_y, local_z),
        );
        let x10 = lerp(
            fade_x,
            grad(self.permutation[ab], local_x, local_y - 1.0, local_z),
            grad(self.permutation[bb], local_x - 1.0, local_y - 1.0, local_z),
        );
        let x01 = lerp(
            fade_x,
            grad(self.permutation[aa + 1], local_x, local_y, local_z - 1.0),
            grad(
                self.permutation[ba + 1],
                local_x - 1.0,
                local_y,
                local_z - 1.0,
            ),
        );
        let x11 = lerp(
            fade_x,
            grad(
                self.permutation[ab + 1],
                local_x,
                local_y - 1.0,
                local_z - 1.0,
            ),
            grad(
                self.permutation[bb + 1],
                local_x - 1.0,
                local_y - 1.0,
                local_z - 1.0,
            ),
        );

        lerp(fade_z, lerp(fade_y, x00, x10), lerp(fade_y, x01, x11))
    }

    /// Returns the three constructor offsets, useful for parity diagnostics.
    pub fn offsets(&self) -> [f64; 3] {
        [self.x_offset, self.y_offset, self.z_offset]
    }

    /// Returns the permutation table's first 256 entries for fixture checks.
    pub fn permutation(&self) -> &[u8; 512] {
        &self.permutation
    }
}

#[inline]
fn fade(value: f64) -> f64 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

#[inline]
fn lerp(amount: f64, start: f64, end: f64) -> f64 {
    start + amount * (end - start)
}

#[inline]
fn grad(hash: u8, x: f64, y: f64, z: f64) -> f64 {
    let hash = hash & 15;
    let u = if hash < 8 { x } else { y };
    let v = if hash < 4 {
        y
    } else if hash == 12 || hash == 14 {
        x
    } else {
        z
    };
    let first = if hash & 1 == 0 { u } else { -u };
    let second = if hash & 2 == 0 { v } else { -v };
    first + second
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_consumes_three_offsets_and_256_shuffle_draws() {
        let mut actual_rng = JavaRandom48::new(0);
        let sampler = LegacyImprovedNoise1_16_5::new(&mut actual_rng);

        let mut expected_rng = JavaRandom48::new(0);
        let expected_offsets = [
            expected_rng.next_f64() * 256.0,
            expected_rng.next_f64() * 256.0,
            expected_rng.next_f64() * 256.0,
        ];
        for index in 0..256 {
            expected_rng.next_int(256 - index);
        }

        assert_eq!(sampler.offsets(), expected_offsets);
        assert_eq!(actual_rng, expected_rng);
    }

    #[test]
    fn permutation_is_a_duplicated_256_entry_permutation() {
        let mut rng = JavaRandom48::new(987654321);
        let sampler = LegacyImprovedNoise1_16_5::new(&mut rng);
        let permutation = sampler.permutation();
        let mut first = permutation[..256].to_vec();
        first.sort_unstable();
        assert_eq!(first, (0_u8..=255).collect::<Vec<_>>());
        assert_eq!(&permutation[..256], &permutation[256..]);
    }

    #[test]
    fn sampling_is_finite_and_deterministic() {
        let mut first_rng = JavaRandom48::new(-123456789);
        let first = LegacyImprovedNoise1_16_5::new(&mut first_rng);
        let mut second_rng = JavaRandom48::new(-123456789);
        let second = LegacyImprovedNoise1_16_5::new(&mut second_rng);

        for point in [[0.0, 0.0, 0.0], [1.25, -9.5, 42.75], [-128.0, 64.0, 0.5]] {
            let a = first.sample(point[0], point[1], point[2]);
            let b = second.sample(point[0], point[1], point[2]);
            assert!(a.is_finite());
            assert_eq!(a, b);
        }
    }
}
