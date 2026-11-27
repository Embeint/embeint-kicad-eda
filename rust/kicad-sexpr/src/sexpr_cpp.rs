// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2016 Mark Roszko <mark.roszko@gmail.com>
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! `libs/sexpr/sexpr.cpp` (unit `libs.sexpr.application.pure.p001`), in source order:
//!
//! - the two `SEXPR` constructors ([`DEFAULT_LINE_NUMBER`] and the stored line number);
//! - the `SEXPR` members `GetChildren`, `GetChild`, `AddChild`, `GetNumberOfChildren`,
//!   `GetString`, `GetInteger`, `GetLongInteger`, `GetDouble`, `GetFloat`, `GetSymbol`,
//!   `GetList` and `AsString` ([`SexprOps`], for every node type);
//! - `~SEXPR_LIST` (an iterative `Drop`);
//! - the nine `operator<<` and seven `operator>>` overloads (`SexprList::insert_*` and
//!   `SexprList::extract_*`);
//! - `SEXPR_LIST::doScan` and `SEXPR_LIST::doAddChildren`.
//!
//! Each `throw` becomes the matching [`SexprError`]; every undefined evaluation (an
//! out-of-range `m_children[]` access, a null child dereference, `doScan` writing a `double`
//! through an `int32_t*`, signed overflow of `m_inStreamChild`) is [`SexprError::Undefined`].

use crate::header::{
    ChildrenArg, ChildrenValue, InString, Isexprable, Node, OutString, ScanArg, ScanTarget, Sexpr,
    SexprDouble, SexprInteger, SexprList, SexprString, SexprSymbol, SexprVector, View, ViewMut,
};
use crate::{InvalidTypeException, SexprError};
use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use kicad_math::stream::FmtFlags;
use kicad_wx_stl_compat::ostream::{
    stock_do_put_long, InsertError, IosBase, NumPunct, OstreambufIterator, StreamBuf,
};

/// `SEXPR::SEXPR( SEXPR_TYPE aType )`: `m_lineNumber( 1 )`. (The other constructor,
/// `SEXPR( SEXPR_TYPE, size_t aLineNumber )`, stores its argument unchanged.)
pub(crate) const DEFAULT_LINE_NUMBER: u64 = 1;

/// `std::string::max_size()` of the reference's libstdc++ (GCC 12 and later) on LP64:
/// `min( PTRDIFF_MAX, allocator max_size() ) - 1`, both limits being `PTRDIFF_MAX`.
pub const STRING_MAX_SIZE: u64 = (1 << 63) - 2;

/// `what()` of the `std::length_error` that `std::string::append( n, c )` throws.
pub const APPEND_LENGTH_ERROR: &str = "basic_string::_M_replace_aux";

fn not_a(kind: &str) -> SexprError {
    SexprError::InvalidType(InvalidTypeException::new(
        format!("SEXPR is not a {kind} type!").as_bytes(),
    ))
}

/// `static_cast<float>( double )` as the x86-64 reference executes it (`cvtsd2ss`): IEEE
/// round to nearest even, overflow to infinity; a NaN keeps its sign and the top 22 payload
/// bits, with the quiet bit set. Rust's own `as` leaves the NaN payload unspecified (and
/// wasm's `f32.demote_f64` may canonicalise it), so NaNs are converted explicitly.
pub fn double_to_float(value: f64) -> f32 {
    if value.is_nan() {
        let bits = value.to_bits();
        let sign = ((bits >> 63) as u32) << 31;
        let payload = ((bits >> 29) & 0x003f_ffff) as u32;
        f32::from_bits(sign | 0x7fc0_0000 | payload)
    } else {
        value as f32
    }
}

/// The implicit `float` to `double` promotion as the reference executes it (`cvtss2sd`):
/// exact, except that a NaN keeps its sign and payload and gets its quiet bit set.
pub fn float_to_double(value: f32) -> f64 {
    if value.is_nan() {
        let bits = value.to_bits();
        let sign = u64::from(bits >> 31) << 63;
        let payload = u64::from(bits & 0x003f_ffff) << 29;
        f64::from_bits(sign | 0x7ff8_0000_0000_0000 | payload)
    } else {
        f64::from(value)
    }
}

