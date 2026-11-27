// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Boost.Random 1.83.0: mersenne_twister.hpp (Copyright Jens Maurer 2000-2001,
// Steven Watanabe 2010), uniform_int_distribution.hpp and uniform_real_distribution.hpp
// (Copyright Jens Maurer 2000-2001, Steven Watanabe 2011), detail/signed_unsigned_tools.hpp
// (Copyright Jens Maurer 2006) and detail/seed.hpp (Copyright Steven Watanabe 2009), Boost
// Software License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]

//! Faithful port of the parts of Boost.Random 1.83.0 that KiCad's
//! `POISSON_DISK::ToroidalUnitTile` instantiates, as shipped in the oracle's locked
//! `libboost1.83-dev 1.83.0-2.1ubuntu3.2` package (`BOOST_VERSION 108300`):
//!
//! - [`Mt19937`]: `boost::random::mt19937`, the
//!   `mersenne_twister_engine<uint32_t,32,624,397,31,0x9908b0df,11,0xffffffff,7,0x9d2c5680,15,0xefc60000,18,1812433253>`
//!   typedef, with its integer seeding (`seed(value)` and `normalize_state()`), its split
//!   `twist()` and tempered `operator()()`.
//! - [`uniform_int`]: `uniform_int_distribution<std::size_t>::operator()` over that engine,
//!   i.e. `detail::generate_uniform_int` for a 64-bit unsigned result type and a 32-bit engine,
//!   with every branch of that instantiation (`range == 0` without a draw, `brange == range`,
//!   the concatenating rejection loop for `brange < range`, and the bucket rejection loop).
//! - [`uniform_real`]: `uniform_real_distribution<double>::operator()`, i.e.
//!   `detail::generate_uniform_real` with its range-halving guard and the integral-engine
//!   rejection loop.
//!
//! No existing crate reproduces these exact algorithms: `rand`'s distributions use different
//! constructions, and `rand_mt` follows the reference `init_genrand` without Boost's
//! `normalize_state()` and has no Boost distributions. The engine is small, so it is ported here
//! from the pinned header rather than taken from a crate with a different seeding contract.
//!
//! Draw order matters: every engine call is made at the same point, and as often, as in the
//! pinned headers, so a caller that reproduces the reference's call order reproduces its values.

/// `n`, the state size of `mt19937`.
const N: usize = 624;
/// `m`, the shift size.
const M: usize = 397;
/// `r`, the mask bits.
const R: u32 = 31;
/// `w`, the word size.
const W: u32 = 32;
/// `a`, the twist matrix XOR mask.
const A: u32 = 0x9908_b0df;
/// Tempering parameters `u`, `d`, `s`, `b`, `t`, `c`, `l`.
const U: u32 = 11;
const D: u32 = 0xffff_ffff;
const S: u32 = 7;
const B: u32 = 0x9d2c_5680;
const T: u32 = 15;
const C: u32 = 0xefc6_0000;
const L: u32 = 18;
/// `f`, the initialisation multiplier.
const F: u32 = 1_812_433_253;

/// `boost::random::mt19937`. `x` is the state array and `i` the index of the next word,
/// exactly the members of the Boost engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mt19937 {
    x: [u32; N],
    i: usize,
}

impl Mt19937 {
    /// `(eng.min)()`.
    pub const MIN: u32 = 0;
    /// `(eng.max)()`.
    pub const MAX: u32 = u32::MAX;

    /// `explicit mt19937(const uint32_t& value)`, which calls `seed(value)`.
    pub fn new(value: u32) -> Self {
        let mut engine = Self { x: [0; N], i: 0 };
        engine.seed(value);
        engine
    }

    /// `seed(const UIntType& value)`.
    pub fn seed(&mut self, value: u32) {
        // const UIntType mask = (max)();
        let mask = Self::MAX;
        self.x[0] = value & mask;
        // for (i = 1; i < n; i++) uses the member i, which ends at n.
        self.i = 1;
        while self.i < N {
            let previous = self.x[self.i - 1];
            // (f * (x[i-1] ^ (x[i-1] >> (w-2))) + i) & mask: the product wraps in
            // uint32_t, the sum is formed in size_t and masked back to 32 bits.
            self.x[self.i] = (F.wrapping_mul(previous ^ (previous >> (W - 2))) as usize)
                .wrapping_add(self.i) as u32
                & mask;
            self.i += 1;
        }
        self.normalize_state();
    }

