// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
// Derived from Clipper2 2.0.1 clipper.core.h (Copyright Angus Johnson 2010-2025), Boost
// Software License 1.0; see LICENSE-BSL-1.0. Port: Embeint, GPL-3.0-or-later.

//! `clipper.core.h`: core structures and functions, in declaration order.
//!
//! The C++ templates are instantiated by KiCad for `int64_t` and `double`; [`Coord`] models
//! those two coordinate types, including the exact C++ conversions between them.

use crate::stream::{endl, insert_f64, insert_i64, insert_text};
use crate::{Clipper2Exception, ClipperError, Result};
use kicad_math::StreamState;

// --- error reporting ---------------------------------------------------------------------

/// `precision_error`.
pub const PRECISION_ERROR: &str = "Precision exceeds the permitted range";
/// `range_error`.
pub const RANGE_ERROR: &str = "Values exceed permitted range";
/// `scale_error`.
pub const SCALE_ERROR: &str = "Invalid scale (either 0 or too large)";
/// `non_pair_error`.
pub const NON_PAIR_ERROR: &str = "There must be 2 values for each coordinate";
/// `undefined_error`.
pub const UNDEFINED_ERROR: &str = "There is an undefined error in Clipper2";

/// `precision_error_i` (non-fatal).
pub const PRECISION_ERROR_I: i32 = 1;
/// `scale_error_i` (non-fatal).
pub const SCALE_ERROR_I: i32 = 2;
/// `non_pair_error_i` (non-fatal).
pub const NON_PAIR_ERROR_I: i32 = 4;
/// `undefined_error_i` (fatal).
pub const UNDEFINED_ERROR_I: i32 = 32;
/// `range_error_i`.
pub const RANGE_ERROR_I: i32 = 64;

/// `PI = 3.141592653589793238` (KiCad's build does not define its own `PI` before this
/// header); that literal rounds to the same double as `f64::consts::PI`.
pub const PI: f64 = ::core::f64::consts::PI;

/// `CLIPPER2_MAX_DEC_PRECISION`: `CLIPPER2_MAX_DECIMAL_PRECISION` is not defined by KiCad.
pub const CLIPPER2_MAX_DEC_PRECISION: i32 = 8;

/// `MAX_COORD = INT64_MAX >> 2`.
pub const MAX_COORD: i64 = i64::MAX >> 2;
/// `MIN_COORD = -MAX_COORD`.
pub const MIN_COORD: i64 = -MAX_COORD;
/// `INVALID = INT64_MAX`.
pub const INVALID: i64 = i64::MAX;
/// `max_coord = static_cast<double>(MAX_COORD)` (rounds to 2^61).
pub const MAX_COORD_D: f64 = MAX_COORD as f64;
/// `min_coord = static_cast<double>(MIN_COORD)`.
pub const MIN_COORD_D: f64 = MIN_COORD as f64;
/// `MAX_DBL = numeric_limits<double>::max()`.
pub const MAX_DBL: f64 = f64::MAX;

/// `DoError(error_code)`. KiCad compiles Clipper2 with exceptions, so this always throws;
/// the result is the exception to propagate. The exception-free branch (`++error_code`) is
/// not compiled in that configuration.
pub fn do_error(error_code: i32) -> ClipperError {
    let description = match error_code {
        PRECISION_ERROR_I => PRECISION_ERROR,
        SCALE_ERROR_I => SCALE_ERROR,
        NON_PAIR_ERROR_I => NON_PAIR_ERROR,
        UNDEFINED_ERROR_I => UNDEFINED_ERROR,
        RANGE_ERROR_I => RANGE_ERROR,
        // "Should never happen, but adding this to stop a compiler warning"
        _ => "Unknown error",
    };
    ClipperError::Exception(Clipper2Exception::new(description))
}

/// `FillRule`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    EvenOdd,
    NonZero,
    Positive,
    Negative,
}

/// `z_type` (`USINGZ`).
pub type ZType = i64;

// --- coordinate types --------------------------------------------------------------------

/// A coordinate value tagged with its C++ type, for mixed-type conversions and comparisons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    I64(i64),
    F64(f64),
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for i64 {}
    impl Sealed for f64 {}
}

/// 2^63 as a double: the exclusive upper bound of a defined `double` to `int64_t` conversion.
const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;
/// 2^127 as a double: the exclusive upper bound of a defined `double` to `__int128` conversion.
const TWO_POW_127: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0;

/// `static_cast<int64_t>(double)`: truncation, defined only when the truncated value fits.
pub fn cast_f64_to_i64(value: f64, what: &'static str) -> Result<i64> {
    // Every double in [-2^63, 2^63) truncates into range; NaN fails both comparisons.
    if (-TWO_POW_63..TWO_POW_63).contains(&value) {
        Ok(value as i64)
    } else {
        Err(ClipperError::Undefined(what))
    }
}

/// `static_cast<__int128_t>(double)`: truncation, defined only when the truncated value fits.
pub fn cast_f64_to_i128(value: f64, what: &'static str) -> Result<i128> {
    if (-TWO_POW_127..TWO_POW_127).contains(&value) {
        Ok(value as i128)
    } else {
        Err(ClipperError::Undefined(what))
    }
}

/// The coordinate types KiCad instantiates Clipper2 with: `int64_t` and `double`.
///
/// Arithmetic methods follow the C++ operators: `int64_t` arithmetic refuses signed
/// overflow (undefined behaviour), `double` arithmetic is IEEE binary64 in source order.
pub trait Coord:
    Copy + PartialOrd + PartialEq + Default + ::core::fmt::Debug + sealed::Sealed
{
    /// `std::is_integral_v<T>`.
    const IS_INTEGRAL: bool;
    /// `(std::numeric_limits<T>::max)()`.
    const MAX: Self;
    /// `std::numeric_limits<T>::lowest()`.
    const LOWEST: Self;
    /// `T(0)`.
    const ZERO: Self;

    /// The value with its C++ type.
    fn num(self) -> Num;
    /// `T + T`.
    fn add(self, other: Self) -> Result<Self>;
    /// `T - T`.
    fn sub(self, other: Self) -> Result<Self>;
    /// `T * T`.
    fn mul(self, other: Self) -> Result<Self>;
    /// Unary `-T`.
    fn neg(self) -> Result<Self>;
    /// `T / 2` (integer division truncates toward zero).
    fn halve(self) -> Self;
    /// `static_cast<double>(T)` (also the usual arithmetic conversion with a `double`).
    fn to_f64(self) -> f64;
    /// `static_cast<T>(int64_t)`.
    fn from_i64(value: i64) -> Self;
    /// `static_cast<T>(double)` (truncation for `int64_t`).
    fn from_f64_cast(value: f64) -> Result<Self>;
    /// `static_cast<__int128_t>(T)`.
    fn to_i128(self) -> Result<i128>;
    /// Implicit conversion of `T` to an `int64_t` parameter.
    fn to_i64(self) -> Result<i64>;
    /// `!val`.
    fn is_falsy(self) -> bool;
    /// `os << val`.
    fn insert(self, state: &mut StreamState) -> Result<String>;

    /// `static_cast<T>(s)` from another coordinate type.
    fn cast_from<S: Coord>(source: S) -> Result<Self> {
        match source.num() {
            Num::I64(value) => Ok(Self::from_i64(value)),
            Num::F64(value) => Self::from_f64_cast(value),
        }
    }

    /// `Point<T>::Init` conversion of one `T2` coordinate: `static_cast<T>(std::round(v))`
    /// when `T` is integral and `T2` is floating point, `static_cast<T>(v)` otherwise.
    fn init_from<S: Coord>(source: S) -> Result<Self> {
        match source.num() {
            Num::F64(value) if Self::IS_INTEGRAL => Self::from_f64_cast(round(value))
                .map_err(|_| ClipperError::Undefined("Point::Init std::round conversion")),
            _ => Self::cast_from(source),
        }
    }

    /// `GetClosestPointOnSegment` offset: `static_cast<T>(nearbyint(v))` for integral `T`,
    /// `static_cast<T>(v)` for floating `T`.
    fn closest_offset(value: f64) -> Result<Self> {
        if Self::IS_INTEGRAL {
            Self::from_f64_cast(nearbyint(value))
        } else {
            Self::from_f64_cast(value)
        }
    }
}

