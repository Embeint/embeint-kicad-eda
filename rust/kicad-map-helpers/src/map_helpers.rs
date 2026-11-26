// SPDX-License-Identifier: GPL-3.0-or-later
//! The seven overloads of `libs/core/include/core/map_helpers.h`.
//!
//! C++ selects an overload from the argument types; Rust names each one. The
//! mapping, in header order:
//!
//! | C++ overload | Rust |
//! | --- | --- |
//! | `get_opt( const std::map<wxString, V>&, const wxString& )` | [`get_opt_wx`] |
//! | `get_opt( const std::map<wxString, V>&, const char* )` | [`get_opt_cstr`] |
//! | `get_opt( const std::map<K, V>&, const K& )` | [`get_opt`] |
//! | `get_def( const std::map<wxString, wxString>&, const char*, const char* = "" )` | [`get_def_cstr`] |
//! | `get_def( const std::map<wxString, wxString>&, const char*, const wxString& = wxString() )` | [`get_def_cstr_wx`] |
//! | `get_def( const std::map<wxString, wxString>&, const wxString&, const wxString& = wxString() )` | [`get_def_wx`], [`get_def_wx_or_empty`] |
//! | `get_def( const std::map<K, V>&, const K&, const V& = V() )` | [`get_def`], [`get_def_or_value_init`] |
//!
//! The default arguments of the two `const char*`-key `get_def` overloads cannot be
//! used in C++: a two-argument call with a `const char*` key matches both with
//! identical conversions and is ambiguous (the oracle adapter checks this at compile
//! time). They therefore have no two-argument Rust form; [`GET_DEF_CSTR_DEFAULT`]
//! names the first one's `""`.
//!
//! Every C++ overload copies the found value (or the default) into its result, and the
//! `const char*` overloads convert with `wxString::FromUTF8` first. Each copy and
//! conversion here is fallible; the operation order is the reference's: convert the
//! key, find it, then copy the value or convert/copy the default.

use alloc::collections::BTreeMap;
use core::ffi::CStr;

use crate::wx_string::{WxError, WxString};

/// The `""` default argument of `get_def( map, const char*, const char* aDefval = "" )`.
pub const GET_DEF_CSTR_DEFAULT: &CStr = c"";

/// A C++ copy (copy constructor) that may refuse to allocate.
///
/// `Copy` types copy infallibly; [`WxString`] reports a refused allocation. A caller
/// type implements this with its C++ copy constructor's semantics.
pub trait TryClone: Sized {
    /// Copy `self`, or report the refused allocation.
    fn try_clone(&self) -> Result<Self, WxError>;
}

impl<T: Copy> TryClone for T {
    fn try_clone(&self) -> Result<Self, WxError> {
        Ok(*self)
    }
}

impl TryClone for WxString {
    fn try_clone(&self) -> Result<Self, WxError> {
        WxString::try_clone(self)
    }
}

/// `std::optional<V> get_opt( const std::map<wxString, V>& aMap, const wxString& aKey )`.
pub fn get_opt_wx<V: TryClone>(
    map: &BTreeMap<WxString, V>,
    key: &WxString,
) -> Result<Option<V>, WxError> {
    match map.get(key) {
        None => Ok(None),
        Some(value) => value.try_clone().map(Some),
    }
}

/// `std::optional<V> get_opt( const std::map<wxString, V>& aMap, const char* aKey )`:
/// `get_opt( aMap, wxString::FromUTF8( aKey ) )`. `None` is a null `aKey`.
///
/// A null, empty or invalid UTF-8 key converts to the empty string, so it finds the
/// entry with the empty key if there is one.
pub fn get_opt_cstr<V: TryClone>(
    map: &BTreeMap<WxString, V>,
    key: Option<&CStr>,
) -> Result<Option<V>, WxError> {
    get_opt_wx(map, &WxString::from_utf8(key)?)
}

/// `std::optional<V> get_opt( const std::map<K, V>& aMap, const K& aKey )`.
///
/// `K: Ord` must be the order of the C++ map's `std::less<K>`.
pub fn get_opt<K: Ord, V: TryClone>(map: &BTreeMap<K, V>, key: &K) -> Result<Option<V>, WxError> {
    match map.get(key) {
        None => Ok(None),
        Some(value) => value.try_clone().map(Some),
    }
}

/// `wxString get_def( const std::map<wxString, wxString>& aMap, const char* aKey,
/// const char* aDefval )`: the found value, else `wxString::FromUTF8( aDefval )`.
///
/// `None` is a null pointer for either argument (it converts to the empty string).
/// The C++ default argument `""` is [`GET_DEF_CSTR_DEFAULT`]; see the module notes.
pub fn get_def_cstr(
    map: &BTreeMap<WxString, WxString>,
    key: Option<&CStr>,
    defval: Option<&CStr>,
) -> Result<WxString, WxError> {
    let key = WxString::from_utf8(key)?;
    match map.get(&key) {
        None => WxString::from_utf8(defval),
        Some(value) => value.try_clone(),
    }
}