    /// `normalize_state()`: rewrites the low bits of `x[0]` from the rewound state and fixes an
    /// all-zero state. Neither changes any output of the integer seeding above (`twist()` reads
    /// only the top bit of the old `x[0]`, and a seeded state is never all zero), but the state
    /// is kept identical to Boost's.
    fn normalize_state(&mut self) {
        let upper_mask: u32 = (!0u32) << R;
        let lower_mask: u32 = !upper_mask;
        let mut y0 = self.x[M - 1] ^ self.x[N - 1];
        if y0 & (1u32 << (W - 1)) != 0 {
            y0 = ((y0 ^ A) << 1) | 1;
        } else {
            y0 <<= 1;
        }
        self.x[0] = (self.x[0] & upper_mask) | (y0 & lower_mask);

        // fix up the state if it's all zeroes.
        if self.x.iter().any(|&word| word != 0) {
            return;
        }
        self.x[0] = 1u32 << (W - 1);
    }

    /// `twist()`, with Boost's loop split (unroll factor 6).
    fn twist(&mut self) {
        let upper_mask: u32 = (!0u32) << R;
        let lower_mask: u32 = !upper_mask;

        const UNROLL_FACTOR: usize = 6;
        const UNROLL_EXTRA1: usize = (N - M) % UNROLL_FACTOR;
        const UNROLL_EXTRA2: usize = (M - 1) % UNROLL_FACTOR;

        let x = &mut self.x;
        for j in 0..N - M - UNROLL_EXTRA1 {
            let y = (x[j] & upper_mask) | (x[j + 1] & lower_mask);
            x[j] = x[j + M] ^ (y >> 1) ^ ((x[j + 1] & 1).wrapping_mul(A));
        }
        for j in N - M - UNROLL_EXTRA1..N - M {
            let y = (x[j] & upper_mask) | (x[j + 1] & lower_mask);
            x[j] = x[j + M] ^ (y >> 1) ^ ((x[j + 1] & 1).wrapping_mul(A));
        }
        for j in N - M..N - 1 - UNROLL_EXTRA2 {
            let y = (x[j] & upper_mask) | (x[j + 1] & lower_mask);
            x[j] = x[j - (N - M)] ^ (y >> 1) ^ ((x[j + 1] & 1).wrapping_mul(A));
        }
        // (m-1) % 6 == 0 for mt19937, so this split part is empty, as in the pinned header.
        #[allow(clippy::reversed_empty_ranges)]
        for j in N - 1 - UNROLL_EXTRA2..N - 1 {
            let y = (x[j] & upper_mask) | (x[j + 1] & lower_mask);
            x[j] = x[j - (N - M)] ^ (y >> 1) ^ ((x[j + 1] & 1).wrapping_mul(A));
        }
        // last iteration
        let y = (x[N - 1] & upper_mask) | (x[0] & lower_mask);
        x[N - 1] = x[M - 1] ^ (y >> 1) ^ ((x[0] & 1).wrapping_mul(A));
        self.i = 0;
    }

    /// `operator()()`.
    pub fn next_u32(&mut self) -> u32 {
        if self.i == N {
            self.twist();
        }
        // Step 4
        let mut z = self.x[self.i];
        self.i += 1;
        z ^= (z >> U) & D;
        z ^= (z << S) & B;
        z ^= (z << T) & C;
        z ^= z >> L;
        z
    }
}

/// `uniform_int_distribution<std::size_t>( aMin, aMax )`: a validated range whose
/// [`sample`](Self::sample) is `operator()( eng )`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UniformIntDistribution {
    min: u64,
    max: u64,
}

impl UniformIntDistribution {
    /// The constructor; `None` for `min > max` (see [`uniform_int`]).
    pub fn new(min: u64, max: u64) -> Option<Self> {
        (min <= max).then_some(Self { min, max })
    }

    /// `operator()( eng )`.
    pub fn sample(&self, eng: &mut Mt19937) -> u64 {
        generate_uniform_int(eng, self.min, self.max)
    }
}

/// `uniform_real_distribution<double>( aMin, aMax )`: a validated range whose
/// [`sample`](Self::sample) is `operator()( eng )`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UniformRealDistribution {
    min: f64,
    max: f64,
}

impl UniformRealDistribution {
    /// The constructor, for finite `min < max` only (see [`InvalidRealRange`]).
    pub fn new(min: f64, max: f64) -> Result<Self, InvalidRealRange> {
        if !(min.is_finite() && max.is_finite() && min < max) {
            return Err(InvalidRealRange { min, max });
        }
        Ok(Self { min, max })
    }