const OVERFLOW: &str = "int64_t overflow";
const CONVERSION: &str = "double to int64_t conversion out of range";

impl Coord for i64 {
    const IS_INTEGRAL: bool = true;
    const MAX: Self = i64::MAX;
    const LOWEST: Self = i64::MIN;
    const ZERO: Self = 0;

    fn num(self) -> Num {
        Num::I64(self)
    }
    fn add(self, other: Self) -> Result<Self> {
        self.checked_add(other)
            .ok_or(ClipperError::Undefined(OVERFLOW))
    }
    fn sub(self, other: Self) -> Result<Self> {
        self.checked_sub(other)
            .ok_or(ClipperError::Undefined(OVERFLOW))
    }
    fn mul(self, other: Self) -> Result<Self> {
        self.checked_mul(other)
            .ok_or(ClipperError::Undefined(OVERFLOW))
    }
    fn neg(self) -> Result<Self> {
        self.checked_neg().ok_or(ClipperError::Undefined(OVERFLOW))
    }
    fn halve(self) -> Self {
        // C++ integer division truncates toward zero, as Rust's does; /2 cannot overflow.
        self / 2
    }
    fn to_f64(self) -> f64 {
        self as f64
    }
    fn from_i64(value: i64) -> Self {
        value
    }
    fn from_f64_cast(value: f64) -> Result<Self> {
        cast_f64_to_i64(value, CONVERSION)
    }
    fn to_i128(self) -> Result<i128> {
        Ok(i128::from(self))
    }
    fn to_i64(self) -> Result<i64> {
        Ok(self)
    }
    fn is_falsy(self) -> bool {
        self == 0
    }
    fn insert(self, state: &mut StreamState) -> Result<String> {
        Ok(insert_i64(self, state)?)
    }
}

impl Coord for f64 {
    const IS_INTEGRAL: bool = false;
    const MAX: Self = f64::MAX;
    const LOWEST: Self = f64::MIN;
    const ZERO: Self = 0.0;

    fn num(self) -> Num {
        Num::F64(self)
    }
    fn add(self, other: Self) -> Result<Self> {
        Ok(self + other)
    }
    fn sub(self, other: Self) -> Result<Self> {
        Ok(self - other)
    }
    fn mul(self, other: Self) -> Result<Self> {
        Ok(self * other)
    }
    fn neg(self) -> Result<Self> {
        Ok(-self)
    }
    fn halve(self) -> Self {
        self / 2.0
    }
    fn to_f64(self) -> f64 {
        self
    }
    fn from_i64(value: i64) -> Self {
        value as f64
    }
    fn from_f64_cast(value: f64) -> Result<Self> {
        Ok(value)
    }
    fn to_i128(self) -> Result<i128> {
        cast_f64_to_i128(self, "double to __int128_t conversion out of range")
    }
    fn to_i64(self) -> Result<i64> {
        cast_f64_to_i64(self, CONVERSION)
    }
    fn is_falsy(self) -> bool {
        // `!val` on a double is `val == 0`; NaN is truthy.
        self == 0.0
    }
    fn insert(self, state: &mut StreamState) -> Result<String> {
        Ok(insert_f64(self, state)?)
    }
}

/// `std::round`: nearest integer, halfway cases away from zero (exact; no host libm).
pub fn round(value: f64) -> f64 {
    value.round()
}

/// `nearbyint` in the default round-to-nearest mode: halfway cases to even (exact).
pub fn nearbyint(value: f64) -> f64 {
    value.round_ties_even()
}

/// `a < b` after the usual arithmetic conversions (an `int64_t` meets a `double` as double).
pub fn mixed_lt<A: Coord, B: Coord>(a: A, b: B) -> bool {
    match (a.num(), b.num()) {
        (Num::I64(a), Num::I64(b)) => a < b,
        (Num::F64(a), Num::F64(b)) => a < b,
        (Num::I64(a), Num::F64(b)) => (a as f64) < b,
        (Num::F64(a), Num::I64(b)) => a < b as f64,
    }
}

/// `(std::max)(a, b)`: `(a < b) ? b : a`.
fn std_max<T: Coord>(a: T, b: T) -> T {
    if a < b {
        b
    } else {
        a
    }
}

/// `(std::min)(a, b)`: `(b < a) ? b : a`.
fn std_min<T: Coord>(a: T, b: T) -> T {
    if b < a {
        b
    } else {
        a
    }
}

// --- Point -------------------------------------------------------------------------------

/// `Point<T>` with `USINGZ`. [`Default`] is `explicit Point() : x(0), y(0), z(0)`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
    pub z: ZType,
}

/// `Point64`.
pub type Point64 = Point<i64>;
/// `PointD`.
pub type PointD = Point<f64>;
/// `Path<T>`.
pub type Path<T> = Vec<Point<T>>;
/// `Paths<T>`.
pub type Paths<T> = Vec<Path<T>>;
/// `Path64`.
pub type Path64 = Path<i64>;
/// `PathD`.
pub type PathD = Path<f64>;
/// `Paths64`.
pub type Paths64 = Paths<i64>;
/// `PathsD`.
pub type PathsD = Paths<f64>;

impl<T: Coord> Point<T> {
    /// `Point(const T x_, const T y_)` with the same coordinate type: no conversion, `z = 0`.
    pub fn xy(x: T, y: T) -> Self {
        Self { x, y, z: 0 }
    }

    /// `template <typename T2> void Init(const T2 x_, const T2 y_, const z_type z_)`.
    pub fn init<S: Coord>(&mut self, x: S, y: S, z: ZType) -> Result<()> {
        let x = T::init_from(x)?;
        let y = T::init_from(y)?;
        self.x = x;
        self.y = y;
        self.z = z;
        Ok(())
    }