/// `SEXPR`'s out-of-line members, available on every node type.
pub trait SexprOps: Node {
    /// `void AddChild( SEXPR* aChild )`. On a non-list the child is not added (the reference
    /// leaks it; it is dropped here, which has no observable effect).
    fn add_child(&mut self, child: Option<Box<Sexpr>>) -> Result<(), SexprError> {
        match self.view_mut() {
            ViewMut::List(list) => {
                list.children.push(child);
                Ok(())
            }
            _ => Err(not_a("list")),
        }
    }

    /// `SEXPR_VECTOR const* GetChildren() const`.
    fn get_children(&self) -> Result<&SexprVector, SexprError> {
        match self.view() {
            View::List(list) => Ok(&list.children),
            _ => Err(not_a("list")),
        }
    }

    /// `SEXPR* GetChild( size_t aIndex ) const`: the child (or null). An index past the last
    /// child is an undefined `operator[]`.
    fn get_child(&self, index: u64) -> Result<Option<&Sexpr>, SexprError> {
        match self.view() {
            View::List(list) => usize::try_from(index)
                .ok()
                .and_then(|index| list.children.get(index))
                .map(|child| child.as_deref())
                .ok_or(SexprError::Undefined),
            _ => Err(not_a("list")),
        }
    }

    /// `size_t GetNumberOfChildren() const`.
    fn get_number_of_children(&self) -> Result<u64, SexprError> {
        let count = self.get_children()?.len();
        u64::try_from(count).map_err(|_| SexprError::Undefined)
    }

    /// `std::string const& GetString() const`.
    fn get_string(&self) -> Result<&[u8], SexprError> {
        match self.view() {
            View::String(atom) => Ok(&atom.value),
            _ => Err(not_a("string")),
        }
    }

    /// `int32_t GetInteger() const`: `static_cast<int>( GetLongInteger() )`, modular.
    fn get_integer(&self) -> Result<i32, SexprError> {
        self.get_long_integer().map(|value| value as i32)
    }

    /// `int64_t GetLongInteger() const`.
    fn get_long_integer(&self) -> Result<i64, SexprError> {
        match self.view() {
            View::Integer(atom) => Ok(atom.value),
            _ => Err(not_a("integer")),
        }
    }

    /// `double GetDouble() const`: a double's value, or an integer's converted (round to
    /// nearest even).
    fn get_double(&self) -> Result<f64, SexprError> {
        match self.view() {
            View::Double(atom) => Ok(atom.value),
            View::Integer(atom) => Ok(atom.value as f64),
            _ => Err(not_a("double")),
        }
    }

    /// `float GetFloat() const`: `static_cast<float>( GetDouble() )`.
    fn get_float(&self) -> Result<f32, SexprError> {
        self.get_double().map(double_to_float)
    }

    /// `std::string const& GetSymbol() const`. The message names the node's line number.
    fn get_symbol(&self) -> Result<&[u8], SexprError> {
        match self.view() {
            View::Symbol(atom) => Ok(&atom.value),
            _ => Err(SexprError::InvalidType(InvalidTypeException::new(
                format!(
                    "GetSymbol(): SEXPR is not a symbol type! error line {}",
                    self.get_line_number()
                )
                .as_bytes(),
            ))),
        }
    }

    /// `SEXPR_LIST* GetList()`: this node as a list.
    fn get_list(&mut self) -> Result<&mut SexprList, SexprError> {
        match self.view_mut() {
            ViewMut::List(list) => Ok(list),
            _ => Err(not_a("list")),
        }
    }

    /// `std::string AsString( size_t aLevel = 0 ) const` (pass 0 for the default).
    fn as_string(&self, level: u64) -> Result<Vec<u8>, SexprError> {
        as_string(self.view(), level)
    }
}

