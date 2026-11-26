// SPDX-License-Identifier: GPL-3.0-or-later
//! `std::mt19937` (32-bit Mersenne Twister, seeded with `init_genrand`), the generator
//! the upstream QA tests and the C++ adapter draw generated edges from. Harness code.

const N: usize = 624;
const M: usize = 397;

pub struct Mt19937 {
    state: [u32; N],
    index: usize,
}

impl Mt19937 {
    /// `std::mt19937( seed )`.
    pub fn new(seed: u32) -> Self {
        let mut state = [0u32; N];
        state[0] = seed;
        for i in 1..N {
            let previous = state[i - 1];
            state[i] = 1_812_433_253u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(i as u32);
        }
        Self { state, index: N }
    }

    fn twist(&mut self) {
        for i in 0..N {
            let y = (self.state[i] & 0x8000_0000) | (self.state[(i + 1) % N] & 0x7fff_ffff);
            let mut next = self.state[(i + M) % N] ^ (y >> 1);
            if y & 1 != 0 {
                next ^= 0x9908_b0df;
            }
            self.state[i] = next;
        }
        self.index = 0;
    }

    /// `operator()()`.
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    /// `discard( n )`.
    pub fn discard(&mut self, n: u64) {
        for _ in 0..n {
            self.next_u32();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The C++ standard requires the 10000th output of a default-seeded (5489)
    /// `std::mt19937` to be 4123659995.
    #[test]
    fn standard_check_value() {
        let mut rng = Mt19937::new(5489);
        rng.discard(9999);
        assert_eq!(rng.next_u32(), 4_123_659_995);
    }
}
