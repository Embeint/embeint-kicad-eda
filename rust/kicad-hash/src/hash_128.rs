// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kimath/include/hash_128.h`: `HASH_128`.

/// A 128-bit hash value. The C++ union views the same 16 bytes as `Value8`,
/// `Value32` and `Value64`; the views here use the qualified little-endian byte
/// order. A new value is zero (`Value8[16]{}`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hash128 {
    value8: [u8; 16],
}

impl Hash128 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_value8(value8: [u8; 16]) -> Self {
        Self { value8 }
    }

    pub fn from_value64(value64: [u64; 2]) -> Self {
        let mut hash = Self::default();
        hash.set_value64(value64);
        hash
    }

    pub fn value8(&self) -> [u8; 16] {
        self.value8
    }

    pub fn value32(&self) -> [u32; 4] {
        let mut words = [0_u32; 4];
        for (word, bytes) in words.iter_mut().zip(self.value8.chunks_exact(4)) {
            let mut load = [0_u8; 4];
            load.copy_from_slice(bytes);
            *word = u32::from_le_bytes(load);
        }
        words
    }

    pub fn value64(&self) -> [u64; 2] {
        let mut words = [0_u64; 2];
        for (word, bytes) in words.iter_mut().zip(self.value8.chunks_exact(8)) {
            let mut load = [0_u8; 8];
            load.copy_from_slice(bytes);
            *word = u64::from_le_bytes(load);
        }
        words
    }

    pub fn set_value64(&mut self, value64: [u64; 2]) {
        for (bytes, word) in self.value8.chunks_exact_mut(8).zip(value64) {
            bytes.copy_from_slice(&word.to_le_bytes());
        }
    }

    /// `Clear()`: `memset( Value64, 0, sizeof( Value64 ) )`.
    pub fn clear(&mut self) {
        self.value8 = [0; 16];
    }

    /// `operator==`: `memcmp` of all 16 bytes (the derived `PartialEq` is the same).
    pub fn equals(&self, other: &Self) -> bool {
        self.value8 == other.value8
    }

    /// `ToString()`: `Value64[0]` then `Value64[1]`, each as 16 upper-case,
    /// zero-filled hexadecimal digits (`std::hex`, `std::uppercase`,
    /// `std::setfill( '0' )`, `std::setw( 16 )`).
    pub fn to_hex_string(&self) -> String {
        let [first, second] = self.value64();
        format!("{first:016X}{second:016X}")
    }
}

impl core::fmt::Display for Hash128 {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.to_hex_string())
    }
}
