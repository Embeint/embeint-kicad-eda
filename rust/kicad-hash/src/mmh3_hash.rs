// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kimath/include/mmh3_hash.h`: the streaming MurmurHash3 x64-128 class
//! `MMH3_HASH` and the non-MSVC helper `mmh3_rotl64`.
//!
//! The pinned class is reproduced operation for operation, including its
//! deliberate quirks: `len` is a wrapping `uint32_t`; `add( int32_t )` writes the
//! word slot `( len % 16 ) / 4` without regard to a partially filled byte
//! position; `addDataV1` ignores any pending partial block and counts its
//! padding in `len`; `digest()` changes the running state and may be followed
//! by more input. The C++ private helpers are public here so the oracle can
//! compare them directly.

use crate::hash_128::Hash128;

const C1: u64 = 0x87c3_7b91_1142_53d5;
const C2: u64 = 0x4cf5_ad43_2745_937f;

/// An argument outside the domain on which the pinned C++ is defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainError {
    /// `mmh3_rotl64( x, r )` shifts by `64 - r`; only `1 <= r <= 63` is defined.
    RotateCount(i8),
    /// `getblock64( i )` indexes `blocks64[2]`.
    BlockIndex(i32),
}

impl core::fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RotateCount(r) => write!(formatter, "rotate count {r} outside 1..=63"),
            Self::BlockIndex(i) => write!(formatter, "block index {i} outside 0..=1"),
        }
    }
}

/// `mmh3_rotl64( x, r )`: `( x << r ) | ( x >> ( 64 - r ) )`. A count of 0 or
/// outside 1..=63 is undefined behaviour in C++ and is refused.
pub fn mmh3_rotl64(x: u64, r: i8) -> Result<u64, DomainError> {
    if !(1..=63).contains(&r) {
        return Err(DomainError::RotateCount(r));
    }
    // For 1 <= r <= 63, ( x << r ) | ( x >> ( 64 - r ) ) is exactly a left rotation.
    Ok(x.rotate_left(r.unsigned_abs().into()))
}

/// `ROTL64` with one of the class's constant counts, checked at compile time.
fn rotl<const R: u32>(x: u64) -> u64 {
    const { assert!(R >= 1 && R <= 63) };
    x.rotate_left(R)
}

/// `fmix64`: the finalization mix that avalanches all bits of a block.
pub fn fmix64(mut k: u64) -> u64 {
    k ^= k >> 33;
    k = k.wrapping_mul(0xff51_afd7_ed55_8ccd);
    k ^= k >> 33;
    k = k.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    k ^= k >> 33;
    k
}

/// `MMH3_HASH`. The C++ leaves `blocks` uninitialized at construction and
/// `reset` does not touch it; here it starts as zero bytes. Reading a block
/// byte that was never written is undefined in the reference (see the unit's
/// upstream-issue record); the difftest oracle zero-fills it the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mmh3Hash {
    h1: u64,
    h2: u64,
    blocks: [u8; 16],
    len: u32,
}

impl Default for Mmh3Hash {
    /// `MMH3_HASH()`: `reset( 0 )`.
    fn default() -> Self {
        let mut hash = Self::zeroed();
        hash.reset(0);
        hash
    }
}

impl Mmh3Hash {
    fn zeroed() -> Self {
        Self {
            h1: 0,
            h2: 0,
            blocks: [0; 16],
            len: 0,
        }
    }

    /// `MMH3_HASH()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `MMH3_HASH( uint32_t aSeed )`: `reset( aSeed )`.
    pub fn with_seed(seed: u32) -> Self {
        let mut hash = Self::zeroed();
        hash.reset(seed);
        hash
    }

    /// Restores a complete state, as reached after a long input stream (for
    /// example a wrapped `len`). Used by the difftest to reach such states
    /// without gigabytes of input; the oracle sets the same private members.
    pub fn from_state(h1: u64, h2: u64, blocks: [u8; 16], len: u32) -> Self {
        Self {
            h1,
            h2,
            blocks,
            len,
        }
    }

    pub fn h1(&self) -> u64 {
        self.h1
    }

    pub fn h2(&self) -> u64 {
        self.h2
    }