impl<T: Node + ?Sized> SexprOps for T {}

/// Append with a fallible reservation (`std::bad_alloc` when it fails).
fn append(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), SexprError> {
    out.try_reserve(bytes.len())
        .map_err(|_| SexprError::BadAlloc)?;
    out.extend_from_slice(bytes);
    Ok(())
}

/// `std::stringstream out; out << GetInteger(); out.str()`: `operator<<( int )` on a fresh
/// stream (`skipws | dec`, width 0, the classic locale) through the accepted port of the
/// stock `num_put<char>::do_put( long )`. An exception from the buffer is caught by the
/// insertion (which only sets `badbit`), so the bytes written before it are the result.
fn integer_text(value: i32) -> Result<Vec<u8>, SexprError> {
    let mut buffer = StreamBuf::new(None, false).map_err(|_| SexprError::BadAlloc)?;
    let mut flags = FmtFlags {
        skipws: true,
        dec: true,
        ..FmtFlags::default()
    };
    let (mut width, mut precision) = (0i64, 6i64);
    let numpunct = NumPunct::default();
    let mut io = IosBase::new(&mut flags, &mut width, &mut precision, &numpunct);
    match stock_do_put_long(
        &mut buffer,
        OstreambufIterator::new(),
        &mut io,
        b' ',
        i64::from(value),
    ) {
        Ok(_) | Err(InsertError::Host(_)) => Ok(buffer.bytes().to_vec()),
        Err(_) => Err(SexprError::BadAlloc),
    }
}

/// `fmt::format( "{:.10g}", value )` (fmt 12.2.0): the shortest of fixed or exponential
/// notation with 10 significant digits, correctly rounded (ties to even), trailing zeros
/// removed; `nan`/`inf` keep the sign bit. Only whether exponential notation is chosen
/// matters to the caller, so exponential output is reported as `None`.
fn general_10(value: f64) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    if value.is_sign_negative() {
        out.push(b'-');
    }
    if value.is_nan() {
        out.extend_from_slice(b"nan");
        return Some(out);
    }
    let magnitude = value.abs();
    if magnitude.is_infinite() {
        out.extend_from_slice(b"inf");
        return Some(out);
    }
    if magnitude == 0.0 {
        out.push(b'0');
        return Some(out);
    }
    // Ten correctly rounded significant digits and the decimal exponent after rounding.
    let scientific = format!("{magnitude:.9e}");
    let (mantissa, exponent) = scientific.split_once('e')?;
    let exponent: i32 = exponent.parse().ok()?;
    if !(-4..10).contains(&exponent) {
        return None;
    }
    let mut digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    while digits.len() > 1 && digits.last() == Some(&b'0') {
        digits.pop();
    }
    if exponent >= 0 {
        let integer = usize::try_from(exponent).ok()? + 1;
        let available = digits.get(..integer.min(digits.len())).unwrap_or(&[]);
        out.extend_from_slice(available);
        out.resize(out.len() + integer.saturating_sub(available.len()), b'0');
        if let Some(fraction) = digits.get(integer..).filter(|f| !f.is_empty()) {
            out.push(b'.');
            out.extend_from_slice(fraction);
        }
    } else {
        out.extend_from_slice(b"0.");
        let zeros = usize::try_from(-exponent - 1).ok()?;
        out.resize(out.len() + zeros, b'0');
        out.extend_from_slice(&digits);
    }
    Some(out)
}

/// The `IsDouble()` branch of `AsString`: `{:.10g}`, or when that is exponential,
/// `{:.10f}` with trailing zeros and then a trailing point removed.
pub fn double_text(value: f64) -> Vec<u8> {
    if let Some(general) = general_10(value) {
        return general;
    }
    // Exponential notation is only chosen for finite nonzero values.
    let mut out = Vec::new();
    if value.is_sign_negative() {
        out.push(b'-');
    }
    out.extend_from_slice(format!("{:.10}", value.abs()).as_bytes());
    while out.last() == Some(&b'0') {
        out.pop();
    }
    if out.len() > 1 && out.last() == Some(&b'.') {
        out.pop();
    }
    out
}