    /// `template <typename T2> Point(const T2 x_, const T2 y_, const z_type z_ = 0)`:
    /// `Init(x_, y_)` then `z = z_`.
    pub fn new<S: Coord>(x: S, y: S, z: ZType) -> Result<Self> {
        let mut point = Self::default();
        point.init(x, y, 0)?;
        point.z = z;
        Ok(point)
    }

    /// `template <typename T2> explicit Point(const Point<T2>& p)`: `Init(p.x, p.y, p.z)`.
    pub fn from_point<S: Coord>(p: &Point<S>) -> Result<Self> {
        let mut point = Self::default();
        point.init(p.x, p.y, p.z)?;
        Ok(point)
    }

    /// `template <typename T2> explicit Point(const Point<T2>& p, z_type z_)`.
    pub fn from_point_with_z<S: Coord>(p: &Point<S>, z: ZType) -> Result<Self> {
        let mut point = Self::default();
        point.init(p.x, p.y, z)?;
        Ok(point)
    }

    /// `Point operator * (const double scale) const`: `Point(x * scale, y * scale, z)`.
    pub fn scaled(&self, scale: f64) -> Result<Self> {
        Self::new(self.x.to_f64() * scale, self.y.to_f64() * scale, self.z)
    }

    /// `void SetZ(const z_type z_value)`.
    pub fn set_z(&mut self, z_value: ZType) {
        self.z = z_value;
    }

    /// `friend bool operator==`: compares `x` and `y` only.
    pub fn point_eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }

    /// `friend bool operator!=`: `!(a == b)`.
    pub fn point_ne(&self, other: &Self) -> bool {
        !self.point_eq(other)
    }

    /// `Point<T> operator-() const`: `Point<T>(-x, -y)`, so `z` becomes 0.
    pub fn negated(&self) -> Result<Self> {
        Ok(Self::xy(self.x.neg()?, self.y.neg()?))
    }

    /// `Point operator+(const Point& b) const`: `Point(x + b.x, y + b.y)`, `z = 0`.
    pub fn plus(&self, b: &Self) -> Result<Self> {
        Ok(Self::xy(self.x.add(b.x)?, self.y.add(b.y)?))
    }

    /// `Point operator-(const Point& b) const`: `Point(x - b.x, y - b.y)`, `z = 0`.
    pub fn minus(&self, b: &Self) -> Result<Self> {
        Ok(Self::xy(self.x.sub(b.x)?, self.y.sub(b.y)?))
    }

    /// `void Negate()`: `x = -x; y = -y;`, `z` unchanged.
    pub fn negate(&mut self) -> Result<()> {
        let x = self.x.neg()?;
        let y = self.y.neg()?;
        self.x = x;
        self.y = y;
        Ok(())
    }

    /// `friend std::ostream& operator<<(std::ostream&, const Point&)`: `x,y,z`.
    pub fn insert(&self, state: &mut StreamState) -> Result<String> {
        let mut text = self.x.insert(state)?;
        text += &insert_text(",", state)?;
        text += &self.y.insert(state)?;
        text += &insert_text(",", state)?;
        text += &insert_i64(self.z, state)?;
        Ok(text)
    }
}

impl<T: Coord> PartialEq for Point<T> {
    /// The C++ `operator==`, which ignores `z`.
    fn eq(&self, other: &Self) -> bool {
        self.point_eq(other)
    }
}

/// `template <typename T, typename T2=T> Path<T>& operator<<(Path<T>& poly, const Point<T2>& p)`:
/// `poly.emplace_back(p)` through the explicit converting constructor.
pub fn append_point<T: Coord, S: Coord>(poly: &mut Path<T>, p: &Point<S>) -> Result<()> {
    let point = Point::<T>::from_point(p)?;
    poly.push(point);
    Ok(())
}

/// `template <typename T> Paths<T>& operator<<(Paths<T>& polys, const Path<T>& p)`.
pub fn append_path<T: Coord>(polys: &mut Paths<T>, p: &Path<T>) {
    polys.push(p.clone());
}

/// `InvalidPoint64`.
pub const INVALID_POINT64: Point64 = Point {
    x: i64::MAX,
    y: i64::MAX,
    z: 0,
};
/// `InvalidPointD`.
pub const INVALID_POINTD: PointD = Point {
    x: f64::MAX,
    y: f64::MAX,
    z: 0,
};

/// `template<typename T> static inline Point<T> MidPoint(const Point<T>&, const Point<T>&)`:
/// `result.x = (p1.x + p2.x) / 2` on a default point, so `z = 0`.
pub fn mid_point<T: Coord>(p1: &Point<T>, p2: &Point<T>) -> Result<Point<T>> {
    // `Point<T> result;` is the zero point; only x and y are assigned.
    Ok(Point {
        x: p1.x.add(p2.x)?.halve(),
        y: p1.y.add(p2.y)?.halve(),
        z: 0,
    })
}

// --- Rect --------------------------------------------------------------------------------

/// `Rect<T>`.
#[derive(Clone, Copy, Debug)]
pub struct Rect<T> {
    pub left: T,
    pub top: T,
    pub right: T,
    pub bottom: T,
}

/// `Rect64`.
pub type Rect64 = Rect<i64>;
/// `RectD`.
pub type RectD = Rect<f64>;

impl<T: Coord> Default for Rect<T> {
    /// `Rect(bool is_valid = true)` with its default argument: all zero.
    fn default() -> Self {
        Self::with_validity(true)
    }
}

impl<T: Coord> Rect<T> {
    /// `Rect(T l, T t, T r, T b)`.
    pub fn new(left: T, top: T, right: T, bottom: T) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// `Rect(bool is_valid)`: zeros, or `left = top = max()` and `right = bottom = lowest()`.
    pub fn with_validity(is_valid: bool) -> Self {
        if is_valid {
            Self::new(T::ZERO, T::ZERO, T::ZERO, T::ZERO)
        } else {
            Self::new(T::MAX, T::MAX, T::LOWEST, T::LOWEST)
        }
    }

    /// `static Rect<T> InvalidRect()`.
    pub fn invalid_rect() -> Self {
        Self::new(T::MAX, T::MAX, T::LOWEST, T::LOWEST)
    }

    /// `bool IsValid() const`: `left != max()`.
    pub fn is_valid(&self) -> bool {
        self.left != T::MAX
    }

    /// `T Width() const`.
    pub fn width(&self) -> Result<T> {
        self.right.sub(self.left)
    }

    /// `T Height() const`.
    pub fn height(&self) -> Result<T> {
        self.bottom.sub(self.top)
    }

    /// `void Width(T width)`: `right = left + width`.
    pub fn set_width(&mut self, width: T) -> Result<()> {
        self.right = self.left.add(width)?;
        Ok(())
    }

    /// `void Height(T height)`: `bottom = top + height`.
    pub fn set_height(&mut self, height: T) -> Result<()> {
        self.bottom = self.top.add(height)?;
        Ok(())
    }

