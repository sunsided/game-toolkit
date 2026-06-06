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

    /// Reseeds the generator. A zero seed falls back to [`Self::DEFAULT_SEED`].
    pub const fn reseed(&mut self, seed: u64) {
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

    /// Returns a uniform `u32` sampled from the half-open `range`.
    ///
    /// # Panics
    ///
    /// Panics if `range` is empty (`range.start >= range.end`).
    #[must_use]
    pub fn range_u32(&mut self, range: Range<u32>) -> u32 {
        assert!(range.start < range.end, "range must be non-empty");
        range.start + self.uniform_below_u32(range.end - range.start)
    }

    /// Returns a uniform `usize` sampled from the half-open `range`.
    ///
    /// # Panics
    ///
    /// Panics if `range` is empty (`range.start >= range.end`).
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
        // Reject-sample entirely in u64 so range_usize yields the same sequence on 32- and
        // 64-bit targets, honouring the across-platforms determinism guarantee. The accepted
        // value is already < upper_exclusive, so the cast back to usize is lossless on both.
        let upper = upper_exclusive as u64;
        let zone = u64::MAX - (u64::MAX % upper);
        loop {
            let value = self.next_u64();
            if value < zone {
                return (value % upper) as usize;
            }
        }
    }
}

impl Default for Rng {
    /// Seeds from [`Rng::DEFAULT_SEED`].
    fn default() -> Self {
        Self::new(Self::DEFAULT_SEED)
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
    fn range_usize_stays_within_bounds() {
        let mut rng = Rng::new(7);
        for _ in 0..2000 {
            let value = rng.range_usize(3..9);
            assert!((3..9).contains(&value));
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

    #[test]
    fn reseed_restores_the_sequence() {
        let mut rng = Rng::new(99);
        let first: Vec<u64> = (0..8).map(|_| rng.next_u64()).collect();
        rng.reseed(99);
        let second: Vec<u64> = (0..8).map(|_| rng.next_u64()).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn zero_seed_falls_back_to_default() {
        // A zero seed must not freeze the generator at state 0 (xorshift is a no-op there).
        assert_eq!(Rng::new(0).next_u64(), Rng::new(Rng::DEFAULT_SEED).next_u64());
        assert_eq!(Rng::default().next_u64(), Rng::new(Rng::DEFAULT_SEED).next_u64());
    }
}