    /// `operator()( eng )`.
    pub fn sample(&self, eng: &mut Mt19937) -> f64 {
        generate_uniform_real(eng, self.min, self.max)
    }
}

/// `uniform_int_distribution<std::size_t>( aMin, aMax )( eng )` on [`Mt19937`]: Boost's
/// `detail::generate_uniform_int` with `T = range_type = std::size_t` (64-bit),
/// `base_result = base_unsigned = uint32_t`, `bmin = 0` and `brange = 0xffffffff`.
///
/// Returns `None` when `min_value > max_value`, which the distribution's constructor rejects
/// with `BOOST_ASSERT` (compiled out under `NDEBUG`, after which the subtraction wraps and the
/// result is unrelated to the requested range); no draw is made then.
pub fn uniform_int(eng: &mut Mt19937, min_value: u64, max_value: u64) -> Option<u64> {
    UniformIntDistribution::new(min_value, max_value).map(|d| d.sample(eng))
}

/// `detail::generate_uniform_int` for the instantiation described at [`uniform_int`].
/// Callers guarantee `min_value <= max_value`.
fn generate_uniform_int(eng: &mut Mt19937, min_value: u64, max_value: u64) -> u64 {
    // range = subtract<size_t>()(max_value, min_value): unsigned, plain subtraction.
    let range: u64 = max_value.wrapping_sub(min_value);
    let bmin: u32 = Mt19937::MIN;
    let brange: u32 = Mt19937::MAX.wrapping_sub(Mt19937::MIN);

    if range == 0 {
        min_value
    } else if u64::from(brange) == range {
        // this will probably never happen in real life
        // basically nothing to do; just take care we don't overflow / underflow
        let v: u32 = eng.next_u32().wrapping_sub(bmin);
        // add<uint32_t, size_t>: T2(x) + y.
        u64::from(v).wrapping_add(min_value)
    } else if u64::from(brange) < range {
        // use rejection method to handle things like 0..3 --> 0..4
        loop {
            // limit == floor((range+1)/(brange+1)); Therefore limit*(brange+1) <= range+1
            let base = u64::from(brange) + 1;
            let limit: u64 = if range == u64::MAX {
                let mut limit = range / base;
                if range % base == u64::from(brange) {
                    limit += 1;
                }
                limit
            } else {
                (range + 1) / base
            };

            // We consider "result" as expressed to base (brange+1):
            // For every power of (brange+1), we determine a random factor
            let mut result: u64 = 0;
            let mut mult: u64 = 1;

            // loop invariants: result < mult, mult <= range
            while mult <= limit {
                result = result
                    .wrapping_add(u64::from(eng.next_u32().wrapping_sub(bmin)).wrapping_mul(mult));

                // equivalent to (mult * (brange+1)) == range+1, but avoids overflow.
                if mult.wrapping_mul(u64::from(brange)) == range.wrapping_sub(mult).wrapping_add(1)
                {
                    // The destination range is an integer power of the generator's range.
                    return result;
                }

                mult = mult.wrapping_mul(u64::from(brange).wrapping_add(1));
            }

            // range/mult < brange+1  -> no endless loop. The recursive call's range is at most
            // brange, so it takes one of the two branches below and recurses no further.
            let mut result_increment: u64 = generate_uniform_int(eng, 0, range / mult);
            if u64::MAX / mult < result_increment {
                // The multiplication would overflow.  Reject immediately.
                continue;
            }
            result_increment = result_increment.wrapping_mul(mult);
            // unsigned integers are guaranteed to wrap on overflow.
            result = result.wrapping_add(result_increment);
            if result < result_increment {
                // The addition overflowed.  Reject.
                continue;
            }
            if result > range {
                // Too big.  Reject.
                continue;
            }
            // add<size_t, size_t>: T2(x) + y.
            return result.wrapping_add(min_value);
        }
    } else {
        // brange > range. mixed_range_type is base_unsigned (uint32_t) in a C++11-or-later GCC
        // build; with BOOST_NO_CXX11_EXPLICIT_CONVERSION_OPERATORS it would be size_t, which gives
        // the same values here because range < brange fits either type.
        let range32 = range as u32;
        // brange == UINT32_MAX for mt19937, so Boost's `brange + 1` alternative is not
        // instantiated: it cannot be reached by this engine.
        let mut bucket_size: u32 = brange / (range32 + 1);
        if brange % (range32 + 1) == range32 {
            bucket_size += 1;
        }
        loop {
            let mut result: u32 = eng.next_u32().wrapping_sub(bmin);
            result /= bucket_size;
            if result <= range32 {
                // add<uint32_t, size_t>: T2(x) + y.
                return u64::from(result).wrapping_add(min_value);
            }
        }
    }
}