    /// `Point<T> MidPoint() const`: `Point<T>((left + right) / 2, (top + bottom) / 2)`.
    pub fn mid_point(&self) -> Result<Point<T>> {
        let x = self.left.add(self.right)?.halve();
        let y = self.top.add(self.bottom)?.halve();
        Ok(Point::xy(x, y))
    }

    /// `Path<T> AsPath() const`: left-top, right-top, right-bottom, left-bottom.
    pub fn as_path(&self) -> Path<T> {
        vec![
            Point::xy(self.left, self.top),
            Point::xy(self.right, self.top),
            Point::xy(self.right, self.bottom),
            Point::xy(self.left, self.bottom),
        ]
    }

    /// `bool Contains(const Point<T>& pt) const` (strictly inside).
    pub fn contains_point(&self, pt: &Point<T>) -> bool {
        pt.x > self.left && pt.x < self.right && pt.y > self.top && pt.y < self.bottom
    }

    /// `bool Contains(const Rect<T>& rec) const` (inclusive).
    pub fn contains_rect(&self, rec: &Self) -> bool {
        rec.left >= self.left
            && rec.right <= self.right
            && rec.top >= self.top
            && rec.bottom <= self.bottom
    }

    /// `void Scale(double scale)`: `left *= scale` and so on; for `int64_t` each product is
    /// converted back by truncation.
    pub fn scale(&mut self, scale: f64) -> Result<()> {
        let left = T::from_f64_cast(self.left.to_f64() * scale)?;
        let top = T::from_f64_cast(self.top.to_f64() * scale)?;
        let right = T::from_f64_cast(self.right.to_f64() * scale)?;
        let bottom = T::from_f64_cast(self.bottom.to_f64() * scale)?;
        *self = Self::new(left, top, right, bottom);
        Ok(())
    }

    /// `bool IsEmpty() const`: `bottom <= top || right <= left`.
    pub fn is_empty(&self) -> bool {
        self.bottom <= self.top || self.right <= self.left
    }

    /// `bool Intersects(const Rect<T>& rec) const`.
    pub fn intersects(&self, rec: &Self) -> bool {
        std_max(self.left, rec.left) <= std_min(self.right, rec.right)
            && std_max(self.top, rec.top) <= std_min(self.bottom, rec.bottom)
    }

    /// `bool operator==(const Rect<T>& other) const`.
    pub fn rect_eq(&self, other: &Self) -> bool {
        self.left == other.left
            && self.right == other.right
            && self.top == other.top
            && self.bottom == other.bottom
    }

    /// `Rect<T>& operator+=(const Rect<T>& other)`.
    pub fn add_assign(&mut self, other: &Self) {
        self.left = std_min(self.left, other.left);
        self.top = std_min(self.top, other.top);
        self.right = std_max(self.right, other.right);
        self.bottom = std_max(self.bottom, other.bottom);
    }

    /// `Rect<T> operator+(const Rect<T>& other) const`.
    pub fn plus(&self, other: &Self) -> Self {
        let mut result = *self;
        result.add_assign(other);
        result
    }

    /// `friend std::ostream& operator<<(std::ostream&, const Rect<T>&)`: `(l,t,r,b) `.
    pub fn insert(&self, state: &mut StreamState) -> Result<String> {
        let mut text = insert_text("(", state)?;
        text += &self.left.insert(state)?;
        text += &insert_text(",", state)?;
        text += &self.top.insert(state)?;
        text += &insert_text(",", state)?;
        text += &self.right.insert(state)?;
        text += &insert_text(",", state)?;
        text += &self.bottom.insert(state)?;
        text += &insert_text(") ", state)?;
        Ok(text)
    }
}

impl<T: Coord> PartialEq for Rect<T> {
    fn eq(&self, other: &Self) -> bool {
        self.rect_eq(other)
    }
}

/// `template <typename T1, typename T2> Rect<T1> ScaleRect(const Rect<T2>& rect, double scale)`.
pub fn scale_rect<T1: Coord, T2: Coord>(rect: &Rect<T2>, scale: f64) -> Result<Rect<T1>> {
    let convert = |value: T2| -> Result<T1> {
        let product = value.to_f64() * scale;
        if T1::IS_INTEGRAL && !T2::IS_INTEGRAL {
            T1::from_f64_cast(round(product))
        } else {
            T1::from_f64_cast(product)
        }
    };
    // `Rect<T1> result;` (all zero), then each field is assigned in order.
    Ok(Rect {
        left: convert(rect.left)?,
        top: convert(rect.top)?,
        right: convert(rect.right)?,
        bottom: convert(rect.bottom)?,
    })
}

/// `InvalidRect64`.
pub fn invalid_rect64() -> Rect64 {
    Rect64::invalid_rect()
}

/// `InvalidRectD`.
pub fn invalid_rect_d() -> RectD {
    RectD::invalid_rect()
}

/// One step of the `GetBounds` loops: `if (p.x < xmin) xmin = static_cast<T>(p.x)` and so on.
fn bounds_step<T: Coord, S: Coord>(bounds: &mut [T; 4], p: &Point<S>) -> Result<()> {
    if mixed_lt(p.x, bounds[0]) {
        bounds[0] = T::cast_from(p.x)?;
    }
    if mixed_lt(bounds[2], p.x) {
        bounds[2] = T::cast_from(p.x)?;
    }
    if mixed_lt(p.y, bounds[1]) {
        bounds[1] = T::cast_from(p.y)?;
    }
    if mixed_lt(bounds[3], p.y) {
        bounds[3] = T::cast_from(p.y)?;
    }
    Ok(())
}

/// `template <typename T, typename T2> Rect<T> GetBounds(const Path<T2>& path)`; with
/// `T2 = T` this is also `template <typename T> Rect<T> GetBounds(const Path<T>& path)`.
pub fn get_bounds_path<T: Coord, S: Coord>(path: &[Point<S>]) -> Result<Rect<T>> {
    let mut bounds = [T::MAX, T::MAX, T::LOWEST, T::LOWEST];
    for p in path {
        bounds_step(&mut bounds, p)?;
    }
    Ok(Rect::new(bounds[0], bounds[1], bounds[2], bounds[3]))
}

/// `template <typename T, typename T2> Rect<T> GetBounds(const Paths<T2>& paths)` (and the
/// same-type overload).
pub fn get_bounds_paths<T: Coord, S: Coord>(paths: &[Path<S>]) -> Result<Rect<T>> {
    let mut bounds = [T::MAX, T::MAX, T::LOWEST, T::LOWEST];
    for path in paths {
        for p in path {
            bounds_step(&mut bounds, p)?;
        }
    }
    Ok(Rect::new(bounds[0], bounds[1], bounds[2], bounds[3]))
}

/// `template <typename T> std::ostream& operator<<(std::ostream&, const Path<T>&)`:
/// `p0, p1, ..., pn` followed by `std::endl`; nothing for an empty path.
pub fn insert_path<T: Coord>(path: &[Point<T>], state: &mut StreamState) -> Result<String> {
    let mut text = String::new();
    if let Some((last, rest)) = path.split_last() {
        for pt in rest {
            text += &pt.insert(state)?;
            text += &insert_text(", ", state)?;
        }
        text += &last.insert(state)?;
        text += endl();
    }
    Ok(text)
}

