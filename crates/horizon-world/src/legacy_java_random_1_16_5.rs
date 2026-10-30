//! Java's 48-bit java.util.Random compatibility for Minecraft 1.16.5 world generation.
//!
//! Nether noise samplers use Java's legacy LCG rather than Rust's RNGs. Keeping
//! this implementation isolated makes the exact random stream testable before
//! it is wired into Perlin/DoublePerlin sampler construction.

const MULTIPLIER: u64 = 0x5DEECE66D;
const ADDEND: u64 = 0xB;
const MASK: u64 = (1_u64 << 48) - 1;

/// Reproduces the state transition and primitive draws of java.util.Random.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaRandom48 {
    seed: u64,
}

impl JavaRandom48 {
    /// Creates a generator using Java's setSeed scrambling rule.
    pub fn new(seed: i64) -> Self {
        let mut random = Self { seed: 0 };
        random.set_seed(seed);
        random
    }

    /// Resets the generator exactly like java.util.Random#setSeed.
    pub fn set_seed(&mut self, seed: i64) {
        self.seed = ((seed as u64) ^ MULTIPLIER) & MASK;
    }

    /// Advances the internal 48-bit LCG and returns the requested high bits.
    pub fn next_bits(&mut self, bits: u32) -> u32 {
        assert!((1..=32).contains(&bits), "bits must be in 1..=32");
        self.seed = self.seed.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        (self.seed >> (48 - bits)) as u32
    }

    /// Returns Java-compatible nextBoolean().
    pub fn next_bool(&mut self) -> bool {
        self.next_bits(1) != 0
    }

    /// Returns Java-compatible nextInt().
    pub fn next_i32(&mut self) -> i32 {
        self.next_bits(32) as i32
    }

    /// Returns Java-compatible nextLong(), including signed int extension.
    pub fn next_i64(&mut self) -> i64 {
        ((self.next_i32() as i64) << 32).wrapping_add(self.next_i32() as i64)
    }

    /// Returns Java-compatible nextInt(bound).
    pub fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if bound & -bound == bound {
            return (((bound as i64) * (self.next_bits(31) as i64)) >> 31) as i32;
        }
        loop {
            let bits = self.next_bits(31) as i32;
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    /// Returns Java-compatible nextFloat() using the high 24 random bits.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_bits(24) as f32) * (1.0 / ((1_u32 << 24) as f32))
    }

    /// Returns Java-compatible nextDouble() using two consecutive draws.
    pub fn next_f64(&mut self) -> f64 {
        let high = (self.next_bits(26) as u64) << 27;
        let low = self.next_bits(27) as u64;
        ((high + low) as f64) * (1.0 / ((1_u64 << 53) as f64))
    }

    /// Skips exactly draws 48-bit LCG transitions.
    pub fn advance(&mut self, draws: u64) {
        // Affine exponentiation computes (a^n, c * (1 + a + ... + a^(n-1)))
        // modulo 2^48, avoiding a loop for the large skip counts used by noise.
        let mut multiplier = MULTIPLIER;
        let mut addend = ADDEND;
        let mut accumulated_multiplier = 1_u64;
        let mut accumulated_addend = 0_u64;
        let mut remaining = draws;
        while remaining != 0 {
            if remaining & 1 != 0 {
                accumulated_multiplier = accumulated_multiplier.wrapping_mul(multiplier) & MASK;
                accumulated_addend = accumulated_addend
                    .wrapping_mul(multiplier)
                    .wrapping_add(addend)
                    & MASK;
            }
            addend = addend.wrapping_mul(multiplier.wrapping_add(1)) & MASK;
            multiplier = multiplier.wrapping_mul(multiplier) & MASK;
            remaining >>= 1;
        }
        self.seed = accumulated_multiplier
            .wrapping_mul(self.seed)
            .wrapping_add(accumulated_addend)
            & MASK;
    }
}

#[cfg(test)]
mod tests {
    use super::JavaRandom48;

    #[test]
    fn matches_java_random_seed_zero_sequence() {
        let mut random = JavaRandom48::new(0);
        assert_eq!(random.next_i32(), -1_155_484_576);
        assert_eq!(random.next_i32(), -723_955_400);
        assert_eq!(random.next_i32(), 1_033_096_058);
    }

    #[test]
    fn matches_java_next_long_seed_zero() {
        let mut random = JavaRandom48::new(0);
        assert_eq!(random.next_i64(), -4_962_768_465_676_381_896);
    }

    #[test]
    fn matches_java_boolean_and_float_draws() {
        let mut boolean_rng = JavaRandom48::new(0);
        assert!(boolean_rng.next_bool());

        let mut float_rng = JavaRandom48::new(0);
        assert!((float_rng.next_f32() - 0.73096776).abs() < f32::EPSILON);
    }

    #[test]
    fn matches_java_bounded_integer_and_double() {
        let mut random = JavaRandom48::new(0);
        assert_eq!(random.next_int(100), 60);
        let mut random = JavaRandom48::new(0);
        assert!((random.next_f64() - 0.730967787376657).abs() < f64::EPSILON);
    }

    #[test]
    fn logarithmic_advance_matches_repeated_draws() {
        let mut stepped = JavaRandom48::new(123456789);
        let mut skipped = stepped.clone();
        for _ in 0..10_000 {
            stepped.next_bits(1);
        }
        skipped.advance(10_000);
        assert_eq!(stepped, skipped);
        assert_eq!(stepped.next_i32(), skipped.next_i32());
    }

    #[test]
    fn reseeding_restarts_the_same_stream() {
        let mut random = JavaRandom48::new(42);
        let first = random.next_i32();
        random.set_seed(42);
        assert_eq!(random.next_i32(), first);
    }
}
