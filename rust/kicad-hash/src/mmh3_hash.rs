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
//!
//! The reference constructors and `reset` never initialize `blocks`. This port
//! therefore records which of the 16 block bytes the reference has written and
//! refuses, before changing any state, every operation whose reference
//! counterpart would read a byte that was never written (undefined behaviour in
//! C++). Such bytes are never given a value: [`Mmh3Hash::blocks`] reports them as
//! `None`. On every sequence inside the reference's defined domain the results
//! are exactly the reference's.

use crate::hash_128::Hash128;

const C1: u64 = 0x87c3_7b91_1142_53d5;
const C2: u64 = 0x4cf5_ad43_2745_937f;
const ALL_BYTES: u16 = 0xFFFF;

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

/// The reference member function whose block read would be undefined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReader {
    /// `hashBlock()` reads all 16 bytes (through `addData`, `add( int32_t )` or directly).
    HashBlock,
    /// `hashTail()` reads bytes `0 .. len & 15` (through `digest` or directly).
    HashTail,
    /// `getblock64( i )` reads bytes `8 * i .. 8 * i + 8`.
    GetBlock64,
}

impl BlockReader {
    pub fn name(self) -> &'static str {
        match self {
            Self::HashBlock => "hashBlock",
            Self::HashTail => "hashTail",
            Self::GetBlock64 => "getblock64",
        }
    }
}

/// Why an `MMH3_HASH` operation was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mmh3Error {
    Domain(DomainError),
    /// The reference would read the block bytes in `unwritten` (bit `i` is byte
    /// `i`), which no earlier operation wrote. Nothing was changed.
    IndeterminateRead {
        reader: BlockReader,
        unwritten: u16,
    },
}

impl core::fmt::Display for Mmh3Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Domain(error) => error.fmt(formatter),
            Self::IndeterminateRead { reader, unwritten } => {
                write!(formatter, "{} reads unwritten block bytes", reader.name())?;
                let mut separator = " ";
                for byte in 0..16 {
                    if unwritten & (1 << byte) != 0 {
                        write!(formatter, "{separator}{byte}")?;
                        separator = ",";
                    }
                }
                Ok(())
            }
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

/// Bits `start .. end` of a 16-bit byte mask; `end <= 16`.
fn span(start: usize, end: usize) -> u16 {
    let mut mask = 0_u16;
    for byte in start..end.min(16) {
        mask |= 1 << byte;
    }
    mask
}

/// `MMH3_HASH`, with the set of block bytes the reference has written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mmh3Hash {
    h1: u64,
    h2: u64,
    /// Storage for written bytes; a byte whose `written` bit is clear has no
    /// reference value and is never read or reported.
    blocks: [u8; 16],
    written: u16,
    len: u32,
}

impl Default for Mmh3Hash {
    /// `MMH3_HASH()`: `reset( 0 )`; `blocks` stays unwritten.
    fn default() -> Self {
        let mut hash = Self::unwritten();
        hash.reset(0);
        hash
    }
}

impl Mmh3Hash {
    fn unwritten() -> Self {
        Self {
            h1: 0,
            h2: 0,
            blocks: [0; 16],
            written: 0,
            len: 0,
        }
    }

    /// `MMH3_HASH()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `MMH3_HASH( uint32_t aSeed )`: `reset( aSeed )`.
    pub fn with_seed(seed: u32) -> Self {
        let mut hash = Self::unwritten();
        hash.reset(seed);
        hash
    }

    /// A complete, fully written state, as reached after a long input stream
    /// (for example a wrapped `len`). The difftest uses it to reach such states
    /// without gigabytes of input; the oracle sets the same private members.
    pub fn from_state(h1: u64, h2: u64, blocks: [u8; 16], len: u32) -> Self {
        Self {
            h1,
            h2,
            blocks,
            written: ALL_BYTES,
            len,
        }
    }

    pub fn h1(&self) -> u64 {
        self.h1
    }

    pub fn h2(&self) -> u64 {
        self.h2
    }

    /// The block bytes; `None` for a byte the reference has never written.
    pub fn blocks(&self) -> [Option<u8>; 16] {
        let mut bytes = [None; 16];
        for (index, (slot, byte)) in bytes.iter_mut().zip(self.blocks).enumerate() {
            if self.written & (1 << index) != 0 {
                *slot = Some(byte);
            }
        }
        bytes
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    /// True when no byte has been counted since the last reset (or after `len` wrapped).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn require(&self, reader: BlockReader, read: u16) -> Result<(), Mmh3Error> {
        Self::require_written(self.written, reader, read)
    }

    fn require_written(written: u16, reader: BlockReader, read: u16) -> Result<(), Mmh3Error> {
        let unwritten = read & !written;
        if unwritten != 0 {
            return Err(Mmh3Error::IndeterminateRead { reader, unwritten });
        }
        Ok(())
    }