/// `template <typename T> std::ostream& operator<<(std::ostream&, const Paths<T>&)`.
pub fn insert_paths<T: Coord>(paths: &[Path<T>], state: &mut StreamState) -> Result<String> {
    let mut text = String::new();
    for p in paths {
        text += &insert_path(p, state)?;
    }
    Ok(text)
}

/// `template <typename T1, typename T2> Path<T1> ScalePath(const Path<T2>& path,
/// double scale_x, double scale_y, int& error_code)`.
///
/// A zero scale sets `scale_error_i` in `error_code` and throws (`DoError`). The following
/// replacement of a zero scale by 1.0 only runs without exceptions, which KiCad does not use.
pub fn scale_path_xy<T1: Coord, T2: Coord>(
    path: &[Point<T2>],
    scale_x: f64,
    scale_y: f64,
    error_code: &mut i32,
) -> Result<Path<T1>> {
    if scale_x == 0.0 || scale_y == 0.0 {
        *error_code |= SCALE_ERROR_I;
        return Err(do_error(SCALE_ERROR_I));
    }
    let mut result = Vec::with_capacity(path.len());
    for pt in path {
        result.push(Point::<T1>::new(
            pt.x.to_f64() * scale_x,
            pt.y.to_f64() * scale_y,
            pt.z,
        )?);
    }
    Ok(result)
}

/// `ScalePath(const Path<T2>& path, double scale, int& error_code)`.
pub fn scale_path<T1: Coord, T2: Coord>(
    path: &[Point<T2>],
    scale: f64,
    error_code: &mut i32,
) -> Result<Path<T1>> {
    scale_path_xy::<T1, T2>(path, scale, scale, error_code)
}

/// `template <typename T1, typename T2> Paths<T1> ScalePaths(const Paths<T2>& paths,
/// double scale_x, double scale_y, int& error_code)`.
///
/// For an integral target the scaled bounds must stay within `min_coord..max_coord`;
/// otherwise `range_error_i` is set and `DoError` throws.
pub fn scale_paths_xy<T1: Coord, T2: Coord>(
    paths: &[Path<T2>],
    scale_x: f64,
    scale_y: f64,
    error_code: &mut i32,
) -> Result<Paths<T1>> {
    if T1::IS_INTEGRAL {
        let r = get_bounds_paths::<f64, T2>(paths)?;
        if (r.left * scale_x) < MIN_COORD_D
            || (r.right * scale_x) > MAX_COORD_D
            || (r.top * scale_y) < MIN_COORD_D
            || (r.bottom * scale_y) > MAX_COORD_D
        {
            *error_code |= RANGE_ERROR_I;
            return Err(do_error(RANGE_ERROR_I));
        }
    }
    let mut result = Vec::with_capacity(paths.len());
    for path in paths {
        result.push(scale_path_xy::<T1, T2>(path, scale_x, scale_y, error_code)?);
    }
    Ok(result)
}

/// `ScalePaths(const Paths<T2>& paths, double scale, int& error_code)`.
pub fn scale_paths<T1: Coord, T2: Coord>(
    paths: &[Path<T2>],
    scale: f64,
    error_code: &mut i32,
) -> Result<Paths<T1>> {
    scale_paths_xy::<T1, T2>(paths, scale, scale, error_code)
}

/// `template <typename T1, typename T2> Path<T1> TransformPath(const Path<T2>& path)`.
pub fn transform_path<T1: Coord, T2: Coord>(path: &[Point<T2>]) -> Result<Path<T1>> {
    let mut result = Vec::with_capacity(path.len());
    for pt in path {
        result.push(Point::<T1>::from_point(pt)?);
    }
    Ok(result)
}

/// `template <typename T1, typename T2> Paths<T1> TransformPaths(const Paths<T2>& paths)`.
pub fn transform_paths<T1: Coord, T2: Coord>(paths: &[Path<T2>]) -> Result<Paths<T1>> {
    let mut result = Vec::new();
    for path in paths {
        result.push(transform_path::<T1, T2>(path)?);
    }
    Ok(result)
}

/// `template<typename T> double Sqr(T val)`.
pub fn sqr<T: Coord>(val: T) -> f64 {
    val.to_f64() * val.to_f64()
}

/// `template<typename T> bool NearEqual(const Point<T>&, const Point<T>&, double)`.
pub fn near_equal<T: Coord>(p1: &Point<T>, p2: &Point<T>, max_dist_sqrd: f64) -> Result<bool> {
    Ok(sqr(p1.x.sub(p2.x)?) + sqr(p1.y.sub(p2.y)?) < max_dist_sqrd)
}

/// `template<typename T> Path<T> StripNearEqual(const Path<T>&, double, bool)`.
pub fn strip_near_equal_path<T: Coord>(
    path: &[Point<T>],
    max_dist_sqrd: f64,
    is_closed_path: bool,
) -> Result<Path<T>> {
    let Some((&first_pt, rest)) = path.split_first() else {
        return Ok(Vec::new());
    };
    let mut result = Vec::with_capacity(path.len());
    let mut last_pt = first_pt;
    result.push(first_pt);
    for pt in rest {
        if !near_equal(pt, &last_pt, max_dist_sqrd)? {
            last_pt = *pt;
            result.push(last_pt);
        }
    }
    if !is_closed_path {
        return Ok(result);
    }
    while result.len() > 1 {
        let Some(back) = result.last() else { break };
        if !near_equal(back, &first_pt, max_dist_sqrd)? {
            break;
        }
        result.pop();
    }
    Ok(result)
}

/// `template<typename T> Paths<T> StripNearEqual(const Paths<T>&, double, bool)`.
pub fn strip_near_equal_paths<T: Coord>(
    paths: &[Path<T>],
    max_dist_sqrd: f64,
    is_closed_path: bool,
) -> Result<Paths<T>> {
    let mut result = Vec::with_capacity(paths.len());
    for path in paths {
        result.push(strip_near_equal_path(path, max_dist_sqrd, is_closed_path)?);
    }
    Ok(result)
}

/// `template<typename T> void StripDuplicates(Path<T>& path, bool is_closed_path)`:
/// `std::unique` (each kept point is compared with the next candidate by `operator==`, which
/// ignores `z`; the first point of a run is kept), then trailing points equal to the front
/// are removed from a closed path.
pub fn strip_duplicates_path<T: Coord>(path: &mut Path<T>, is_closed_path: bool) {
    let mut kept: Path<T> = Vec::with_capacity(path.len());
    for pt in path.iter() {
        match kept.last() {
            Some(last) if last.point_eq(pt) => {}
            _ => kept.push(*pt),
        }
    }
    *path = kept;
    if is_closed_path {
        while path.len() > 1 {
            match (path.last(), path.first()) {
                (Some(back), Some(front)) if back.point_eq(front) => {
                    path.pop();
                }
                _ => break,
            }
        }
    }
}