    pub fn blocks(&self) -> [u8; 16] {
        self.blocks
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    /// True when no byte has been counted since the last reset (or after `len` wrapped).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `reset( uint32_t aSeed = 0 )`; `blocks` keeps its contents.
    pub fn reset(&mut self, seed: u32) {
        self.h1 = u64::from(seed);
        self.h2 = u64::from(seed);
        self.len = 0;
    }

    /// `reset()` with its default argument.
    pub fn reset_default(&mut self) {
        self.reset(0);
    }

    /// `addData( const uint8_t* data, size_t length )`: completes a block an
    /// earlier call left partial, hashes whole blocks, then keeps a zero-padded tail.
    pub fn add_data(&mut self, data: &[u8]) {
        let mut data = data;
        // pending < 16, so every slice bound below stays within the 16-byte block.
        let pending = (self.len % 16) as usize;

        if pending > 0 && !data.is_empty() {
            let fill = data.len().min(16 - pending);
            let (head, rest) = data.split_at(fill);
            self.blocks[pending..pending + fill].copy_from_slice(head);
            data = rest;
            // fill <= 16; the uint32_t addition wraps.
            self.len = self.len.wrapping_add(fill as u32);

            if self.len % 16 != 0 {
                return;
            }

            self.hash_block();
        }

        while data.len() >= 16 {
            let (block, rest) = data.split_at(16);
            self.blocks.copy_from_slice(block);
            self.hash_block();
            data = rest;
            self.len = self.len.wrapping_add(16);
        }

        if !data.is_empty() {
            let remaining = data.len();
            self.blocks[..remaining].copy_from_slice(data);
            self.blocks[remaining..].fill(0);
            self.len = self.len.wrapping_add(remaining as u32);
        }
    }

    /// `addDataV1`: the pre-fix tail handling kept for verifying old files. It
    /// ignores a pending partial block, pads the tail only to a 4-byte boundary
    /// (a full extra word when already aligned) and counts the padding in `len`.
    pub fn add_data_v1(&mut self, data: &[u8]) {
        let mut data = data;

        while data.len() >= 16 {
            let (block, rest) = data.split_at(16);
            self.blocks.copy_from_slice(block);
            self.hash_block();
            data = rest;
            self.len = self.len.wrapping_add(16);
        }

        if !data.is_empty() {
            let remaining = data.len();
            self.blocks[..remaining].copy_from_slice(data);
            // remaining <= 15 gives padding in 1..=4 and remaining + padding <= 16.
            let padding = 4 - (remaining + 4) % 4;
            self.blocks[remaining..remaining + padding].fill(0);
            self.len = self.len.wrapping_add((remaining + padding) as u32);
        }
    }

    /// `add( const std::string& )`: the string's bytes through `addData`.
    pub fn add_string(&mut self, input: &[u8]) {
        self.add_data(input);
    }

    /// `add( const std::vector<char>& )`: the vector's bytes through `addData`.
    pub fn add_chars(&mut self, input: &[u8]) {
        self.add_data(input);
    }

    /// `add( int32_t )`: stores the little-endian word in slot `( len % 16 ) / 4`,
    /// counts four bytes and hashes the block only when `len` reaches a multiple of 16.
    pub fn add_i32(&mut self, input: i32) {
        let slot = ((self.len % 16) / 4) as usize * 4;
        self.blocks[slot..slot + 4].copy_from_slice(&input.to_le_bytes());
        self.len = self.len.wrapping_add(4);

        if self.len % 16 == 0 {
            self.hash_block();
        }
    }

    /// `digest()`: mixes the tail and finalizes into a new `HASH_128`. The
    /// running state keeps the changes, as in C++.
    pub fn digest(&mut self) -> Hash128 {
        self.hash_tail();

        let mut h128 = Hash128::default();
        self.hash_final(&mut h128);

        h128
    }

    /// `getblock64( int i )`: the little-endian 64-bit word `i` of the block.
    pub fn get_block64(&self, i: i32) -> Result<u64, DomainError> {
        match i {
            0 => Ok(self.block64::<0>()),
            1 => Ok(self.block64::<1>()),
            _ => Err(DomainError::BlockIndex(i)),
        }
    }

    fn block64<const I: usize>(&self) -> u64 {
        const { assert!(I < 2) };
        let mut load = [0_u8; 8];
        load.copy_from_slice(&self.blocks[I * 8..I * 8 + 8]);
        u64::from_le_bytes(load)
    }

    /// `hashBlock()`: mixes the current 16-byte block into `h1`/`h2`.
    pub fn hash_block(&mut self) {
        let mut k1 = self.block64::<0>();
        let mut k2 = self.block64::<1>();

        k1 = k1.wrapping_mul(C1);
        k1 = rotl::<31>(k1);
        k1 = k1.wrapping_mul(C2);
        self.h1 ^= k1;

        self.h1 = rotl::<27>(self.h1);
        self.h1 = self.h1.wrapping_add(self.h2);
        self.h1 = self.h1.wrapping_mul(5).wrapping_add(0x52dc_e729);

        k2 = k2.wrapping_mul(C2);
        k2 = rotl::<33>(k2);
        k2 = k2.wrapping_mul(C1);
        self.h2 ^= k2;

        self.h2 = rotl::<31>(self.h2);
        self.h2 = self.h2.wrapping_add(self.h1);
        self.h2 = self.h2.wrapping_mul(5).wrapping_add(0x3849_5ab5);
    }

    /// `hashTail()`: the fall-through `switch( len & 15 )` over the first
    /// `len & 15` block bytes; bytes 8..14 feed `k2`, bytes 0..7 feed `k1`.
    pub fn hash_tail(&mut self) {
        let tail = self.blocks;
        let count = (self.len & 15) as usize;

        let mut k1 = 0_u64;
        let mut k2 = 0_u64;

        if count >= 9 {
            for index in (8..count).rev() {
                k2 ^= u64::from(tail[index]) << ((index - 8) * 8);
            }
            k2 = k2.wrapping_mul(C2);
            k2 = rotl::<33>(k2);
            k2 = k2.wrapping_mul(C1);
            self.h2 ^= k2;
        }

        if count >= 1 {
            for index in (0..count.min(8)).rev() {
                k1 ^= u64::from(tail[index]) << (index * 8);
            }
            k1 = k1.wrapping_mul(C1);
            k1 = rotl::<31>(k1);
            k1 = k1.wrapping_mul(C2);
            self.h1 ^= k1;
        }
    }

    /// `hashFinal( HASH_128& out )`.
    pub fn hash_final(&mut self, out: &mut Hash128) {
        self.h1 ^= u64::from(self.len);
        self.h2 ^= u64::from(self.len);

        self.h1 = self.h1.wrapping_add(self.h2);
        self.h2 = self.h2.wrapping_add(self.h1);

        self.h1 = fmix64(self.h1);
        self.h2 = fmix64(self.h2);

        self.h1 = self.h1.wrapping_add(self.h2);
        self.h2 = self.h2.wrapping_add(self.h1);

        out.set_value64([self.h1, self.h2]);
    }
}
