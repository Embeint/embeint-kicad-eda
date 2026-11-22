// SPDX-License-Identifier: GPL-3.0-or-later
use md5::block_api::compress;

/// A single C++ `Hash` call accepts a 32-bit byte count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LengthError;

#[derive(Clone, Debug)]
pub struct Md5Hash {
    data: [u8; 64],
    data_len: usize,
    bit_len: u64,
    state: [u32; 4],
    hash: [u8; 16],
    valid: bool,
}

impl Default for Md5Hash {
    fn default() -> Self {
        Self {
            data: [0; 64],
            data_len: 0,
            bit_len: 0,
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            hash: [0; 16],
            valid: false,
        }
    }
}

impl PartialEq for Md5Hash {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for Md5Hash {}

impl Md5Hash {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn init(&mut self) {
        *self = Self::default();
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn set_valid(&mut self, valid: bool) {
        self.valid = valid;
    }

    pub fn bytes(&self) -> &[u8; 16] {
        &self.hash
    }

    /// Matches the pinned Linux oracle's 32-bit, little-endian `int` overload.
    pub fn hash_int(&mut self, value: i32) {
        self.update(&value.to_le_bytes());
    }

    /// Neither hashing overload changes validity or the previously stored digest.
    pub fn hash(&mut self, data: &[u8]) -> Result<(), LengthError> {
        u32::try_from(data.len()).map_err(|_| LengthError)?;
        self.update(data);
        Ok(())
    }

    fn transform(&mut self) {
        compress(&mut self.state, std::slice::from_ref(&self.data));
    }

    fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let count = data.len().min(64 - self.data_len);
            let (head, tail) = data.split_at(count);
            self.data[self.data_len..self.data_len + count].copy_from_slice(head);
            self.data_len += count;
            data = tail;
            if self.data_len == 64 {
                self.transform();
                self.bit_len = self.bit_len.wrapping_add(512);
                self.data_len = 0;
            }
        }
    }

    /// KiCad leaves data_len and the padded buffer in place, even on repeated calls.
    pub fn finalize(&mut self) {
        self.data[self.data_len] = 0x80;
        if self.data_len < 56 {
            self.data[self.data_len + 1..56].fill(0);
        } else {
            self.data[self.data_len + 1..64].fill(0);
            self.transform();
            self.data[..56].fill(0);
        }
        self.bit_len = self.bit_len.wrapping_add((self.data_len as u64) * 8);
        self.data[56..].copy_from_slice(&self.bit_len.to_le_bytes());
        self.transform();
        for (chunk, word) in self.hash.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        self.valid = true;
    }

    /// Preserves the reference's B..G mapping for nibbles 10..15 and trailing space.
    pub fn format(&self, compact: bool) -> String {
        let nibble = |value: u8| {
            char::from(if value > 9 {
                b'A' + value - 9
            } else {
                b'0' + value
            })
        };
        let mut text = String::with_capacity(if compact { 32 } else { 48 });
        for byte in self.hash {
            text.push(nibble(byte >> 4));
            text.push(nibble(byte & 15));
            if !compact {
                text.push(' ');
            }
        }
        text
    }

    pub fn format_default(&self) -> String {
        self.format(false)
    }
}