/// `template<typename T> void StripDuplicates(Paths<T>& paths, bool is_closed_path)`.
pub fn strip_duplicates_paths<T: Coord>(paths: &mut Paths<T>, is_closed_path: bool) {
    for path in paths.iter_mut() {
        strip_duplicates_path(path, is_closed_path);
    }
}

// --- Miscellaneous -----------------------------------------------------------------------

/// `void CheckPrecisionRange(int& precision, int& error_code)`.
///
/// Out of range, `precision_error_i` is set and `DoError` throws; the clamp that follows in
/// the C++ only runs without exceptions and is not reachable in KiCad's build.
pub fn check_precision_range(precision: &mut i32, error_code: &mut i32) -> Result<()> {
    if *precision >= -CLIPPER2_MAX_DEC_PRECISION && *precision <= CLIPPER2_MAX_DEC_PRECISION {
        return Ok(());
    }
    *error_code |= PRECISION_ERROR_I;
    Err(do_error(PRECISION_ERROR_I))
}

/// `void CheckPrecisionRange(int& precision)`.
pub fn check_precision_range_only(precision: &mut i32) -> Result<()> {
    let mut error_code = 0;
    check_precision_range(precision, &mut error_code)
}

/// `int TriSign(int64_t x)`: 0, 1 or -1.
pub fn tri_sign(x: i64) -> i32 {
    i32::from(x > 0) - i32::from(x < 0)
}

/// `UInt128Struct`: a 128-bit unsigned value as low and high words.
#[derive(Clone, Copy, Debug, Default)]
pub struct UInt128Struct {
    pub lo: u64,
    pub hi: u64,
}

impl UInt128Struct {
    /// `bool operator==(const UInt128Struct& other) const`.
    pub fn struct_eq(&self, other: &Self) -> bool {
        self.lo == other.lo && self.hi == other.hi
    }
}

impl PartialEq for UInt128Struct {
    fn eq(&self, other: &Self) -> bool {
        self.struct_eq(other)
    }
}

/// `UInt128Struct MultiplyUInt64(uint64_t a, uint64_t b)`: schoolbook 32-bit limbs, with
/// the reference's unsigned (wrapping) arithmetic.
pub fn multiply_u64(a: u64, b: u64) -> UInt128Struct {
    let lo = |x: u64| x & 0xFFFF_FFFF;
    let hi = |x: u64| x >> 32;
    let x1 = lo(a).wrapping_mul(lo(b));
    let x2 = hi(a).wrapping_mul(lo(b)).wrapping_add(hi(x1));
    let x3 = lo(a).wrapping_mul(hi(b)).wrapping_add(lo(x2));
    UInt128Struct {
        lo: (lo(x3) << 32) | lo(x1),
        hi: hi(a)
            .wrapping_mul(hi(b))
            .wrapping_add(hi(x2))
            .wrapping_add(hi(x3)),
    }
}

/// `bool ProductsAreEqual(int64_t a, int64_t b, int64_t c, int64_t d)`: the 64-bit
/// GCC/Clang `__int128_t` branch. Products of two `int64_t` always fit.
pub fn products_are_equal(a: i64, b: i64, c: i64, d: i64) -> bool {
    i128::from(a) * i128::from(b) == i128::from(c) * i128::from(d)
}

/// `std::abs(int64_t)` converted to `uint64_t`; `std::abs(INT64_MIN)` is undefined.
fn abs_u64(value: i64) -> Result<u64> {
    value
        .checked_abs()
        .map(i64::unsigned_abs)
        .ok_or(ClipperError::Undefined("std::abs(INT64_MIN)"))
}

/// `ProductsAreEqual`, the branch compiled without `__int128_t` (MSVC or 32-bit targets).
pub fn products_are_equal_without_int128(a: i64, b: i64, c: i64, d: i64) -> Result<bool> {
    let abs_a = abs_u64(a)?;
    let abs_b = abs_u64(b)?;
    let abs_c = abs_u64(c)?;
    let abs_d = abs_u64(d)?;
    let ab = multiply_u64(abs_a, abs_b);
    let cd = multiply_u64(abs_c, abs_d);
    // "it's important to differentiate 0 values here from other values"
    let sign_ab = tri_sign(a) * tri_sign(b);
    let sign_cd = tri_sign(c) * tri_sign(d);
    Ok(ab.struct_eq(&cd) && sign_ab == sign_cd)
}

/// `template <typename T> int CrossProductSign(pt1, pt2, pt3)`: the `__int128_t` branch.
/// For `double` the differences are truncated to `__int128_t` before multiplying.
pub fn cross_product_sign<T: Coord>(pt1: &Point<T>, pt2: &Point<T>, pt3: &Point<T>) -> Result<i32> {
    let a = pt2.x.sub(pt1.x)?;
    let b = pt3.y.sub(pt2.y)?;
    let c = pt2.y.sub(pt1.y)?;
    let d = pt3.x.sub(pt2.x)?;
    const PRODUCT: &str = "__int128_t overflow";
    let ab = a
        .to_i128()?
        .checked_mul(b.to_i128()?)
        .ok_or(ClipperError::Undefined(PRODUCT))?;
    let cd = c
        .to_i128()?
        .checked_mul(d.to_i128()?)
        .ok_or(ClipperError::Undefined(PRODUCT))?;
    Ok(if ab > cd {
        1
    } else if ab < cd {
        -1
    } else {
        0
    })
}

/// `CrossProductSign<int64_t>`, the branch compiled without `__int128_t`.
pub fn cross_product_sign_without_int128(
    pt1: &Point64,
    pt2: &Point64,
    pt3: &Point64,
) -> Result<i32> {
    let a = pt2.x.sub(pt1.x)?;
    let b = pt3.y.sub(pt2.y)?;
    let c = pt2.y.sub(pt1.y)?;
    let d = pt3.x.sub(pt2.x)?;
    let ab = multiply_u64(abs_u64(a)?, abs_u64(b)?);
    let cd = multiply_u64(abs_u64(c)?, abs_u64(d)?);
    let sign_ab = tri_sign(a) * tri_sign(b);
    let sign_cd = tri_sign(c) * tri_sign(d);
    if sign_ab == sign_cd {
        let result = if ab.hi == cd.hi {
            if ab.lo == cd.lo {
                return Ok(0);
            }
            if ab.lo > cd.lo {
                1
            } else {
                -1
            }
        } else if ab.hi > cd.hi {
            1
        } else {
            -1
        };
        return Ok(if sign_ab > 0 { result } else { -result });
    }
    Ok(if sign_ab > sign_cd { 1 } else { -1 })
}

/// `template <typename T> bool IsCollinear(pt1, sharedPt, pt2)`: `ProductsAreEqual` on the
/// differences, which convert to its `int64_t` parameters.
pub fn is_collinear<T: Coord>(
    pt1: &Point<T>,
    shared_pt: &Point<T>,
    pt2: &Point<T>,
) -> Result<bool> {
    let a = shared_pt.x.sub(pt1.x)?;
    let b = pt2.y.sub(shared_pt.y)?;
    let c = shared_pt.y.sub(pt1.y)?;
    let d = pt2.x.sub(shared_pt.x)?;
    Ok(products_are_equal(
        a.to_i64()?,
        b.to_i64()?,
        c.to_i64()?,
        d.to_i64()?,
    ))
}

