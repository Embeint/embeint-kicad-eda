// SPDX-License-Identifier: GPL-3.0-or-later
//! `std::hash<std::string>` of libstdc++ on x86-64: `std::_Hash_bytes( data, size,
//! 0xc70f6907 )` (`libstdc++-v3/libsupc++/hash_bytes.cc`, the 64-bit `size_t` variant, a
//! MurmurHash64A derivative). The reference runtime is the pinned `libstdc++.so.6`; the
//! function is unchanged between GCC 13 and 15.

/// `static_cast<size_t>( 0xc70f6907UL )`, the seed `std::hash<std::string>` passes.
pub const STRING_HASH_SEED: u64 = 0xc70f_6907;

const MUL: u64 = (0xc6a4_a793u64 << 32) + 0x5bd1_e995;

fn shift_mix(v: u64) -> u64 {
    v ^ (v >> 47)
}

/// `load_bytes( p, n )`: the `n` (1..=7) bytes at `p` as a little-endian integer, the last
/// byte most significant.
fn load_bytes(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .rev()
        .fold(0u64, |acc, &b| (acc << 8).wrapping_add(u64::from(b)))
}

/// `std::_Hash_bytes( ptr, len, seed )` for a 64-bit `size_t`.
pub fn hash_bytes(bytes: &[u8], seed: u64) -> u64 {
    let len = bytes.len() as u64;
    let mut hash = seed ^ len.wrapping_mul(MUL);
    let mut chunks = bytes.chunks_exact(8);
    for chunk in &mut chunks {
        // unaligned_load: memcpy into a size_t, little-endian on x86-64.
        let mut word = [0u8; 8];
        word.copy_from_slice(chunk);
        let data = shift_mix(u64::from_le_bytes(word).wrapping_mul(MUL)).wrapping_mul(MUL);
        hash ^= data;
        hash = hash.wrapping_mul(MUL);
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        hash ^= load_bytes(tail);
        hash = hash.wrapping_mul(MUL);
    }
    hash = shift_mix(hash).wrapping_mul(MUL);
    shift_mix(hash)
}

/// `std::hash<std::string>{}( s )`.
pub fn string_hash(bytes: &[u8]) -> u64 {
    hash_bytes(bytes, STRING_HASH_SEED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_bytes_are_little_endian() {
        assert_eq!(load_bytes(&[1, 2]), 0x0201);
        assert_eq!(load_bytes(&[0xff]), 0xff);
    }

    #[test]
    fn hash_depends_on_every_byte() {
        let a = string_hash(b"abcdefghi");
        let b = string_hash(b"abcdefghj");
        assert_ne!(a, b);
        assert_ne!(string_hash(b""), string_hash(b"\0"));
    }
}
