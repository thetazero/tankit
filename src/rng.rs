//! Small deterministic PRNG (splitmix64). Seeded proc-gen must be reproducible.

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    /// The generator's whole state, to save and pick up again (`from_state`).
    pub fn state(&self) -> u64 {
        self.0
    }

    pub fn from_state(state: u64) -> Self {
        Rng(state)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f32()
    }

    /// Uniform integer in [a, b] inclusive.
    pub fn int(&mut self, a: i32, b: i32) -> i32 {
        if b <= a {
            return a;
        }
        a + (self.next_u64() % (b - a + 1) as u64) as i32
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }

    /// Standard normal (mean 0, deviation 1), by Box-Muller.
    pub fn normal(&mut self) -> f32 {
        let u = 1.0 - self.f32(); // (0, 1]: ln stays finite
        let v = self.f32();
        (-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
    }

    /// How many of `n` things, each one with chance `p`, come up: exactly
    /// for a few dozen, by the normal approximation (rounded, clamped to
    /// 0..=n) past that.
    pub fn binomial(&mut self, n: u32, p: f32) -> u32 {
        let p = p.clamp(0.0, 1.0);
        if n <= 48 {
            return (0..n).filter(|_| self.chance(p)).count() as u32;
        }
        let mean = n as f32 * p;
        let sd = (mean * (1.0 - p)).sqrt();
        (mean + sd * self.normal()).round().clamp(0.0, n as f32) as u32
    }

    pub fn index(&mut self, len: usize) -> usize {
        (self.next_u64() % len.max(1) as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    /// A binomial draw is never more than its trials, and over many draws
    /// averages `n p`, small `n` (exact) and large (approximated) alike.
    #[test]
    fn binomial_averages_n_p() {
        let mut rng = Rng::new(3);
        for (n, p) in [(10, 0.3), (40, 0.03), (500, 0.03), (2000, 0.5)] {
            let draws = 4000;
            let mut sum = 0u64;
            for _ in 0..draws {
                let k = rng.binomial(n, p);
                assert!(k <= n);
                sum += k as u64;
            }
            let mean = sum as f32 / draws as f32;
            let want = n as f32 * p;
            assert!(
                (mean - want).abs() < 0.05 * want + 0.1,
                "{n} {p}: {mean} vs {want}"
            );
        }
        assert_eq!(rng.binomial(7, 0.0), 0);
        assert_eq!(rng.binomial(7, 1.0), 7);
    }
}