/// `template <typename T> double CrossProduct(pt1, pt2, pt3)`.
pub fn cross_product3<T: Coord>(pt1: &Point<T>, pt2: &Point<T>, pt3: &Point<T>) -> Result<f64> {
    Ok(pt2.x.sub(pt1.x)?.to_f64() * pt3.y.sub(pt2.y)?.to_f64()
        - pt2.y.sub(pt1.y)?.to_f64() * pt3.x.sub(pt2.x)?.to_f64())
}

/// `template <typename T> double CrossProduct(const Point<T>& vec1, const Point<T>& vec2)`.
pub fn cross_product2<T: Coord>(vec1: &Point<T>, vec2: &Point<T>) -> Result<f64> {
    Ok(vec1.y.mul(vec2.x)?.to_f64() - vec2.y.mul(vec1.x)?.to_f64())
}

/// `template <typename T> double DotProduct(pt1, pt2, pt3)`.
pub fn dot_product3<T: Coord>(pt1: &Point<T>, pt2: &Point<T>, pt3: &Point<T>) -> Result<f64> {
    Ok(pt2.x.sub(pt1.x)?.to_f64() * pt3.x.sub(pt2.x)?.to_f64()
        + pt2.y.sub(pt1.y)?.to_f64() * pt3.y.sub(pt2.y)?.to_f64())
}

/// `template <typename T> double DotProduct(const Point<T>& vec1, const Point<T>& vec2)`.
pub fn dot_product2<T: Coord>(vec1: &Point<T>, vec2: &Point<T>) -> Result<f64> {
    Ok(vec1.x.mul(vec2.x)?.to_f64() + vec1.y.mul(vec2.y)?.to_f64())
}

/// `template <typename T> double DistanceSqr(const Point<T> pt1, const Point<T> pt2)`.
pub fn distance_sqr<T: Coord>(pt1: &Point<T>, pt2: &Point<T>) -> Result<f64> {
    Ok(sqr(pt1.x.sub(pt2.x)?) + sqr(pt1.y.sub(pt2.y)?))
}

/// `template <typename T> double PerpendicDistFromLineSqrd(pt, line1, line2)`.
pub fn perpendic_dist_from_line_sqrd<T: Coord>(
    pt: &Point<T>,
    line1: &Point<T>,
    line2: &Point<T>,
) -> Result<f64> {
    let a = pt.x.sub(line1.x)?.to_f64();
    let b = pt.y.sub(line1.y)?.to_f64();
    let c = line2.x.sub(line1.x)?.to_f64();
    let d = line2.y.sub(line1.y)?.to_f64();
    if c == 0.0 && d == 0.0 {
        return Ok(0.0);
    }
    Ok(sqr(a * d - c * b) / (c * c + d * d))
}

/// One `Area` term: `static_cast<double>(p->y + q->y) * (p->x - q->x)`.
fn area_term<T: Coord>(p: &Point<T>, q: &Point<T>) -> Result<f64> {
    Ok(p.y.add(q.y)?.to_f64() * p.x.sub(q.x)?.to_f64())
}

/// Bounds-checked element access for index arithmetic copied from iterator code.
fn at<T>(path: &[T], index: usize) -> Result<&T> {
    path.get(index)
        .ok_or(ClipperError::Undefined("iterator out of range"))
}

/// `template <typename T> double Area(const Path<T>& path)`: the shoelace sum taken two
/// edges per step, in the reference's iterator order.
pub fn area<T: Coord>(path: &[Point<T>]) -> Result<f64> {
    let cnt = path.len();
    if cnt < 3 {
        return Ok(0.0);
    }
    let mut a = 0.0;
    let mut it2 = cnt - 1;
    let stop = if cnt & 1 == 0 { cnt } else { cnt - 1 };
    let mut it1 = 0;
    while it1 != stop {
        a += area_term(at(path, it2)?, at(path, it1)?)?;
        it2 = it1 + 1;
        a += area_term(at(path, it1)?, at(path, it2)?)?;
        it1 += 2;
    }
    if cnt & 1 == 1 {
        a += area_term(at(path, it2)?, at(path, it1)?)?;
    }
    Ok(a * 0.5)
}

/// `template <typename T> double Area(const Paths<T>& paths)`.
pub fn area_paths<T: Coord>(paths: &[Path<T>]) -> Result<f64> {
    let mut a = 0.0;
    for path in paths {
        a += area(path)?;
    }
    Ok(a)
}

/// `template <typename T> bool IsPositive(const Path<T>& poly)`: `Area(poly) >= 0`.
pub fn is_positive<T: Coord>(poly: &[Point<T>]) -> Result<bool> {
    Ok(area(poly)? >= 0.0)
}

/// `template<typename T> bool GetLineIntersectPt(ln1a, ln1b, ln2a, ln2b, Point<T>& ip)`, the
/// default (not `CLIPPER2_HI_PRECISION`) branch. Returns `false` for parallel lines, leaving
/// `ip` unchanged; otherwise `ip` is constrained to segment 1 (a clamped end point keeps its
/// `z`; an interior point gets `z = 0`).
pub fn get_line_intersect_pt<T: Coord>(
    ln1a: &Point<T>,
    ln1b: &Point<T>,
    ln2a: &Point<T>,
    ln2b: &Point<T>,
    ip: &mut Point<T>,
) -> Result<bool> {
    let dx1 = ln1b.x.sub(ln1a.x)?.to_f64();
    let dy1 = ln1b.y.sub(ln1a.y)?.to_f64();
    let dx2 = ln2b.x.sub(ln2a.x)?.to_f64();
    let dy2 = ln2b.y.sub(ln2a.y)?.to_f64();
    let det = dy1 * dx2 - dy2 * dx1;
    if det == 0.0 {
        return Ok(false);
    }
    let t = (ln1a.x.sub(ln2a.x)?.to_f64() * dy2 - ln1a.y.sub(ln2a.y)?.to_f64() * dx2) / det;
    if t <= 0.0 {
        *ip = *ln1a;
    } else if t >= 1.0 {
        *ip = *ln1b;
    } else {
        let x = T::from_f64_cast(ln1a.x.to_f64() + t * dx1)?;
        let y = T::from_f64_cast(ln1a.y.to_f64() + t * dy1)?;
        ip.x = x;
        ip.y = y;
        ip.z = 0;
    }
    Ok(true)
}

/// `template<typename T> Point<T> TranslatePoint(const Point<T>& pt, double dx, double dy)`.
pub fn translate_point<T: Coord>(pt: &Point<T>, dx: f64, dy: f64) -> Result<Point<T>> {
    Point::new(pt.x.to_f64() + dx, pt.y.to_f64() + dy, pt.z)
}

