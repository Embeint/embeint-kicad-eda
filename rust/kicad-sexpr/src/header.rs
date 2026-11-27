// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2016 Mark Roszko <mark.roszko@gmail.com>
// Copyright The KiCad Developers, see AUTHORS.txt for contributors.

//! The `libs/sexpr/include/sexpr` headers (unit
//! `libs.sexpr.include.sexpr.application.pure.p001`).
//!
//! - `sexpr.h`: [`SexprType`], the node types [`SexprList`], [`SexprInteger`],
//!   [`SexprDouble`], [`SexprString`] and [`SexprSymbol`] with their inline constructors, the
//!   [`Node`] predicates and `GetLineNumber`, [`OutString`]/[`InString`] with
//!   [`as_symbol`]/[`as_string`] (and their non-const overloads [`as_symbol_in`]/
//!   [`as_string_in`]), [`ScanArg`] and [`ChildrenArg`] with every constructor, and the
//!   `SEXPR_LIST` constructors and `Scan`/`AddChildren` templates.
//! - `isexprable.h`: [`Isexprable`] with its two default virtual bodies.
//! - `sexpr_exception.h`: [`ParseException`] and [`InvalidTypeException`].
//! - `sexpr_parser.h`: declarations only (`SEXPR::PARSER`); no inline body to port here.
//!
//! The C++ class hierarchy (every node is a `SEXPR`) is the [`Node`] trait, implemented by
//! [`Sexpr`] (any node) and by each concrete node type. The out-of-line `SEXPR` member
//! functions of `sexpr.cpp` are [`crate::sexpr_cpp`].

use crate::sexpr_cpp::DEFAULT_LINE_NUMBER;
use crate::SexprError;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::CStr;

/// `SEXPR::SEXPR_TYPE` (`enum class SEXPR_TYPE : char`), in declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SexprType {
    /// `SEXPR_TYPE_LIST`.
    List,
    /// `SEXPR_TYPE_ATOM_INTEGER`.
    AtomInteger,
    /// `SEXPR_TYPE_ATOM_DOUBLE`.
    AtomDouble,
    /// `SEXPR_TYPE_ATOM_STRING`.
    AtomString,
    /// `SEXPR_TYPE_ATOM_SYMBOL`.
    AtomSymbol,
}

/// `SEXPR_VECTOR` (`std::vector<SEXPR*>`): owned children, any of which may be null.
pub type SexprVector = Vec<Option<Box<Sexpr>>>;

/// The `int aLineNumber` parameter of the derived constructors, passed to the `size_t`
/// parameter of `SEXPR( SEXPR_TYPE, size_t )`: the integral conversion is modular, so a
/// negative line number becomes `2^64 + aLineNumber`.
const fn line_from_int(line_number: i32) -> u64 {
    line_number as i64 as u64
}

/// `SEXPR_INTEGER`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SexprInteger {
    line_number: u64,
    /// `m_value`.
    pub value: i64,
}

impl SexprInteger {
    /// `SEXPR_INTEGER( int64_t aValue )`: line number 1.
    pub const fn new(value: i64) -> Self {
        Self {
            line_number: DEFAULT_LINE_NUMBER,
            value,
        }
    }

    /// `SEXPR_INTEGER( int64_t aValue, int aLineNumber )`.
    pub const fn with_line_number(value: i64, line_number: i32) -> Self {
        Self {
            line_number: line_from_int(line_number),
            value,
        }
    }
}

/// `SEXPR_DOUBLE`.
#[derive(Clone, Debug, PartialEq)]
pub struct SexprDouble {
    line_number: u64,
    /// `m_value`.
    pub value: f64,
}

impl SexprDouble {
    /// `SEXPR_DOUBLE( double aValue )`: line number 1.
    pub const fn new(value: f64) -> Self {
        Self {
            line_number: DEFAULT_LINE_NUMBER,
            value,
        }
    }

    /// `SEXPR_DOUBLE( double aValue, int aLineNumber )`.
    pub const fn with_line_number(value: f64, line_number: i32) -> Self {
        Self {
            line_number: line_from_int(line_number),
            value,
        }
    }
}