/// The atom branches of `AsString`.
fn atom_text(node: View<'_>, out: &mut Vec<u8>) -> Result<(), SexprError> {
    match node {
        View::String(atom) => {
            append(out, b"\"")?;
            append(out, &atom.value)?;
            append(out, b"\"")
        }
        View::Symbol(atom) => append(out, &atom.value),
        View::Integer(atom) => append(out, &integer_text(atom.value as i32)?),
        View::Double(atom) => append(out, &double_text(atom.value)),
        View::List(_) => Ok(()),
    }
}

/// Open a list at `level`: `"\n"` unless the level is 0, `level * 2` spaces (modulo 2^64,
/// `size_t`) and `"("`. The append of the spaces throws `length_error` when they exceed
/// `max_size()` minus the frame's own `result` (empty or the newline).
fn open_list(level: u64, out: &mut Vec<u8>) -> Result<(), SexprError> {
    let local = if level != 0 {
        append(out, b"\n")?;
        1
    } else {
        0
    };
    let indent = level.wrapping_mul(2);
    if STRING_MAX_SIZE - local < indent {
        return Err(SexprError::LengthError(APPEND_LENGTH_ERROR));
    }
    let indent = usize::try_from(indent).map_err(|_| SexprError::BadAlloc)?;
    out.try_reserve(indent).map_err(|_| SexprError::BadAlloc)?;
    out.resize(out.len() + indent, b' ');
    append(out, b"(")
}

/// `SEXPR::AsString( size_t aLevel )`. The reference recurses into each child at
/// `aLevel + 1`, appending its text and a space after every child but the last; this writes
/// the same bytes in the same order with an explicit stack.
fn as_string(root: View<'_>, level: u64) -> Result<Vec<u8>, SexprError> {
    let mut out = Vec::new();
    let View::List(list) = root else {
        atom_text(root, &mut out)?;
        return Ok(out);
    };
    open_list(level, &mut out)?;
    let mut stack = Vec::new();
    stack.push((list.children.iter(), level.wrapping_add(1), true));
    while let Some((children, child_level, first)) = stack.last_mut() {
        let Some(child) = children.next() else {
            append(&mut out, b")")?;
            stack.pop();
            continue;
        };
        if !*first {
            append(&mut out, b" ")?;
        }
        *first = false;
        let child_level = *child_level;
        match child.as_deref().map(Node::view) {
            None => return Err(SexprError::Undefined),
            Some(View::List(inner)) => {
                open_list(child_level, &mut out)?;
                stack.push((inner.children.iter(), child_level.wrapping_add(1), true));
            }
            Some(atom) => atom_text(atom, &mut out)?,
        }
    }
    Ok(out)
}

/// `SEXPR_LIST::~SEXPR_LIST()`: delete every child, then clear the vector. Null children are
/// skipped (`delete nullptr` does nothing). The reference recurses through each child's
/// destructor; this drops the same nodes with an explicit stack, so a deep tree cannot
/// exhaust the native stack.
impl Drop for SexprList {
    fn drop(&mut self) {
        let mut pending: Vec<Box<Sexpr>> = self.children.drain(..).flatten().collect();
        while let Some(mut node) = pending.pop() {
            if let Sexpr::List(list) = &mut *node {
                pending.extend(list.children.drain(..).flatten());
            }
        }
    }
}

impl SexprList {
    /// `list.AddChild( child )` on a list, which cannot fail its type check.
    fn push(&mut self, child: Option<Box<Sexpr>>) -> &mut Self {
        self.children.push(child);
        self
    }

    /// `operator<<( SEXPR_LIST&, const ISEXPRABLE& )`: add `obj.SerializeSEXPR()`, which may be
    /// null.
    pub fn insert_isexprable(&mut self, obj: &dyn Isexprable) -> &mut Self {
        let child = obj.serialize_sexpr();
        self.push(child)
    }

