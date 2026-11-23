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

    /// `ToString()` in the classic C++ locale: `Value64[0]` then `Value64[1]`,
    /// each as 16 upper-case, zero-filled hexadecimal digits (`std::hex`,
    /// `std::uppercase`, `std::setfill( '0' )`, `std::setw( 16 )`). Equal to
    /// [`Hash128::to_string_in`] with [`CxxNumericGrouping::classic`].
    pub fn to_hex_string(&self) -> String {
        let [first, second] = self.value64();
        format!("{first:016X}{second:016X}")
    }

    /// `ToString()` under a caller-supplied global C++ locale. The reference
    /// builds a `std::stringstream`, which takes the global locale's
    /// `std::numpunct<char>`; libstdc++'s `num_put` then inserts the thousands
    /// separator according to the grouping (hexadecimal included) before the
    /// `setw( 16 )` zero fill, which is applied on the left of the grouped text
    /// and never truncates. The bytes need not be UTF-8 (the separator is any
    /// `char`). Other facets (for example a custom `ctype<char>`) are outside
    /// this contract and must be classic.
    pub fn to_string_in(&self, locale: &CxxNumericGrouping) -> Vec<u8> {
        let mut text = Vec::with_capacity(32);
        for word in self.value64() {
            let digits = format!("{word:X}").into_bytes();
            let grouped = if locale.use_grouping() {
                add_grouping(locale.thousands_sep, &locale.grouping, &digits)
            } else {
                digits
            };
            // setw( 16 ) with setfill( '0' ) and the default (right) adjustment.
            text.resize(text.len() + 16_usize.saturating_sub(grouped.len()), b'0');
            text.extend_from_slice(&grouped);
        }
        text
    }
}

impl core::fmt::Display for Hash128 {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.to_hex_string())
    }
}

/// The largest `numpunct<char>::grouping()` string accepted, in bytes. A 64-bit
/// value has at most 16 hexadecimal digits, so longer strings cannot change the
/// result; they are refused rather than truncated.
pub const MAX_GROUPING_BYTES: usize = 64;

/// A grouping string longer than [`MAX_GROUPING_BYTES`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupingTooLong(pub usize);

impl core::fmt::Display for GroupingTooLong {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "grouping of {} bytes exceeds {MAX_GROUPING_BYTES}",
            self.0
        )
    }
}

/// The immutable `std::numpunct<char>` values of a global C++ locale that
/// affect integer output: `thousands_sep()` and `grouping()`, whose bytes are
/// interpreted as `char` (signed on the qualified x86-64 platform).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CxxNumericGrouping {
    thousands_sep: u8,
    grouping: Vec<u8>,
}

impl Default for CxxNumericGrouping {
    fn default() -> Self {
        Self::classic()
    }
}

impl CxxNumericGrouping {
    /// The classic `"C"` locale: separator `','` and no grouping.
    pub fn classic() -> Self {
        Self {
            thousands_sep: b',',
            grouping: Vec::new(),
        }
    }

    pub fn new(thousands_sep: u8, grouping: &[u8]) -> Result<Self, GroupingTooLong> {
        if grouping.len() > MAX_GROUPING_BYTES {
            return Err(GroupingTooLong(grouping.len()));
        }
        Ok(Self {
            thousands_sep,
            grouping: grouping.to_vec(),
        })
    }

    pub fn thousands_sep(&self) -> u8 {
        self.thousands_sep
    }

    pub fn grouping(&self) -> &[u8] {
        &self.grouping
    }

    /// libstdc++ `__numpunct_cache::_M_use_grouping`: a nonempty grouping whose
    /// first group is positive and not `CHAR_MAX`.
    fn use_grouping(&self) -> bool {
        self.grouping
            .first()
            .is_some_and(|first| (*first as i8) > 0 && *first != 127)
    }
}

/// libstdc++ `std::__add_grouping` (`bits/locale_facets.tcc`) for `char`.
/// Groups are taken from the right; the last group repeats; a group that is not
/// positive or is `CHAR_MAX` ends grouping. Requires a nonempty `grouping`.
fn add_grouping(sep: u8, grouping: &[u8], digits: &[u8]) -> Vec<u8> {
    let group = |index: usize| grouping.get(index).map_or(0, |byte| *byte as i8);
    let valid = |index: usize| group(index) > 0 && grouping.get(index) != Some(&127);
    let last_index = grouping.len().saturating_sub(1);

    let mut index = 0;
    let mut repeats = 0_usize;
    let mut last = digits.len();
    // while( __last - __first > __gbeg[__idx] && __gbeg[__idx] > 0 && != CHAR_MAX )
    while (last as i64) > i64::from(group(index)) && valid(index) {
        last -= group(index).unsigned_abs() as usize;
        if index < last_index {
            index += 1;
        } else {
            repeats += 1;
        }
    }

    let mut out = Vec::with_capacity(digits.len() * 2);
    out.extend_from_slice(digits.get(..last).unwrap_or_default());
    let mut next = last;
    let mut copy_group = |out: &mut Vec<u8>, size: i8| {
        out.push(sep);
        for _ in 0..size.max(0) {
            if let Some(digit) = digits.get(next) {
                out.push(*digit);
            }
            next += 1;
        }
    };
    for _ in 0..repeats {
        copy_group(&mut out, group(index));
    }
    while index > 0 {
        index -= 1;
        copy_group(&mut out, group(index));
    }
    out
}