/// `SEXPR_STRING`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SexprString {
    line_number: u64,
    /// `m_value`.
    pub value: Vec<u8>,
}

impl SexprString {
    /// `SEXPR_STRING( const std::string& aValue )`: a copy, line number 1.
    pub fn new(value: &[u8]) -> Self {
        Self {
            line_number: DEFAULT_LINE_NUMBER,
            value: value.to_vec(),
        }
    }

    /// `SEXPR_STRING( const std::string& aValue, int aLineNumber )`.
    pub fn with_line_number(value: &[u8], line_number: i32) -> Self {
        Self {
            line_number: line_from_int(line_number),
            value: value.to_vec(),
        }
    }
}

/// `SEXPR_SYMBOL`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SexprSymbol {
    line_number: u64,
    /// `m_value`.
    pub value: Vec<u8>,
}

impl SexprSymbol {
    /// `SEXPR_SYMBOL( const std::string& aValue )`: a copy, line number 1.
    pub fn new(value: &[u8]) -> Self {
        Self {
            line_number: DEFAULT_LINE_NUMBER,
            value: value.to_vec(),
        }
    }

    /// `SEXPR_SYMBOL( const std::string& aValue, int aLineNumber )`.
    pub fn with_line_number(value: &[u8], line_number: i32) -> Self {
        Self {
            line_number: line_from_int(line_number),
            value: value.to_vec(),
        }
    }
}

/// `SEXPR_LIST`: owns its children, deleting them when it is dropped (`~SEXPR_LIST`, in
/// [`crate::sexpr_cpp`]).
///
/// The implicitly declared C++ copy constructor copies the child pointers, so a copy and
/// its original delete the same children; it is deliberately not provided.
#[derive(Debug, PartialEq)]
pub struct SexprList {
    line_number: u64,
    /// `m_children`.
    pub children: SexprVector,
    /// `m_inStreamChild`: the next child the extraction operators read.
    pub(crate) in_stream_child: i32,
}

impl SexprList {
    /// `SEXPR_LIST()`: no children, line number 1.
    pub const fn new() -> Self {
        Self {
            line_number: DEFAULT_LINE_NUMBER,
            children: Vec::new(),
            in_stream_child: 0,
        }
    }

    /// `SEXPR_LIST( int aLineNumber )`. A single `int` argument selects this non-template
    /// constructor over the variadic template.
    pub const fn with_line_number(line_number: i32) -> Self {
        Self {
            line_number: line_from_int(line_number),
            children: Vec::new(),
            in_stream_child: 0,
        }
    }

    /// `template <typename... Args> SEXPR_LIST( const Args&... args )`: line number 1, then
    /// `AddChildren( args... )`. A failure of `AddChildren` fails the construction.
    pub fn from_args(args: Vec<ChildrenArg>) -> Result<Self, SexprError> {
        let mut list = Self::new();
        list.add_children(args)?;
        Ok(list)
    }

