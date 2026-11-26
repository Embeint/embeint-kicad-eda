// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/include/core/union_find.h`: `KI_UNION_FIND`, a disjoint-set over
//! a dense range of indices whose trees are rooted at their smallest index.
//!
//! The reference stores `std::atomic<uint32_t>` parents so `Unite()` can run on many
//! threads. Portable code has no threads: a caller holds the set exclusively, and the
//! atomic loads, stores and compare-exchanges are performed in program order with
//! their single-threaded results (a compare-exchange still compares and reports
//! failure, as written). Arguments are `size_t` in the reference; here `u64`, so the
//! reference's `static_cast<uint32_t>` truncation is the same on every target.

use core::fmt;

/// `KI_UNION_FIND` failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnionFindError {
    /// `Reset` (and the constructor) throw `std::length_error` with this message for a
    /// count above 2^32 - 1; the set is then unchanged.
    LengthError,
    /// The parent array could not be allocated (the reference throws `bad_alloc`).
    Allocation,
    /// An index (after truncation to 32 bits) is not below `Size()`: the reference
    /// indexes outside its vector (undefined).
    IndexOutOfRange,
}

impl UnionFindError {
    /// `what()` of the reference's `std::length_error`.
    pub const LENGTH_ERROR_WHAT: &'static str = "KI_UNION_FIND exceeds its 2^32 element limit";
}

impl fmt::Display for UnionFindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LengthError => f.write_str(Self::LENGTH_ERROR_WHAT),
            Self::Allocation => f.write_str("allocation failed"),
            Self::IndexOutOfRange => f.write_str("index out of range"),
        }
    }
}

/// `KI_UNION_FIND`. Neither `Clone` nor `Copy`, like the reference.
#[derive(Debug, PartialEq, Eq)]
pub struct KiUnionFind {
    parent: Vec<u32>,
    components: u64,
}

impl KiUnionFind {
    /// `explicit KI_UNION_FIND( size_t aCount )`: `Reset( aCount )` on an empty set.
    ///
    /// # Errors
    /// [`UnionFindError::LengthError`] or [`UnionFindError::Allocation`].
    pub fn new(count: u64) -> Result<Self, UnionFindError> {
        let mut set = Self {
            parent: Vec::new(),
            components: 0,
        };
        set.reset(count)?;
        Ok(set)
    }

    /// `Reset( aCount )`: discard all unions; `count` single-element components.
    ///
    /// # Errors
    /// [`UnionFindError::LengthError`] above 2^32 - 1, checked first, or
    /// [`UnionFindError::Allocation`]; the set is then unchanged.
    pub fn reset(&mut self, count: u64) -> Result<(), UnionFindError> {
        // Parents are uint32_t to keep the array dense, so a larger set would alias
        // index 2^32 onto index zero.
        if count > u64::from(u32::MAX) {
            return Err(UnionFindError::LengthError);
        }

        let length = usize::try_from(count).map_err(|_| UnionFindError::Allocation)?;
        let mut fresh = Vec::new();
        fresh
            .try_reserve_exact(length)
            .map_err(|_| UnionFindError::Allocation)?;

        // The fresh vector is built, then swapped in, then filled with ii -> ii.
        // count <= u32::MAX, so every index converts exactly.
        fresh.extend((0..=u32::MAX).take(length));
        self.parent = fresh;
        self.components = count;
        Ok(())
    }

    /// `Size()`.
    pub fn size(&self) -> u64 {
        self.parent.len() as u64
    }

    /// `ComponentCount()`: the number of components that remain.
    pub fn component_count(&self) -> u64 {
        self.components
    }

