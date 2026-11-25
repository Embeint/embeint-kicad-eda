// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/base64.cpp` (namespace `base64`).
//!
//! The reference uses `size_t`, which is 64 bits on the pinned Linux profile. The
//! length helpers therefore take and return `u64` with the same modular arithmetic,
//! so native and wasm32 results are identical. `encode`/`decode` append to the
//! output exactly like the C++ (which never clears `aOutput`).

/// Upstream `ENCODE_BASE64`.
const ENCODE_BASE64: [u8; 64] =
    *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Upstream `E`: `static const uint8_t E = -1`.
const E: u8 = 0xFF;

/// Upstream `DECODE_BASE64`, indexed by the low seven bits of an input byte.
#[rustfmt::skip]
const DECODE_BASE64: [u8; 128] = [
/*  0x0 0x1 0x2 0x3 0x4 0x5 0x6 0x7 0x8 0x9 0xA 0xB 0xC 0xD 0xE 0xF */
     E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,
     E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E,
     E,  E,  E,  E,  E,  E,  E,  E,  E,  E,  E, 62,  E,  E,  E, 63,
    52, 53, 54, 55, 56, 57, 58, 59, 60, 61,  E,  E,  E,  E,  E,  E,
     E,  0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11, 12, 13, 14,
    15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,  E,  E,  E,  E,  E,
     E, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
    41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51,  E,  E,  E,  E,  E,
];

/// The output vector could not grow (the reference would throw `std::bad_alloc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationError;

/// `base64::encode_length`: `4 * ((n + 2) / 3) + (n + 2) % 3 - 2` in modular `size_t`.
#[must_use]
pub fn encode_length(input_len: u64) -> u64 {
    let n = input_len.wrapping_add(2);
    4u64.wrapping_mul(n / 3).wrapping_add(n % 3).wrapping_sub(2)
}

/// `base64::decode_length`: zero for `n % 4 == 1`, else
/// `3 * ((n + 2) / 4) + (n + 2) % 4 - 2` in modular `size_t`.
#[must_use]
pub fn decode_length(input_len: u64) -> u64 {
    if input_len % 4 == 1 {
        0
    } else {
        let n = input_len.wrapping_add(2);
        3u64.wrapping_mul(n / 4).wrapping_add(n % 4).wrapping_sub(2)
    }
}

/// Look up an encoded character. The index is always below 64.
fn enc(value: u32) -> u8 {
    ENCODE_BASE64
        .get((value & 0x3F) as usize)
        .copied()
        .unwrap_or(0)
}

/// Look up a decoded sextet for an input byte (`DECODE_BASE64[byte & 0x7F]`).
fn dec(byte: u8) -> u32 {
    u32::from(
        DECODE_BASE64
            .get(usize::from(byte & 0x7F))
            .copied()
            .unwrap_or(E),
    )
}

/// Reserve `additional` more bytes or report the reference's allocation failure.
fn reserve(output: &mut Vec<u8>, additional: usize) -> Result<(), AllocationError> {
    output.try_reserve(additional).map_err(|_| AllocationError)
}

/// `base64::encode`: append the unpadded encoding of `input` to `output`.
///
/// # Errors
/// [`AllocationError`] when the output cannot grow.
pub fn encode(input: &[u8], output: &mut Vec<u8>) -> Result<(), AllocationError> {
    let end = (input.len() / 3) * 3;
    // aOutput.reserve( encode_length( aInput.size() ) ) only affects capacity; reserve
    // the exact appended size instead so allocation stays bounded by the input.
    let appended = (input.len() / 3)
        .checked_mul(4)
        .and_then(|n| n.checked_add([0, 2, 3][input.len() % 3]))
        .ok_or(AllocationError)?;
    reserve(output, appended)?;

    for block in input[..end].chunks_exact(3) {
        let value = (u32::from(block[0]) << 16) | (u32::from(block[1]) << 8) | u32::from(block[2]);

        output.push(enc(value >> 18));
        output.push(enc(value >> 12));
        output.push(enc(value >> 6));
        output.push(enc(value));
    }

    let rest = &input[end..];

    if let Some(&first) = rest.first() {
        let mut value = u32::from(first);

        if let Some(&second) = rest.get(1) {
            // remainder == 2
            value = (value << 10) | (u32::from(second) << 2);
            output.push(enc(value >> 12));
        } else {
            value <<= 4;
        }

        output.push(enc(value >> 6));
        output.push(enc(value));
    }

    Ok(())
}