    /// `reset( uint32_t aSeed = 0 )`; `blocks` keeps its contents (and its unwritten bytes).
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
    /// Refused only when completing the pending block would hash unwritten bytes.
    pub fn add_data(&mut self, data: &[u8]) -> Result<(), Mmh3Error> {
        let mut data = data;
        // pending < 16, so every slice bound below stays within the 16-byte block.
        let pending = (self.len % 16) as usize;

        if pending > 0 && !data.is_empty() {
            let fill = data.len().min(16 - pending);
            // fill <= 16; the uint32_t addition wraps.
            let completes = self.len.wrapping_add(fill as u32) % 16 == 0;
            if completes {
                let written = self.written | span(pending, pending + fill);
                Self::require_written(written, BlockReader::HashBlock, ALL_BYTES)?;
            }

            let (head, rest) = data.split_at(fill);
            self.blocks[pending..pending + fill].copy_from_slice(head);
            self.written |= span(pending, pending + fill);
            data = rest;
            self.len = self.len.wrapping_add(fill as u32);

            if !completes {
                return Ok(());
            }

            self.hash_block_written();
        }

        while data.len() >= 16 {
            let (block, rest) = data.split_at(16);
            self.blocks.copy_from_slice(block);
            self.written = ALL_BYTES;
            self.hash_block_written();
            data = rest;
            self.len = self.len.wrapping_add(16);
        }

        if !data.is_empty() {
            let remaining = data.len();
            self.blocks[..remaining].copy_from_slice(data);
            self.blocks[remaining..].fill(0);
            self.written = ALL_BYTES;
            self.len = self.len.wrapping_add(remaining as u32);
        }

        Ok(())
    }

    /// `addDataV1`: the pre-fix tail handling kept for verifying old files. It
    /// ignores a pending partial block, pads the tail only to a 4-byte boundary
    /// (a full extra word when already aligned) and counts the padding in `len`.
    /// It reads no unwritten byte, so it is never refused.
    pub fn add_data_v1(&mut self, data: &[u8]) {
        let mut data = data;

        while data.len() >= 16 {
            let (block, rest) = data.split_at(16);
            self.blocks.copy_from_slice(block);
            self.written = ALL_BYTES;
            self.hash_block_written();
            data = rest;
            self.len = self.len.wrapping_add(16);
        }

        if !data.is_empty() {
            let remaining = data.len();
            self.blocks[..remaining].copy_from_slice(data);
            // remaining <= 15 gives padding in 1..=4 and remaining + padding <= 16.
            let padding = 4 - (remaining + 4) % 4;
            self.blocks[remaining..remaining + padding].fill(0);
            self.written |= span(0, remaining + padding);
            self.len = self.len.wrapping_add((remaining + padding) as u32);
        }
    }

    /// `add( const std::string& )`: the string's bytes through `addData`.
    pub fn add_string(&mut self, input: &[u8]) -> Result<(), Mmh3Error> {
        self.add_data(input)
    }

    /// `add( const std::vector<char>& )`: the vector's bytes through `addData`.
    pub fn add_chars(&mut self, input: &[u8]) -> Result<(), Mmh3Error> {
        self.add_data(input)
    }

    /// `add( int32_t )`: stores the little-endian word in slot `( len % 16 ) / 4`,
    /// counts four bytes and hashes the block only when `len` reaches a multiple
    /// of 16. Refused when that hash would read unwritten bytes.
    pub fn add_i32(&mut self, input: i32) -> Result<(), Mmh3Error> {
        let slot = ((self.len % 16) / 4) as usize * 4;
        let written = self.written | span(slot, slot + 4);
        let completes = self.len.wrapping_add(4) % 16 == 0;
        if completes {
            Self::require_written(written, BlockReader::HashBlock, ALL_BYTES)?;
        }

        self.blocks[slot..slot + 4].copy_from_slice(&input.to_le_bytes());
        self.written = written;
        self.len = self.len.wrapping_add(4);

        if completes {
            self.hash_block_written();
        }
        Ok(())
    }

    /// `digest()`: mixes the tail and finalizes into a new `HASH_128`. The
    /// running state keeps the changes, as in C++. Refused when the tail
    /// includes unwritten bytes.
    pub fn digest(&mut self) -> Result<Hash128, Mmh3Error> {
        self.hash_tail()?;

        let mut h128 = Hash128::default();
        self.hash_final(&mut h128);

        Ok(h128)
    }

    /// `getblock64( int i )`: the little-endian 64-bit word `i` of the block.
    pub fn get_block64(&self, i: i32) -> Result<u64, Mmh3Error> {
        match i {
            0 => {
                self.require(BlockReader::GetBlock64, span(0, 8))?;
                Ok(self.block64::<0>())
            }
            1 => {
                self.require(BlockReader::GetBlock64, span(8, 16))?;
                Ok(self.block64::<1>())
            }
            _ => Err(Mmh3Error::Domain(DomainError::BlockIndex(i))),
        }
    }

    fn block64<const I: usize>(&self) -> u64 {
        const { assert!(I < 2) };
        let mut load = [0_u8; 8];
        load.copy_from_slice(&self.blocks[I * 8..I * 8 + 8]);
        u64::from_le_bytes(load)
    }

    /// `hashBlock()`: mixes the current 16-byte block into `h1`/`h2`.
    pub fn hash_block(&mut self) -> Result<(), Mmh3Error> {
        self.require(BlockReader::HashBlock, ALL_BYTES)?;
        self.hash_block_written();
        Ok(())
    }

    /// `hashBlock()` once every block byte is known to be written.
    fn hash_block_written(&mut self) {
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
    pub fn hash_tail(&mut self) -> Result<(), Mmh3Error> {
        let count = (self.len & 15) as usize;
        self.require(BlockReader::HashTail, span(0, count))?;

        let tail = self.blocks;
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
        Ok(())
    }

    /// `hashFinal( HASH_128& out )`; reads no block byte.
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