    /// `template <typename... Args> size_t Scan( const Args&... args )`: the arguments as an
    /// array, then `doScan( arg_array, sizeof...( Args ) )`.
    ///
    /// The C++ template needs at least one argument (an empty array is ill-formed); an empty
    /// slice here is passed through with `num_args` 0.
    pub fn scan(&mut self, args: &mut [ScanArg<'_>]) -> Result<u64, SexprError> {
        self.do_scan(args)
    }

    /// `template <typename... Args> void AddChildren( const Args&... args )`: the arguments as
    /// an array, then `doAddChildren( arg_array, sizeof...( Args ) )`.
    pub fn add_children(&mut self, args: Vec<ChildrenArg>) -> Result<(), SexprError> {
        self.do_add_children(args)
    }

    /// The private `m_inStreamChild`, for observation only (the reference adapter reads it
    /// through explicit-instantiation member access).
    pub const fn in_stream_child(&self) -> i32 {
        self.in_stream_child
    }
}

impl Default for SexprList {
    fn default() -> Self {
        Self::new()
    }
}

/// Any node: a `SEXPR*` that may point to each concrete type.
#[derive(Debug, PartialEq)]
pub enum Sexpr {
    /// A `SEXPR_LIST`.
    List(SexprList),
    /// A `SEXPR_INTEGER`.
    Integer(SexprInteger),
    /// A `SEXPR_DOUBLE`.
    Double(SexprDouble),
    /// A `SEXPR_STRING`.
    String(SexprString),
    /// A `SEXPR_SYMBOL`.
    Symbol(SexprSymbol),
}

impl From<SexprList> for Sexpr {
    fn from(value: SexprList) -> Self {
        Self::List(value)
    }
}

impl From<SexprInteger> for Sexpr {
    fn from(value: SexprInteger) -> Self {
        Self::Integer(value)
    }
}

impl From<SexprDouble> for Sexpr {
    fn from(value: SexprDouble) -> Self {
        Self::Double(value)
    }
}

impl From<SexprString> for Sexpr {
    fn from(value: SexprString) -> Self {
        Self::String(value)
    }
}

impl From<SexprSymbol> for Sexpr {
    fn from(value: SexprSymbol) -> Self {
        Self::Symbol(value)
    }
}

/// A shared view of a node as its concrete type (`static_cast` after a type check).
#[derive(Clone, Copy, Debug)]
pub enum View<'a> {
    /// A `SEXPR_LIST`.
    List(&'a SexprList),
    /// A `SEXPR_INTEGER`.
    Integer(&'a SexprInteger),
    /// A `SEXPR_DOUBLE`.
    Double(&'a SexprDouble),
    /// A `SEXPR_STRING`.
    String(&'a SexprString),
    /// A `SEXPR_SYMBOL`.
    Symbol(&'a SexprSymbol),
}

/// A mutable view of a node as its concrete type.
#[derive(Debug)]
pub enum ViewMut<'a> {
    /// A `SEXPR_LIST`.
    List(&'a mut SexprList),
    /// A `SEXPR_INTEGER`.
    Integer(&'a mut SexprInteger),
    /// A `SEXPR_DOUBLE`.
    Double(&'a mut SexprDouble),
    /// A `SEXPR_STRING`.
    String(&'a mut SexprString),
    /// A `SEXPR_SYMBOL`.
    Symbol(&'a mut SexprSymbol),
}

/// `class SEXPR`: what every node is. The inline members of `sexpr.h` are provided here;
/// the out-of-line members of `sexpr.cpp` are [`crate::sexpr_cpp::SexprOps`].
pub trait Node {
    /// The node as its concrete type.
    fn view(&self) -> View<'_>;

    /// The node as its concrete type, mutably.
    fn view_mut(&mut self) -> ViewMut<'_>;

    /// The protected `m_type`, set by the constructor and never changed.
    fn sexpr_type(&self) -> SexprType {
        match self.view() {
            View::List(_) => SexprType::List,
            View::Integer(_) => SexprType::AtomInteger,
            View::Double(_) => SexprType::AtomDouble,
            View::String(_) => SexprType::AtomString,
            View::Symbol(_) => SexprType::AtomSymbol,
        }
    }

    /// `bool IsList() const`.
    fn is_list(&self) -> bool {
        self.sexpr_type() == SexprType::List
    }

    /// `bool IsSymbol() const`.
    fn is_symbol(&self) -> bool {
        self.sexpr_type() == SexprType::AtomSymbol
    }

    /// `bool IsString() const`.
    fn is_string(&self) -> bool {
        self.sexpr_type() == SexprType::AtomString
    }

    /// `bool IsDouble() const`.
    fn is_double(&self) -> bool {
        self.sexpr_type() == SexprType::AtomDouble
    }

    /// `bool IsInteger() const`.
    fn is_integer(&self) -> bool {
        self.sexpr_type() == SexprType::AtomInteger
    }

    /// `size_t GetLineNumber() const`.
    fn get_line_number(&self) -> u64 {
        match self.view() {
            View::List(node) => node.line_number,
            View::Integer(node) => node.line_number,
            View::Double(node) => node.line_number,
            View::String(node) => node.line_number,
            View::Symbol(node) => node.line_number,
        }
    }
}

impl Node for Sexpr {
    fn view(&self) -> View<'_> {
        match self {
            Self::List(node) => View::List(node),
            Self::Integer(node) => View::Integer(node),
            Self::Double(node) => View::Double(node),
            Self::String(node) => View::String(node),
            Self::Symbol(node) => View::Symbol(node),
        }
    }

    fn view_mut(&mut self) -> ViewMut<'_> {
        match self {
            Self::List(node) => ViewMut::List(node),
            Self::Integer(node) => ViewMut::Integer(node),
            Self::Double(node) => ViewMut::Double(node),
            Self::String(node) => ViewMut::String(node),
            Self::Symbol(node) => ViewMut::Symbol(node),
        }
    }
}

macro_rules! concrete_node {
    ($type:ident, $variant:ident) => {
        impl Node for $type {
            fn view(&self) -> View<'_> {
                View::$variant(self)
            }

            fn view_mut(&mut self) -> ViewMut<'_> {
                ViewMut::$variant(self)
            }
        }
    };
}

concrete_node!(SexprList, List);
concrete_node!(SexprInteger, Integer);
concrete_node!(SexprDouble, Double);
concrete_node!(SexprString, String);
concrete_node!(SexprSymbol, Symbol);

/// `struct _OUT_STRING`: a string to add as a symbol (`_Symbol`) or as a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutString<'a> {
    /// `_Symbol`.
    pub symbol: bool,
    /// `_String`, a reference to the caller's string.
    pub string: &'a [u8],
}

/// `_OUT_STRING AsSymbol( const std::string& aString )`.
pub const fn as_symbol(string: &[u8]) -> OutString<'_> {
    OutString {
        symbol: true,
        string,
    }
}

/// `_OUT_STRING AsString( const std::string& aString )`.
pub const fn as_string(string: &[u8]) -> OutString<'_> {
    OutString {
        symbol: false,
        string,
    }
}

/// `struct _IN_STRING`: a string to read a symbol (`_Symbol`) or a string into.
#[derive(Debug, PartialEq, Eq)]
pub struct InString<'a> {
    /// `_Symbol`.
    pub symbol: bool,
    /// `_String`, a reference to the caller's string.
    pub string: &'a mut Vec<u8>,
}

/// `_IN_STRING AsSymbol( std::string& aString )`.
pub fn as_symbol_in(string: &mut Vec<u8>) -> InString<'_> {
    InString {
        symbol: true,
        string,
    }
}