    /// `operator<<( SEXPR_LIST&, int64_t )`.
    pub fn insert_i64(&mut self, value: i64) -> &mut Self {
        self.push(Some(Box::new(Sexpr::from(SexprInteger::new(value)))))
    }

    /// `operator<<( SEXPR_LIST&, int32_t )`.
    pub fn insert_i32(&mut self, value: i32) -> &mut Self {
        self.push(Some(Box::new(Sexpr::from(SexprInteger::new(i64::from(
            value,
        ))))))
    }

    /// `operator<<( SEXPR_LIST&, float )`: a double atom of the promoted value.
    pub fn insert_f32(&mut self, value: f32) -> &mut Self {
        self.push(Some(Box::new(Sexpr::from(SexprDouble::new(
            float_to_double(value),
        )))))
    }

    /// `operator<<( SEXPR_LIST&, double )`.
    pub fn insert_f64(&mut self, value: f64) -> &mut Self {
        self.push(Some(Box::new(Sexpr::from(SexprDouble::new(value)))))
    }

    /// `operator<<( SEXPR_LIST&, std::string )`: a string atom.
    pub fn insert_string(&mut self, value: Vec<u8>) -> &mut Self {
        self.push(Some(Box::new(Sexpr::from(SexprString::new(&value)))))
    }

    /// `operator<<( SEXPR_LIST&, SEXPR* )`: the node (or null) as it is.
    pub fn insert_sexpr(&mut self, obj: Option<Box<Sexpr>>) -> &mut Self {
        self.push(obj)
    }

    /// `operator<<( SEXPR_LIST&, const _OUT_STRING )`: a symbol or a string atom.
    pub fn insert_out_string(&mut self, setting: OutString<'_>) -> &mut Self {
        let atom = if setting.symbol {
            Sexpr::from(SexprSymbol::new(setting.string))
        } else {
            Sexpr::from(SexprString::new(setting.string))
        };
        self.push(Some(Box::new(atom)))
    }

    /// `operator<<( SEXPR_LIST&, SEXPR_LIST* )`: the list (or null) as a child.
    pub fn insert_list(&mut self, list2: Option<Box<SexprList>>) -> &mut Self {
        self.push(list2.map(|list| Box::new(Sexpr::List(*list))))
    }

    /// `operator>>( SEXPR_LIST&, ISEXPRABLE& )`: `obj.DeserializeSEXPR( input )`.
    pub fn extract_isexprable(&mut self, obj: &mut dyn Isexprable) -> &mut Self {
        obj.deserialize_sexpr(self);
        self
    }

    /// `input.GetChild( input.m_inStreamChild )`, dereferenced: undefined past the last
    /// child or for a null child.
    fn stream_child(&self) -> Result<&Sexpr, SexprError> {
        let index = u64::try_from(self.in_stream_child).map_err(|_| SexprError::Undefined)?;
        self.get_child(index)?.ok_or(SexprError::Undefined)
    }

    /// `input.m_inStreamChild++` (signed overflow is undefined).
    fn advance(&mut self) -> Result<&mut Self, SexprError> {
        self.in_stream_child = self
            .in_stream_child
            .checked_add(1)
            .ok_or(SexprError::Undefined)?;
        Ok(self)
    }

    /// `operator>>( SEXPR_LIST&, int32_t& )`.
    pub fn extract_i32(&mut self, inte: &mut i32) -> Result<&mut Self, SexprError> {
        let child = self.stream_child()?;
        if !child.is_integer() {
            return Err(SexprError::InvalidArgument("SEXPR is not a integer type!"));
        }
        *inte = child.get_integer()?;
        self.advance()
    }