/// `wxString get_def( const std::map<wxString, wxString>& aMap, const char* aKey,
/// const wxString& aDefval )`: the found value, else a copy of `aDefval`.
///
/// The C++ default argument `wxString()` cannot be used (see the module notes).
pub fn get_def_cstr_wx(
    map: &BTreeMap<WxString, WxString>,
    key: Option<&CStr>,
    defval: &WxString,
) -> Result<WxString, WxError> {
    let key = WxString::from_utf8(key)?;
    match map.get(&key) {
        None => defval.try_clone(),
        Some(value) => value.try_clone(),
    }
}

/// `wxString get_def( const std::map<wxString, wxString>& aMap, const wxString& aKey,
/// const wxString& aDefval )`.
pub fn get_def_wx(
    map: &BTreeMap<WxString, WxString>,
    key: &WxString,
    defval: &WxString,
) -> Result<WxString, WxError> {
    match map.get(key) {
        None => defval.try_clone(),
        Some(value) => value.try_clone(),
    }
}

/// The same overload called with its default argument `aDefval = wxString()`.
pub fn get_def_wx_or_empty(
    map: &BTreeMap<WxString, WxString>,
    key: &WxString,
) -> Result<WxString, WxError> {
    get_def_wx(map, key, &WxString::new())
}

/// `V get_def( const std::map<K, V>& aMap, const K& aKey, const V& aDefval )`.
pub fn get_def<K: Ord, V: TryClone>(
    map: &BTreeMap<K, V>,
    key: &K,
    defval: &V,
) -> Result<V, WxError> {
    match map.get(key) {
        None => defval.try_clone(),
        Some(value) => value.try_clone(),
    }
}

/// The same overload called with its default argument `aDefval = V()`.
///
/// `V::default()` must equal C++ value-initialisation `V()`: zero for arithmetic
/// types and for enumerations (which need not be an enumerator), the empty string for
/// [`WxString`]. Like the reference, the default is constructed before the lookup.
pub fn get_def_or_value_init<K: Ord, V: TryClone + Default>(
    map: &BTreeMap<K, V>,
    key: &K,
) -> Result<V, WxError> {
    let defval = V::default();
    match map.get(key) {
        None => defval.try_clone(),
        Some(value) => value.try_clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, Eq)]
    struct CopiedValue(u32);

    impl TryClone for CopiedValue {
        fn try_clone(&self) -> Result<Self, WxError> {
            Ok(Self(self.0 + 1))
        }
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct RefusedCopy;

    impl TryClone for RefusedCopy {
        fn try_clone(&self) -> Result<Self, WxError> {
            Err(WxError::Alloc { requested: 1 })
        }
    }

    #[test]
    fn value_initialized_default_is_copied() -> Result<(), WxError> {
        let map = BTreeMap::<i32, CopiedValue>::new();
        assert_eq!(get_def_or_value_init(&map, &1)?, CopiedValue(1));
        assert_eq!(
            get_def_or_value_init(&map, &1)?,
            get_def(&map, &1, &CopiedValue::default())?
        );
        Ok(())
    }

    #[test]
    fn value_initialized_default_reports_copy_failure() {
        let map = BTreeMap::<i32, RefusedCopy>::new();
        assert_eq!(
            get_def_or_value_init(&map, &1),
            Err(WxError::Alloc { requested: 1 })
        );
    }

    fn wx(text: &str) -> Result<WxString, WxError> {
        let units: alloc::vec::Vec<i32> = text.chars().map(|c| c as i32).collect();
        WxString::from_units(&units)
    }

    #[test]
    fn overloads_find_and_default() -> Result<(), WxError> {
        let mut map = BTreeMap::new();
        map.insert(wx("TYPE")?, wx("res")?);
        map.insert(WxString::new(), wx("empty")?);
        assert_eq!(get_def_cstr(&map, Some(c"TYPE"), Some(c"x"))?, wx("res")?);
        assert_eq!(get_def_cstr(&map, Some(c"NONE"), Some(c"x"))?, wx("x")?);
        // Invalid UTF-8 and a null key both look up the empty key.
        assert_eq!(get_opt_cstr(&map, Some(c"\xFF"))?, Some(wx("empty")?));
        assert_eq!(get_opt_cstr(&map, None)?, Some(wx("empty")?));
        assert_eq!(get_def_wx_or_empty(&map, &wx("NONE")?)?, WxString::new());
        assert_eq!(get_def_cstr_wx(&map, Some(c"NONE"), &wx("d")?)?, wx("d")?);
        let numbers = BTreeMap::from([(1, 10), (2, 20)]);
        assert_eq!(get_opt(&numbers, &2)?, Some(20));
        assert_eq!(get_def(&numbers, &3, &7)?, 7);
        assert_eq!(get_def_or_value_init(&numbers, &3)?, 0);
        Ok(())
    }
}