/// Refusal of a `uniform_real_distribution<double>` range for which the pinned algorithm is
/// not a terminating draw: Boost requires `min <= max`; `min == max` never satisfies
/// `result < max` and loops forever, a NaN bound never compares, and an infinite bound makes the
/// halving guard recurse without end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvalidRealRange {
    pub min: f64,
    pub max: f64,
}

/// `uniform_real_distribution<double>( aMin, aMax )( eng )` on [`Mt19937`]:
/// `detail::generate_uniform_real`. Only finite `min < max` is accepted (see
/// [`InvalidRealRange`]); no draw is made for a refused range.
pub fn uniform_real(
    eng: &mut Mt19937,
    min_value: f64,
    max_value: f64,
) -> Result<f64, InvalidRealRange> {
    UniformRealDistribution::new(min_value, max_value).map(|d| d.sample(eng))
}

/// The dispatching `generate_uniform_real(eng, min_value, max_value)`. For finite
/// `min < max` the halving branch is taken at most once: after halving, the difference of the
/// quartered bounds is at most `DBL_MAX / 2`.
fn generate_uniform_real(eng: &mut Mt19937, min_value: f64, max_value: f64) -> f64 {
    if max_value / 2.0 - min_value / 2.0 > f64::MAX / 2.0 {
        return 2.0 * generate_uniform_real(eng, min_value / 2.0, max_value / 2.0);
    }
    generate_uniform_real_integral(eng, min_value, max_value)
}

/// `generate_uniform_real(eng, min, max, true_type)`: the integral-engine rejection loop.
fn generate_uniform_real_integral(eng: &mut Mt19937, min_value: f64, max_value: f64) -> f64 {
    loop {
        let numerator = f64::from(eng.next_u32().wrapping_sub(Mt19937::MIN));
        let divisor = f64::from(Mt19937::MAX.wrapping_sub(Mt19937::MIN)) + 1.0;
        let result = numerator / divisor * (max_value - min_value) + min_value;
        if result < max_value {
            return result;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The C++ and Boost check value: the 10000th output of a default-seeded (5489) mt19937.
    #[test]
    fn mt19937_check_value() {
        let mut rng = Mt19937::new(5489);
        for _ in 0..9999 {
            rng.next_u32();
        }
        assert_eq!(rng.next_u32(), 4_123_659_995);
    }

    /// The reference implementation's first outputs for init_genrand(5489).
    #[test]
    fn mt19937_first_outputs() {
        let mut rng = Mt19937::new(5489);
        assert_eq!(rng.next_u32(), 3_499_211_612);
        assert_eq!(rng.next_u32(), 581_869_302);
        assert_eq!(rng.next_u32(), 3_890_346_734);
    }

    #[test]
    fn zero_range_draws_nothing() {
        let mut a = Mt19937::new(7);
        let b = a.clone();
        assert_eq!(uniform_int(&mut a, 5, 5), Some(5));
        assert_eq!(a, b);
        assert_eq!(uniform_int(&mut a, 6, 5), None);
        assert_eq!(a, b);
    }

    #[test]
    fn full_engine_range_is_one_draw() {
        let mut a = Mt19937::new(11);
        let mut b = a.clone();
        let v = uniform_int(&mut a, 3, 3 + u64::from(u32::MAX));
        assert_eq!(v, Some(3 + u64::from(b.next_u32())));
        assert_eq!(a, b);
    }

    #[test]
    fn full_u64_range_concatenates_two_draws() {
        let mut a = Mt19937::new(13);
        let mut b = a.clone();
        let v = uniform_int(&mut a, 0, u64::MAX);
        let low = u64::from(b.next_u32());
        let high = u64::from(b.next_u32());
        assert_eq!(v, Some(low | (high << 32)));
        assert_eq!(a, b);
    }

    #[test]
    fn unit_real_is_one_scaled_draw() {
        let mut a = Mt19937::new(17);
        let mut b = a.clone();
        let v = uniform_real(&mut a, 0.0, 1.0);
        assert_eq!(v, Ok(f64::from(b.next_u32()) / 4_294_967_296.0));
        assert!(uniform_real(&mut a, 1.0, 1.0).is_err());
        assert!(uniform_real(&mut a, 0.0, f64::INFINITY).is_err());
        assert!(uniform_real(&mut a, f64::NAN, 1.0).is_err());
    }
}