    /// `operator>>( SEXPR_LIST&, std::string& )`. A symbol passes the type test but
    /// `GetString()` then throws `INVALID_TYPE_EXCEPTION`, leaving `str` and the position.
    pub fn extract_string(&mut self, str: &mut Vec<u8>) -> Result<&mut Self, SexprError> {
        let child = self.stream_child()?;
        if !(child.is_string() || child.is_symbol()) {
            return Err(SexprError::InvalidArgument("SEXPR is not a string type!"));
        }
        let value = child.get_string()?.to_vec();
        *str = value;
        self.advance()
    }

    /// `operator>>( SEXPR_LIST&, int64_t& )`.
    pub fn extract_i64(&mut self, lint: &mut i64) -> Result<&mut Self, SexprError> {
        let child = self.stream_child()?;
        if !child.is_integer() {
            return Err(SexprError::InvalidArgument(
                "SEXPR is not a long integer type!",
            ));
        }
        *lint = child.get_long_integer()?;
        self.advance()
    }

    /// `operator>>( SEXPR_LIST&, float& )`: doubles only (an integer is refused).
    pub fn extract_f32(&mut self, fl: &mut f32) -> Result<&mut Self, SexprError> {
        let child = self.stream_child()?;
        if !child.is_double() {
            return Err(SexprError::InvalidArgument("SEXPR is not a float type!"));
        }
        *fl = child.get_float()?;
        self.advance()
    }

    /// `operator>>( SEXPR_LIST&, double& )`: doubles only (an integer is refused).
    pub fn extract_f64(&mut self, dbl: &mut f64) -> Result<&mut Self, SexprError> {
        let child = self.stream_child()?;
        if !child.is_double() {
            return Err(SexprError::InvalidArgument("SEXPR is not a double type!"));
        }
        *dbl = child.get_double()?;
        self.advance()
    }

    /// `operator>>( SEXPR_LIST&, const _IN_STRING )`.
    pub fn extract_in_string(&mut self, is: InString<'_>) -> Result<&mut Self, SexprError> {
        let child = self.stream_child()?;
        let value = if is.symbol {
            if !child.is_symbol() {
                return Err(SexprError::InvalidArgument(
                    "operator>>: SEXPR is not a symbol type!",
                ));
            }
            child.get_symbol()?.to_vec()
        } else {
            if !child.is_string() {
                return Err(SexprError::InvalidArgument("SEXPR is not a string type!"));
            }
            child.get_string()?.to_vec()
        };
        *is.string = value;
        self.advance()
    }

