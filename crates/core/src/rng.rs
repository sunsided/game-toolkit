use std::ops::Range;

/// Deterministic xorshift64* pseudo-random number generator.
///
/// Same seed always yields the same sequence across runs and platforms.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Fallback state used when a zero seed is supplied.
    pub const DEFAULT_SEED: u64 = 0x4d59_5df4_d0f3_3173;
    /// xorshift64* output scrambler multiplier.
    const XORSHIFT64_STAR_MULTIPLIER: u64 = 0x2545_f491_4f6c_dd1d;
    /// Number of random mantissa bits used for a uniform `f32` in `[0, 1)`.
    const F32_MANTISSA_BITS: u32 = 24;

    /// Creates a new deterministic generator from `seed`.
    pub const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { Self::DEFAULT_SEED } else { seed },
        }
    }

    /// Reseeds the generator.
    pub fn reseed(&mut self, seed: u64) {
        self.state = if seed == 0 { Self::DEFAULT_SEED } else { seed };
    }

    /// Returns the next `u64`.
    #[must_use]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(Self::XORSHIFT64_STAR_MULTIPLIER)
    }

    /// Returns the next `u32`.
    #[must_use]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Returns a uniform `u32` sampled from `range`.
    #[must_use]
    pub fn range_u32(&mut self, range: Range<u32>) -> u32 {
        assert!(range.start < range.end, "range must be non-empty");
        range.start + self.uniform_below_u32(range.end - range.start)
    }

    /// Returns a uniform `usize` sampled from `range`.
    #[must_use]
    pub fn range_usize(&mut self, range: Range<usize>) -> usize {
        assert!(range.start < range.end, "range must be non-empty");
        range.start + self.uniform_below_usize(range.end - range.start)
    }

    /// Returns a uniform `f32` in `[0, 1)`.
    #[must_use]
    pub fn next_f32(&mut self) -> f32 {
        const INV_2_POW_24: f32 = 1.0 / ((1u32 << Rng::F32_MANTISSA_BITS) as f32);
        ((self.next_u32() >> (u32::BITS - Self::F32_MANTISSA_BITS)) as f32) * INV_2_POW_24
    }

    fn uniform_below_u32(&mut self, upper_exclusive: u32) -> u32 {
        let zone = u32::MAX - (u32::MAX % upper_exclusive);
        loop {
            let value = self.next_u32();
            if value < zone {
                return value % upper_exclusive;
            }
        }
    }

    fn uniform_below_usize(&mut self, upper_exclusive: usize) -> usize {
        let zone = usize::MAX - (usize::MAX % upper_exclusive);
        loop {
            let value = self.next_u64() as usize;
            if value < zone {
                return value % upper_exclusive;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(1337);
        let mut b = Rng::new(1337);
        for _ in 0..32 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn range_u32_stays_within_bounds() {
        let mut rng = Rng::new(42);
        for _ in 0..2000 {
            let value = rng.range_u32(10..20);
            assert!((10..20).contains(&value));
        }
    }

    #[test]
    fn f32_is_in_zero_to_one() {
        let mut rng = Rng::new(1);
        for _ in 0..2000 {
            let value = rng.next_f32();
            assert!((0.0..1.0).contains(&value));
        }
    }
}