/// `base64::decode`: append the decoding of `input` to `output`.
///
/// Trailing `=` are stripped (padding is optional). A length of `4k + 1` after
/// stripping, or zero, appends nothing. Bytes outside the alphabet are decoded
/// through the table's `0xFF` entries exactly like the reference, and the high bit
/// of every byte is ignored.
///
/// # Errors
/// [`AllocationError`] when the output cannot grow.
pub fn decode(input: &[u8], output: &mut Vec<u8>) -> Result<(), AllocationError> {
    let mut len = input.len();

    // Padding is optional; encode() never writes it
    while len > 0 && input.get(len - 1) == Some(&b'=') {
        len -= 1;
    }

    let end = (len / 4) * 4;
    let decode_size = decode_length(len as u64);

    if decode_size == 0 {
        return Ok(());
    }

    // decode_size never exceeds len for a real slice; the conversion cannot fail.
    reserve(
        output,
        usize::try_from(decode_size).map_err(|_| AllocationError)?,
    )?;

    for block in input[..end].chunks_exact(4) {
        let value =
            (dec(block[0]) << 18) | (dec(block[1]) << 12) | (dec(block[2]) << 6) | dec(block[3]);

        output.push(((value >> 16) & 0xff) as u8);
        output.push(((value >> 8) & 0xff) as u8);
        output.push((value & 0xff) as u8);
    }

    let rest = &input[end..len];

    // decode_length() is zero for a remainder of one, so a remainder has two or three bytes.
    if let [first, second, tail @ ..] = rest {
        let mut value = (dec(*first) << 6) | dec(*second);

        if let [third] = tail {
            value = (value << 6) | dec(*third);
            value >>= 2;

            output.push(((value >> 8) & 0xff) as u8);
        } else {
            value >>= 4;
        }

        output.push((value & 0xff) as u8);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        assert_eq!(decode(text.as_bytes(), &mut out), Ok(()));
        out
    }

    #[test]
    fn upstream_qa_vectors() {
        assert_eq!(decoded(""), b"");
        assert_eq!(decoded("TWFu"), b"Man");
        assert_eq!(decoded("TWE"), b"Ma");
        assert_eq!(decoded("TQ"), b"M");
        assert_eq!(decoded("TWE="), b"Ma");
        assert_eq!(decoded("TQ=="), b"M");
        assert_eq!(decoded("aGVsbG8gd29ybGQ"), b"hello world");
    }

    #[test]
    fn upstream_qa_round_trip() {
        let mut input = Vec::new();
        for len in 0..64u32 {
            let mut encoded = Vec::new();
            let mut output = Vec::new();
            assert_eq!(encode(&input, &mut encoded), Ok(()));
            assert_eq!(encoded.len() as u64, encode_length(input.len() as u64));
            assert_eq!(decode(&encoded, &mut output), Ok(()));
            assert_eq!(output, input);
            input.push((len * 37 + 11) as u8);
        }
    }

    #[test]
    fn modular_lengths() {
        assert_eq!(encode_length(0), 0);
        assert_eq!(encode_length(u64::MAX), u64::MAX);
        assert_eq!(decode_length(1), 0);
        assert_eq!(decode_length(0), 0);
        // (MAX + 2) wraps to 1: 3 * 0 + 1 - 2 wraps to MAX.
        assert_eq!(decode_length(u64::MAX), u64::MAX);
        assert_eq!(decode_length(u64::MAX - 1), u64::MAX - 1);
    }
}