    /// `size_t SEXPR_LIST::doScan( const SEXPR_SCAN_ARG* args, size_t num_args )`: read each
    /// argument from the child at its index; an `INVALID_TYPE_EXCEPTION` or a `STRING_COMP`
    /// mismatch returns that index, otherwise `num_args`. A `STRING` argument whose child is
    /// neither a symbol nor a string, and a `STRING_COMP` argument whose child is neither,
    /// are skipped. The `else` branch (an unknown argument type) cannot be constructed.
    pub(crate) fn do_scan(&mut self, args: &mut [ScanArg<'_>]) -> Result<u64, SexprError> {
        let mut i: u64 = 0;
        for arg in args.iter_mut() {
            let child = self.get_child(i)?.ok_or(SexprError::Undefined)?;
            let read = match &mut arg.target {
                ScanTarget::Double(value) => child.get_double().map(|v| **value = v),
                // `*arg.u.dbl_value = child->GetInteger()` stores a double through the
                // int32_t*: undefined once GetInteger() returns.
                ScanTarget::Int(_) => child.get_integer().and(Err(SexprError::Undefined)),
                ScanTarget::String(value) => {
                    if child.is_symbol() {
                        **value = child.get_symbol()?.to_vec();
                    } else if child.is_string() {
                        **value = child.get_string()?.to_vec();
                    }
                    Ok(())
                }
                ScanTarget::LongInt(value) => child.get_long_integer().map(|v| **value = v),
                ScanTarget::SexprString { symbol, string } => {
                    let value = if *symbol {
                        child.get_symbol()
                    } else {
                        child.get_string()
                    };
                    value.map(|v| **string = v.to_vec())
                }
                ScanTarget::StringComp(compared) => {
                    if child.is_symbol() {
                        if child.get_symbol()? != compared.as_slice() {
                            return Ok(i);
                        }
                    } else if child.is_string() && child.get_string()? != compared.as_slice() {
                        return Ok(i);
                    }
                    Ok(())
                }
            };
            match read {
                Ok(()) => {}
                Err(SexprError::InvalidType(_)) => return Ok(i),
                Err(error) => return Err(error),
            }
            i += 1;
        }
        Ok(i)
    }

    /// `void SEXPR_LIST::doAddChildren( const SEXPR_CHILDREN_ARG* args, size_t num_args )`:
    /// add one child per argument, in order. The `else` branch (an unknown argument type)
    /// cannot be constructed.
    pub(crate) fn do_add_children(&mut self, args: Vec<ChildrenArg>) -> Result<(), SexprError> {
        for arg in args {
            let child = match arg.value {
                ChildrenValue::Double(value) => {
                    Some(Box::new(Sexpr::from(SexprDouble::new(value))))
                }
                ChildrenValue::Int(value) => {
                    Some(Box::new(Sexpr::from(SexprInteger::new(i64::from(value)))))
                }
                ChildrenValue::LongInt(value) => {
                    Some(Box::new(Sexpr::from(SexprInteger::new(value))))
                }
                ChildrenValue::String(value) => {
                    Some(Box::new(Sexpr::from(SexprString::new(&value))))
                }
                ChildrenValue::SexprAtom(node) => node,
                ChildrenValue::SexprString { symbol, string } => Some(Box::new(if symbol {
                    Sexpr::from(SexprSymbol::new(&string))
                } else {
                    Sexpr::from(SexprString::new(&string))
                })),
            };
            self.add_child(child)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{as_string_in, as_symbol, DefaultIsexprable};
    use alloc::vec;

    fn text(value: f64) -> alloc::string::String {
        alloc::string::String::from_utf8(double_text(value)).unwrap_or_default()
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn doubles_format_as_the_reference() {
        for (value, expected) in [
            (3.14, "3.14"),
            (42.42, "42.42"),
            (0.0, "0"),
            (-0.0, "-0"),
            (1e10, "10000000000"),
            (9999999999.5, "9999999999.5"),
            (1e-5, "0.00001"),
            (1e-11, "0"),
            (-1e-11, "-0"),
            (0.0001, "0.0001"),
            (1234567890.5, "1234567890"),
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (f64::NAN, "nan"),
            (-f64::NAN, "-nan"),
            (1.0 / 3.0, "0.3333333333"),
        ] {
            assert_eq!(text(value), expected, "{value:e}");
        }
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn upstream_as_string_list_example() {
        // qa/tests/libs/sexpr/test_sexpr.cpp AsStringList.
        let mut list = SexprList::new();
        list.add_child(Some(Box::new(Sexpr::from(SexprSymbol::new(b"symbol")))))
            .ok();
        let mut sub = SexprList::new();
        sub.insert_i32(2)
            .insert_f64(42.42)
            .insert_string(b"substring".to_vec());
        list.insert_list(Some(Box::new(sub)));
        list.insert_i32(1)
            .insert_f64(3.14)
            .insert_string(b"string".to_vec());
        assert_eq!(list.get_number_of_children(), Ok(5));
        assert_eq!(
            list.as_string(0).ok().as_deref(),
            Some(&b"(symbol \n  (2 42.42 \"substring\") 1 3.14 \"string\")"[..])
        );
    }

    #[test]
    fn as_string_indentation_wraps_and_overflows_like_size_t() {
        let mut list = SexprList::new();
        list.insert_list(Some(Box::new(SexprList::new())));
        assert_eq!(
            list.as_string(1 << 63).ok().as_deref(),
            Some(&b"\n(\n  ())"[..])
        );
        assert_eq!(
            list.as_string(1 << 62),
            Err(SexprError::LengthError(APPEND_LENGTH_ERROR))
        );
        // At a nonzero level the newline already in the frame's result leaves one byte less:
        // 2 * (2^62 - 1) = max_size() exceeds max_size() - 1.
        assert_eq!(
            SexprList::new().as_string((1 << 62) - 1),
            Err(SexprError::LengthError(APPEND_LENGTH_ERROR))
        );
        assert_eq!(
            SexprInteger::new(1 << 32)
                .as_string(u64::MAX)
                .ok()
                .as_deref(),
            Some(&b"0"[..])
        );
    }

    #[test]
    fn typed_accessors_throw_the_reference_messages() {
        let atom = SexprInteger::with_line_number(5, 9);
        assert_eq!(atom.get_double(), Ok(5.0));
        let Err(SexprError::InvalidType(error)) = atom.get_symbol() else {
            return;
        };
        use crate::StdException;
        assert_eq!(
            error.what(),
            b"GetSymbol(): SEXPR is not a symbol type! error line 9"
        );
        assert_eq!(atom.get_children(), Err(not_a("list")));
    }

    #[test]
    fn stream_extraction_follows_the_reference() {
        let mut list = SexprList::new();
        list.insert_out_string(as_symbol(b"sym"))
            .insert_i64(1 << 40);
        let mut text = Vec::new();
        assert!(matches!(
            list.extract_string(&mut text),
            Err(SexprError::InvalidType(_))
        ));
        assert_eq!(list.in_stream_child(), 0);
        let mut target = Vec::new();
        assert!(list
            .extract_in_string(crate::header::as_symbol_in(&mut target))
            .is_ok());
        let mut small = 0;
        assert!(list.extract_i32(&mut small).is_ok());
        assert_eq!((small, target.as_slice()), (0, &b"sym"[..]));
        assert_eq!(
            list.extract_i32(&mut small).err(),
            Some(SexprError::Undefined)
        );
        let mut object = DefaultIsexprable;
        list.extract_isexprable(&mut object);
        let _ = as_string_in(&mut text);
    }

    #[test]
    fn scan_and_add_children() {
        let mut list = SexprList::from_args(vec![
            ChildrenArg::int(3),
            ChildrenArg::out_string(&as_symbol(b"k")),
            ChildrenArg::sexpr(None),
        ])
        .unwrap_or_default();
        assert_eq!(list.children.len(), 3);
        let mut value = 0.0;
        let mut args = [ScanArg::double(&mut value), ScanArg::const_string(b"k")];
        assert_eq!(list.scan(&mut args), Ok(2));
        assert_eq!(value, 3.0);
        let mut small = 0;
        assert_eq!(
            list.scan(&mut [ScanArg::int(&mut small)]),
            Err(SexprError::Undefined)
        );
        let mut long = 0;
        assert_eq!(list.scan(&mut [ScanArg::long_int(&mut long)]), Ok(1));
    }

    #[test]
    fn deep_trees_drop_and_serialise_without_recursion() {
        let chain = |depth: usize| {
            let mut root = SexprList::new();
            for _ in 0..depth {
                let mut parent = SexprList::new();
                parent.insert_list(Some(Box::new(root)));
                root = parent;
            }
            root
        };
        // depth + 1 lists: "(" and ")" each, and a newline and 2 * d spaces below the root.
        let depth = 3_000;
        let expected = 2 * (depth + 1) + depth + depth * (depth + 1);
        assert_eq!(
            chain(depth).as_string(0).map(|s| s.len()).ok(),
            Some(expected)
        );
        drop(chain(200_000));
    }

    #[test]
    fn float_conversions_keep_x86_nan_payloads() {
        let snan = f64::from_bits(0x7ff4_0000_e000_0000);
        assert_eq!(double_to_float(snan).to_bits(), 0x7fe0_0007);
        assert_eq!(
            float_to_double(f32::from_bits(0xff80_0001)).to_bits(),
            0xfff8_0000_2000_0000
        );
        assert_eq!(double_to_float(1e300), f32::INFINITY);
    }
}