    /// `m_parent[i].load()`.
    fn load(&self, index: u32) -> Result<u32, UnionFindError> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.parent.get(i))
            .copied()
            .ok_or(UnionFindError::IndexOutOfRange)
    }

    /// `m_parent[i].store( value )`.
    fn store(&mut self, index: u32, value: u32) -> Result<(), UnionFindError> {
        let slot = usize::try_from(index)
            .ok()
            .and_then(|i| self.parent.get_mut(i))
            .ok_or(UnionFindError::IndexOutOfRange)?;
        *slot = value;
        Ok(())
    }

    /// `m_parent[i].compare_exchange_strong( expected, desired )`: on a mismatch
    /// `expected` receives the current value.
    fn compare_exchange(
        &mut self,
        index: u32,
        expected: &mut u32,
        desired: u32,
    ) -> Result<bool, UnionFindError> {
        let current = self.load(index)?;

        if current == *expected {
            self.store(index, desired)?;
            Ok(true)
        } else {
            *expected = current;
            Ok(false)
        }
    }

    /// `Unite( aA, aB )`: merge the components holding `a` and `b`.
    ///
    /// Returns true if this call made the link.
    ///
    /// # Errors
    /// [`UnionFindError::IndexOutOfRange`] where the reference reads outside its
    /// vector; parents already shortened by the call stay shortened.
    pub fn unite(&mut self, a: u64, b: u64) -> Result<bool, UnionFindError> {
        // static_cast<uint32_t>: the low 32 bits.
        let mut u = a as u32;
        let mut v = b as u32;
        let mut pu = self.load(u)?;
        let mut pv = self.load(v)?;

        while pu != pv {
            // Work on the side with the larger parent so that a root always links
            // downwards.
            if pu < pv {
                core::mem::swap(&mut u, &mut v);
                core::mem::swap(&mut pu, &mut pv);
            }

            if u == pu {
                // u is a root and pv is below it, so this link is legal.
                if self.compare_exchange(u, &mut pu, pv)? {
                    self.components = self.components.wrapping_sub(1);
                    return Ok(true);
                }

                // The exchange reloaded pu with the value that won, so try again.
            } else {
                // Lift u one level and climb. A lost exchange costs only the shortcut,
                // so the result is ignored.
                let grandparent = self.load(pu)?;
                let mut expected = pu;

                self.compare_exchange(u, &mut expected, grandparent)?;

                u = pu;
                pu = self.load(u)?;
            }
        }

        Ok(false)
    }

    /// `Find( aX )`: the representative of the component holding `x`.
    ///
    /// # Errors
    /// [`UnionFindError::IndexOutOfRange`].
    pub fn find(&self, x: u64) -> Result<u64, UnionFindError> {
        let mut x = x as u32;
        let mut p = self.load(x)?;

        while p != x {
            x = p;
            p = self.load(x)?;
        }

        Ok(u64::from(x))
    }

    /// `FindCompress( aX )`: as [`Self::find`], pointing each visited node at its
    /// grandparent on the way.
    ///
    /// # Errors
    /// [`UnionFindError::IndexOutOfRange`].
    pub fn find_compress(&mut self, x: u64) -> Result<u64, UnionFindError> {
        let mut x = x as u32;
        let mut p = self.load(x)?;

        while p != x {
            let grandparent = self.load(p)?;

            self.store(x, grandparent)?;
            x = p;
            p = grandparent;
        }

        Ok(u64::from(x))
    }

    /// `Connected( aA, aB )`: `Find( aA ) == Find( aB )`.
    ///
    /// # Errors
    /// [`UnionFindError::IndexOutOfRange`].
    pub fn connected(&self, a: u64, b: u64) -> Result<bool, UnionFindError> {
        Ok(self.find(a)? == self.find(b)?)
    }

    /// The private parent array, for inspection.
    pub fn parents(&self) -> &[u32] {
        &self.parent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unite_links_downwards_and_counts() -> Result<(), UnionFindError> {
        let mut set = KiUnionFind::new(6)?;
        assert!(set.unite(5, 3)?);
        assert!(set.unite(4, 5)?);
        assert!(!set.unite(3, 4)?);
        assert_eq!(set.component_count(), 4);
        assert_eq!(set.find(4)?, 3);
        assert!(set.connected(5, 4)?);
        // Truncation to 32 bits: 2^32 + 1 is index 1.
        assert_eq!(set.find((1 << 32) + 1)?, 1);
        assert_eq!(set.find(6), Err(UnionFindError::IndexOutOfRange));
        assert_eq!(set.reset(1 << 32), Err(UnionFindError::LengthError));
        assert_eq!(set.size(), 6);
        Ok(())
    }
}