/// `_IN_STRING AsString( std::string& aString )`.
pub fn as_string_in(string: &mut Vec<u8>) -> InString<'_> {
    InString {
        symbol: false,
        string,
    }
}

/// `SEXPR_SCAN_ARG::Type`: which union member (or `str_value`) an argument uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanArgType {
    /// `INT`.
    Int,
    /// `DOUBLE`.
    Double,
    /// `STRING`.
    String,
    /// `LONGINT`.
    LongInt,
    /// `STRING_COMP`.
    StringComp,
    /// `SEXPR_STRING`.
    SexprString,
}

/// What a [`ScanArg`] holds: the destination of a read, or the string to compare with.
#[derive(Debug)]
pub(crate) enum ScanTarget<'a> {
    /// `u.int_value`. `doScan` would store a `double` through it, which is undefined, so the
    /// port never writes it.
    Int(#[allow(dead_code)] &'a mut i32),
    /// `u.dbl_value`.
    Double(&'a mut f64),
    /// `u.str_value`.
    String(&'a mut Vec<u8>),
    /// `u.lint_value`.
    LongInt(&'a mut i64),
    /// `str_value`.
    StringComp(Vec<u8>),
    /// `u.sexpr_str`: its `_Symbol` and `_String`.
    SexprString {
        symbol: bool,
        string: &'a mut Vec<u8>,
    },
}

/// `class SEXPR_SCAN_ARG`: one argument of [`SexprList::scan`].
#[derive(Debug)]
pub struct ScanArg<'a> {
    pub(crate) target: ScanTarget<'a>,
}

impl<'a> ScanArg<'a> {
    /// `SEXPR_SCAN_ARG( int32_t* aValue )`: type `INT`.
    pub fn int(value: &'a mut i32) -> Self {
        Self {
            target: ScanTarget::Int(value),
        }
    }

    /// `SEXPR_SCAN_ARG( int64_t* aValue )`: type `LONGINT`.
    pub fn long_int(value: &'a mut i64) -> Self {
        Self {
            target: ScanTarget::LongInt(value),
        }
    }

    /// `SEXPR_SCAN_ARG( double* aValue )`: type `DOUBLE`.
    pub fn double(value: &'a mut f64) -> Self {
        Self {
            target: ScanTarget::Double(value),
        }
    }

    /// `SEXPR_SCAN_ARG( std::string* aValue )`: type `STRING`.
    pub fn string(value: &'a mut Vec<u8>) -> Self {
        Self {
            target: ScanTarget::String(value),
        }
    }

    /// `SEXPR_SCAN_ARG( _IN_STRING& aValue )`: type `SEXPR_STRING`. The C++ argument keeps a
    /// pointer to the `_IN_STRING`; the borrow keeps its `_Symbol` unchanged until the scan.
    pub fn sexpr_string(value: &'a mut InString<'_>) -> Self {
        Self {
            target: ScanTarget::SexprString {
                symbol: value.symbol,
                string: &mut *value.string,
            },
        }
    }

    /// `SEXPR_SCAN_ARG( const std::string* aValue )`: type `STRING_COMP`, a copy of the
    /// string to compare with.
    pub fn const_string(value: &[u8]) -> Self {
        Self {
            target: ScanTarget::StringComp(value.to_vec()),
        }
    }

    /// `SEXPR_SCAN_ARG( std::string aValue )`: type `STRING_COMP`.
    pub fn string_value(value: Vec<u8>) -> Self {
        Self {
            target: ScanTarget::StringComp(value),
        }
    }

    /// `SEXPR_SCAN_ARG( const char* aValue )`: type `STRING_COMP`, the bytes before the
    /// terminating NUL.
    pub fn c_str(value: &CStr) -> Self {
        Self {
            target: ScanTarget::StringComp(value.to_bytes().to_vec()),
        }
    }

    /// The private `type`.
    pub fn arg_type(&self) -> ScanArgType {
        match self.target {
            ScanTarget::Int(_) => ScanArgType::Int,
            ScanTarget::Double(_) => ScanArgType::Double,
            ScanTarget::String(_) => ScanArgType::String,
            ScanTarget::LongInt(_) => ScanArgType::LongInt,
            ScanTarget::StringComp(_) => ScanArgType::StringComp,
            ScanTarget::SexprString { .. } => ScanArgType::SexprString,
        }
    }
}

/// `SEXPR_CHILDREN_ARG::Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildrenArgType {
    /// `INT`.
    Int,
    /// `DOUBLE`.
    Double,
    /// `STRING`.
    String,
    /// `LONGINT`.
    LongInt,
    /// `SEXPR_STRING`.
    SexprString,
    /// `SEXPR_ATOM`.
    SexprAtom,
}

/// What a [`ChildrenArg`] holds.
#[derive(Debug)]
pub(crate) enum ChildrenValue {
    /// `u.int_value`.
    Int(i32),
    /// `u.dbl_value`.
    Double(f64),
    /// `str_value`.
    String(Vec<u8>),
    /// `u.lint_value`.
    LongInt(i64),
    /// `str_value` and `u.symbol`.
    SexprString { symbol: bool, string: Vec<u8> },
    /// `u.sexpr_ptr`, which the list takes ownership of when it is added.
    SexprAtom(Option<Box<Sexpr>>),
}

/// `class SEXPR_CHILDREN_ARG`: one argument of [`SexprList::add_children`] and
/// [`SexprList::from_args`].
#[derive(Debug)]
pub struct ChildrenArg {
    pub(crate) value: ChildrenValue,
}

impl ChildrenArg {
    /// `SEXPR_CHILDREN_ARG( int32_t aValue )`: type `INT`.
    pub const fn int(value: i32) -> Self {
        Self {
            value: ChildrenValue::Int(value),
        }
    }

    /// `SEXPR_CHILDREN_ARG( int64_t aValue )`: type `LONGINT`.
    pub const fn long_int(value: i64) -> Self {
        Self {
            value: ChildrenValue::LongInt(value),
        }
    }

    /// `SEXPR_CHILDREN_ARG( double aValue )`: type `DOUBLE`.
    pub const fn double(value: f64) -> Self {
        Self {
            value: ChildrenValue::Double(value),
        }
    }

    /// `SEXPR_CHILDREN_ARG( const std::string& aValue )`: type `STRING`, a copy.
    pub fn string(value: &[u8]) -> Self {
        Self {
            value: ChildrenValue::String(value.to_vec()),
        }
    }

    /// `SEXPR_CHILDREN_ARG( const char* aValue )`: type `STRING`, the bytes before the
    /// terminating NUL.
    pub fn c_str(value: &CStr) -> Self {
        Self {
            value: ChildrenValue::String(value.to_bytes().to_vec()),
        }
    }

    /// `SEXPR_CHILDREN_ARG( const _OUT_STRING& aValue )`: type `SEXPR_STRING`, a copy of
    /// `_String` and `_Symbol`.
    pub fn out_string(value: &OutString<'_>) -> Self {
        Self {
            value: ChildrenValue::SexprString {
                symbol: value.symbol,
                string: value.string.to_vec(),
            },
        }
    }

    /// `SEXPR_CHILDREN_ARG( SEXPR* aPointer )`: type `SEXPR_ATOM`; the node (or null) is
    /// added as it is.
    pub fn sexpr(value: Option<Box<Sexpr>>) -> Self {
        Self {
            value: ChildrenValue::SexprAtom(value),
        }
    }

    /// The private `type`.
    pub fn arg_type(&self) -> ChildrenArgType {
        match self.value {
            ChildrenValue::Int(_) => ChildrenArgType::Int,
            ChildrenValue::Double(_) => ChildrenArgType::Double,
            ChildrenValue::String(_) => ChildrenArgType::String,
            ChildrenValue::LongInt(_) => ChildrenArgType::LongInt,
            ChildrenValue::SexprString { .. } => ChildrenArgType::SexprString,
            ChildrenValue::SexprAtom(_) => ChildrenArgType::SexprAtom,
        }
    }
}

/// `class ISEXPRABLE`: an object that can serialize itself to and from a node. Both virtual
/// functions have the reference's default bodies.
pub trait Isexprable {
    /// `virtual SEXPR* SerializeSEXPR() const`: by default `nullptr`.
    fn serialize_sexpr(&self) -> Option<Box<Sexpr>> {
        None
    }

    /// `virtual void DeserializeSEXPR( SEXPR& sexp )`: by default nothing.
    fn deserialize_sexpr(&mut self, _sexp: &mut dyn Node) {}
}

/// A plain `ISEXPRABLE` object (the class is not abstract), with both default bodies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DefaultIsexprable;

impl Isexprable for DefaultIsexprable {}

/// `std::exception::what()`: a NUL-terminated message.
pub trait StdException {
    /// `const char* what() const noexcept`: the bytes before the first NUL.
    fn what(&self) -> &[u8];
}

/// `msg.c_str()` read as a C string: the bytes before the first embedded NUL.
fn c_string(message: &[u8]) -> &[u8] {
    match message.iter().position(|&byte| byte == 0) {
        Some(end) => message.get(..end).unwrap_or(message),
        None => message,
    }
}

/// `SEXPR::PARSE_EXCEPTION`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseException {
    msg: Vec<u8>,
}