/// `template<typename T> Point<T> ReflectPoint(const Point<T>& pt, const Point<T>& pivot)`.
pub fn reflect_point<T: Coord>(pt: &Point<T>, pivot: &Point<T>) -> Result<Point<T>> {
    let x = pivot.x.add(pivot.x.sub(pt.x)?)?;
    let y = pivot.y.add(pivot.y.sub(pt.y)?)?;
    Ok(Point { x, y, z: pt.z })
}

/// `template<typename T> int GetSign(const T& val)`: 0 when `!val`, else `val > 0 ? 1 : -1`
/// (so NaN gives -1).
pub fn get_sign<T: Coord>(val: T) -> i32 {
    if val.is_falsy() {
        return 0;
    }
    if val > T::ZERO {
        1
    } else {
        -1
    }
}

/// `bool SegmentsIntersect(seg1a, seg1b, seg2a, seg2b, bool inclusive = false)`.
pub fn segments_intersect(
    seg1a: &Point64,
    seg1b: &Point64,
    seg2a: &Point64,
    seg2b: &Point64,
    inclusive: bool,
) -> Result<bool> {
    let dy1 = seg1b.y.sub(seg1a.y)?.to_f64();
    let dx1 = seg1b.x.sub(seg1a.x)?.to_f64();
    let dy2 = seg2b.y.sub(seg2a.y)?.to_f64();
    let dx2 = seg2b.x.sub(seg2a.x)?.to_f64();
    let cp = dy1 * dx2 - dy2 * dx1;
    if cp == 0.0 {
        return Ok(false); // ie parallel segments
    }
    let ax = seg1a.x.sub(seg2a.x)?.to_f64();
    let ay = seg1a.y.sub(seg2a.y)?.to_f64();
    if inclusive {
        // result **includes** segments that touch at an end point
        let t = ax * dy2 - ay * dx2;
        if t == 0.0 {
            return Ok(true);
        }
        if t > 0.0 {
            if cp < 0.0 || t > cp {
                return Ok(false);
            }
        } else if cp > 0.0 || t < cp {
            return Ok(false);
        }
        let t = ax * dy1 - ay * dx1;
        if t == 0.0 {
            return Ok(true);
        }
        if t > 0.0 {
            Ok(cp > 0.0 && t <= cp)
        } else {
            Ok(cp < 0.0 && t >= cp)
        }
    } else {
        // result **excludes** segments that touch at an end point
        let t = ax * dy2 - ay * dx2;
        if t == 0.0 {
            return Ok(false);
        }
        if t > 0.0 {
            if cp < 0.0 || t >= cp {
                return Ok(false);
            }
        } else if cp > 0.0 || t <= cp {
            return Ok(false);
        }
        let t = ax * dy1 - ay * dx1;
        if t == 0.0 {
            return Ok(false);
        }
        if t > 0.0 {
            Ok(cp > 0.0 && t < cp)
        } else {
            Ok(cp < 0.0 && t > cp)
        }
    }
}

/// `template<typename T> Point<T> GetClosestPointOnSegment(offPt, seg1, seg2)`.
// The explicit comparisons keep the reference's handling of NaN and signed zero visible.
#[allow(clippy::manual_clamp)]
pub fn get_closest_point_on_segment<T: Coord>(
    off_pt: &Point<T>,
    seg1: &Point<T>,
    seg2: &Point<T>,
) -> Result<Point<T>> {
    if seg1.x == seg2.x && seg1.y == seg2.y {
        return Ok(*seg1);
    }
    let dx = seg2.x.sub(seg1.x)?.to_f64();
    let dy = seg2.y.sub(seg1.y)?.to_f64();
    let mut q = (off_pt.x.sub(seg1.x)?.to_f64() * dx + off_pt.y.sub(seg1.y)?.to_f64() * dy)
        / (sqr(dx) + sqr(dy));
    if q < 0.0 {
        q = 0.0;
    } else if q > 1.0 {
        q = 1.0;
    }
    let x = seg1.x.add(T::closest_offset(q * dx)?)?;
    let y = seg1.y.add(T::closest_offset(q * dy)?)?;
    Ok(Point::xy(x, y))
}

/// `PointInPolygonResult`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointInPolygonResult {
    IsOn,
    IsInside,
    IsOutside,
}

/// `template <typename T> PointInPolygonResult PointInPolygon(pt, polygon)`.
///
/// The reference walks iterators `first`, `curr`, `prev` and a movable `cend`; this port
/// keeps them as indices into `polygon` with identical moves.
pub fn point_in_polygon<T: Coord>(
    pt: &Point<T>,
    polygon: &[Point<T>],
) -> Result<PointInPolygonResult> {
    let n = polygon.len();
    if n < 3 {
        return Ok(PointInPolygonResult::IsOutside);
    }
    let mut val = 0;
    let cbegin = 0usize;
    let mut first = cbegin;
    let mut cend = n;
    while first != cend && at(polygon, first)?.y == pt.y {
        first += 1;
    }
    if first == cend {
        // not a proper polygon
        return Ok(PointInPolygonResult::IsOutside);
    }
    let mut is_above = at(polygon, first)?.y < pt.y;
    let starting_above = is_above;
    let mut curr = first + 1;
    loop {
        if curr == cend {
            if cend == first || first == cbegin {
                break;
            }
            cend = first;
            curr = cbegin;
        }
        if is_above {
            while curr != cend && at(polygon, curr)?.y < pt.y {
                curr += 1;
            }
            if curr == cend {
                continue;
            }
        } else {
            while curr != cend && at(polygon, curr)?.y > pt.y {
                curr += 1;
            }
            if curr == cend {
                continue;
            }
        }
        // nb: NOT cend (since might equal first)
        let prev = if curr == cbegin { n - 1 } else { curr - 1 };
        let c = *at(polygon, curr)?;
        let p = *at(polygon, prev)?;
        if c.y == pt.y {
            if c.x == pt.x || (c.y == p.y && ((pt.x < p.x) != (pt.x < c.x))) {
                return Ok(PointInPolygonResult::IsOn);
            }
            curr += 1;
            if curr == first {
                break;
            }
            continue;
        }
        if pt.x < c.x && pt.x < p.x {
            // we're only interested in edges crossing on the left
        } else if pt.x > p.x && pt.x > c.x {
            val = 1 - val; // toggle val
        } else {
            let d = cross_product_sign(&p, &c, pt)?;
            if d == 0 {
                return Ok(PointInPolygonResult::IsOn);
            }
            if (d < 0) == is_above {
                val = 1 - val;
            }
        }
        is_above = !is_above;
        curr += 1;
    }
    if is_above != starting_above {
        cend = n;
        if curr == cend {
            curr = cbegin;
        }
        let prev = if curr == cbegin { cend - 1 } else { curr - 1 };
        let d = cross_product_sign(at(polygon, prev)?, at(polygon, curr)?, pt)?;
        if d == 0 {
            return Ok(PointInPolygonResult::IsOn);
        }
        if (d < 0) == is_above {
            val = 1 - val;
        }
    }
    Ok(if val == 0 {
        PointInPolygonResult::IsOutside
    } else {
        PointInPolygonResult::IsInside
    })
}

#[cfg(test)]
mod tests;
