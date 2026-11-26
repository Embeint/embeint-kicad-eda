// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/include/core/minoptmax.h`: `MINOPTMAX<T>`, a constraint with an
//! optional minimum, optimum and maximum. KiCad instantiates it for `int` (the
//! default) and `long long`.

/// The operations `MINOPTMAX<T>` needs from `T`: copy, `operator<`, value
/// initialisation (`T{}`), the literal `0` and `std::numeric_limits<T>::max()`.
pub trait MinOptMaxValue: Copy + PartialOrd {
    /// `T{}`, the initial value of the stored members.
    fn value_initialized() -> Self;
    /// The literal `0` returned by `Min()` without a minimum.
    fn zero() -> Self;
    /// `std::numeric_limits<T>::max()`.
    fn max_value() -> Self;
}

macro_rules! integer_value {
    ($($type:ty),*) => {$(
        impl MinOptMaxValue for $type {
            fn value_initialized() -> Self {
                0
            }
            fn zero() -> Self {
                0
            }
            fn max_value() -> Self {
                <$type>::MAX
            }
        }
    )*};
}

integer_value!(i8, i16, i32, i64, u8, u16, u32, u64);

/// `std::max<T>( a, b )`: `( a < b ) ? b : a`.
fn std_max<T: PartialOrd>(a: T, b: T) -> T {
    if a < b {
        b
    } else {
        a
    }
}

/// `std::min<T>( a, b )`: `( b < a ) ? b : a`.
fn std_min<T: PartialOrd>(a: T, b: T) -> T {
    if b < a {
        b
    } else {
        a
    }
}

/// `MINOPTMAX<T>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinOptMax<T: MinOptMaxValue = i32> {
    is_null: bool,
    min: T,
    opt: T,
    max: T,
    has_min: bool,
    has_opt: bool,
    has_max: bool,
}

impl<T: MinOptMaxValue> Default for MinOptMax<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: MinOptMaxValue> MinOptMax<T> {
    /// The implicit default constructor: null, no values, members value-initialised.
    pub fn new() -> Self {
        Self {
            is_null: true,
            min: T::value_initialized(),
            opt: T::value_initialized(),
            max: T::value_initialized(),
            has_min: false,
            has_opt: false,
            has_max: false,
        }
    }

    /// `Min()`: the minimum, or `0` without one.
    pub fn min(&self) -> T {
        if self.has_min {
            self.min
        } else {
            T::zero()
        }
    }

    /// `Max()`: the maximum, or `std::numeric_limits<T>::max()` without one.
    pub fn max(&self) -> T {
        if self.has_max {
            self.max
        } else {
            T::max_value()
        }
    }

    /// `Opt()`: the optimum, or `Min()` without one.
    pub fn opt(&self) -> T {
        if self.has_opt {
            self.opt
        } else {
            self.min()
        }
    }

    /// `PinnedOpt()`: `std::max<T>( Min(), std::min<T>( Max(), Opt() ) )`.
    pub fn pinned_opt(&self) -> T {
        std_max(self.min(), std_min(self.max(), self.opt()))
    }

    /// `PinnedOpt( T aDefault )`: as [`Self::pinned_opt`], with `default` in place of a
    /// missing optimum.
    pub fn pinned_opt_or(&self, default: T) -> T {
        std_max(
            self.min(),
            std_min(self.max(), if self.has_opt { self.opt } else { default }),
        )
    }

    /// `HasMin()`.
    pub fn has_min(&self) -> bool {
        self.has_min
    }

    /// `HasMax()`.
    pub fn has_max(&self) -> bool {
        self.has_max
    }

    /// `HasOpt()`.
    pub fn has_opt(&self) -> bool {
        self.has_opt
    }

    /// `SetMin( v )`.
    pub fn set_min(&mut self, v: T) {
        self.is_null = false;
        self.min = v;
        self.has_min = true;
    }

    /// `SetMax( v )`.
    pub fn set_max(&mut self, v: T) {
        self.is_null = false;
        self.max = v;
        self.has_max = true;
    }

    /// `SetOpt( v )`.
    pub fn set_opt(&mut self, v: T) {
        self.is_null = false;
        self.opt = v;
        self.has_opt = true;
    }

    /// `IsNull()`: true until a value has been set.
    pub fn is_null(&self) -> bool {
        self.is_null
    }

    /// `Matches( v )`: `v` is not below a set minimum and not above a set maximum.
    pub fn matches(&self, v: T) -> bool {
        if self.has_min && v < self.min {
            return false;
        }
        if self.has_max && v > self.max {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_pinning() {
        let mut c: MinOptMax = MinOptMax::new();
        assert!(c.is_null());
        assert_eq!((c.min(), c.max(), c.opt()), (0, i32::MAX, 0));
        assert_eq!(c.pinned_opt_or(-5), 0);
        c.set_max(10);
        assert_eq!(c.pinned_opt_or(50), 10);
        c.set_min(20);
        // An inverted range pins to the minimum.
        assert_eq!(c.pinned_opt(), 20);
        assert!(!c.matches(15));
        assert!(!c.is_null());
    }
}