impl ParseException {
    /// `PARSE_EXCEPTION( const std::string& aMessage )`: a copy of the message.
    pub fn new(message: &[u8]) -> Self {
        Self {
            msg: message.to_vec(),
        }
    }
}

impl StdException for ParseException {
    /// `what()`: `msg.c_str()`.
    fn what(&self) -> &[u8] {
        c_string(&self.msg)
    }
}

/// `SEXPR::INVALID_TYPE_EXCEPTION`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidTypeException {
    msg: Vec<u8>,
}

impl InvalidTypeException {
    /// `INVALID_TYPE_EXCEPTION( const std::string& aMessage )`: a copy of the message.
    pub fn new(message: &[u8]) -> Self {
        Self {
            msg: message.to_vec(),
        }
    }
}

impl StdException for InvalidTypeException {
    /// `what()`: `msg.c_str()`.
    fn what(&self) -> &[u8] {
        c_string(&self.msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_set_type_value_and_line() {
        let node = SexprInteger::new(-7);
        assert!(node.is_integer() && !node.is_list() && !node.is_double());
        assert_eq!((node.value, node.get_line_number()), (-7, 1));
        let node = SexprDouble::with_line_number(0.5, -1);
        assert!(node.is_double());
        assert_eq!(node.get_line_number(), u64::MAX);
        let node = SexprSymbol::with_line_number(b"a\0b", i32::MIN);
        assert!(node.is_symbol() && !node.is_string());
        assert_eq!(node.value, b"a\0b");
        assert_eq!(node.get_line_number(), u64::MAX - (1u64 << 31) + 1);
        let node = Sexpr::from(SexprString::with_line_number(b"", 42));
        assert!(node.is_string());
        assert_eq!(node.sexpr_type(), SexprType::AtomString);
        assert_eq!(node.get_line_number(), 42);
        let list = SexprList::with_line_number(0);
        assert!(list.is_list() && list.children.is_empty());
        assert_eq!((list.get_line_number(), list.in_stream_child()), (0, 0));
    }

    #[test]
    fn exceptions_report_their_message_up_to_the_first_nul() {
        assert_eq!(ParseException::new(b"bad\0tail").what(), b"bad");
        assert_eq!(InvalidTypeException::new(b"").what(), b"");
        assert_eq!(InvalidTypeException::new(b"\0").what(), b"");
        assert_eq!(InvalidTypeException::new(b"whole").what(), b"whole");
    }

    #[test]
    fn string_wrappers_keep_flag_and_reference() {
        let text = b"x".to_vec();
        let out = as_symbol(&text);
        assert!(out.symbol && core::ptr::eq(out.string, text.as_slice()));
        assert!(!as_string(&text).symbol);
        let mut target = Vec::new();
        let input = as_string_in(&mut target);
        assert!(!input.symbol);
        input.string.extend_from_slice(b"written");
        assert_eq!(target, b"written");
        assert!(as_symbol_in(&mut target).symbol);
    }

    #[test]
    fn argument_constructors_select_the_reference_types() {
        let (mut i, mut l, mut d, mut s) = (0i32, 0i64, 0f64, Vec::new());
        let mut text = Vec::new();
        let mut input = as_symbol_in(&mut text);
        let types = [
            ScanArg::int(&mut i).arg_type(),
            ScanArg::long_int(&mut l).arg_type(),
            ScanArg::double(&mut d).arg_type(),
            ScanArg::string(&mut s).arg_type(),
            ScanArg::sexpr_string(&mut input).arg_type(),
            ScanArg::const_string(b"c").arg_type(),
            ScanArg::string_value(b"v".to_vec()).arg_type(),
            ScanArg::c_str(c"z").arg_type(),
        ];
        use ScanArgType as T;
        assert_eq!(
            types,
            [
                T::Int,
                T::LongInt,
                T::Double,
                T::String,
                T::SexprString,
                T::StringComp,
                T::StringComp,
                T::StringComp
            ]
        );
        use ChildrenArgType as C;
        assert_eq!(
            [
                ChildrenArg::int(1).arg_type(),
                ChildrenArg::long_int(1).arg_type(),
                ChildrenArg::double(1.0).arg_type(),
                ChildrenArg::string(b"s").arg_type(),
                ChildrenArg::c_str(c"s").arg_type(),
                ChildrenArg::out_string(&as_symbol(b"s")).arg_type(),
                ChildrenArg::sexpr(None).arg_type(),
            ],
            [
                C::Int,
                C::LongInt,
                C::Double,
                C::String,
                C::String,
                C::SexprString,
                C::SexprAtom
            ]
        );
    }

    #[test]
    fn default_isexprable_serializes_to_null_and_ignores_input() {
        let mut object = DefaultIsexprable;
        assert!(object.serialize_sexpr().is_none());
        let mut node = Sexpr::from(SexprInteger::new(3));
        object.deserialize_sexpr(&mut node);
        assert_eq!(node, Sexpr::from(SexprInteger::new(3)));
    }
}
