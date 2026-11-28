// SPDX-License-Identifier: GPL-3.0-or-later
//! `libs/kiplatform/os/unix/secrets.cpp`: secrets in the freedesktop Secret Service through
//! libsecret.

use alloc::boxed::Box;
use alloc::vec::Vec;

use kicad_kiplatform::secrets::{GetDone, StoreDone};
use kicad_map_helpers::WxString;

use crate::host::{
    Attribute, Conv, PendingLookup, PendingStore, SecretSchema, UnixHost,
    SECRET_SCHEMA_ATTRIBUTE_STRING, SECRET_SCHEMA_NONE,
};
use crate::wx::{concat, from_c_str, from_utf8, utf8_str};
use crate::{c_str, Ptr, Result, UnixError};

/// `static const SecretSchema schema`: `"org.kicad.kicad"`, `SECRET_SCHEMA_NONE`, string
/// attributes `service` and `key`, then the `{ nullptr, SECRET_SCHEMA_ATTRIBUTE_STRING }`
/// terminator.
pub const SCHEMA: SecretSchema = SecretSchema {
    name: b"org.kicad.kicad",
    flags: SECRET_SCHEMA_NONE,
    attributes: &[
        (b"service", SECRET_SCHEMA_ATTRIBUTE_STRING),
        (b"key", SECRET_SCHEMA_ATTRIBUTE_STRING),
    ],
    terminator_type: SECRET_SCHEMA_ATTRIBUTE_STRING,
};

/// `SECRET_COLLECTION_DEFAULT`.
pub const SECRET_COLLECTION_DEFAULT: &[u8] = b"default";
/// `SECRET_SEARCH_ALL`.
pub const SECRET_SEARCH_ALL: i32 = 1 << 1;
/// `SECRET_SEARCH_LOAD_SECRETS`.
pub const SECRET_SEARCH_LOAD_SECRETS: i32 = 1 << 3;

/// `aService + ":" + aKey`: `operator+( const wxString&, const char* )` converts the colon
/// with `wxConvLibc`.
fn display<H: UnixHost + ?Sized>(
    host: &mut H,
    service: &WxString,
    key: &WxString,
) -> Result<WxString> {
    let colon = from_c_str(host, Conv::Libc, Some(b":"))?;
    concat(&[service.units(), colon.units(), key.units()])
}

/// The four UTF-8 buffers `StoreSecret` and `StoreSecretAsync` build, in declaration order.
fn store_buffers<H: UnixHost + ?Sized>(
    host: &mut H,
    service: &WxString,
    key: &WxString,
    secret: &WxString,
) -> Result<[Vec<u8>; 4]> {
    let display = display(host, service, key)?;
    let utf8_display = utf8_str(host, &display)?;
    let utf8_secret = utf8_str(host, secret)?;
    let utf8_service = utf8_str(host, service)?;
    let utf8_key = utf8_str(host, key)?;
    Ok([utf8_display, utf8_secret, utf8_service, utf8_key])
}

/// The `"service", ..., "key", ..., NULL` attribute list; `.data()` reads each buffer up to
/// its first NUL.
fn attributes<'a>(service: &'a [u8], key: &'a [u8]) -> [Attribute<'a>; 2] {
    [(b"service", c_str(service)), (b"key", c_str(key))]
}

/// `bool StoreSecret( aService, aKey, aSecret )`.
pub fn store_secret<H: UnixHost + ?Sized>(
    host: &mut H,
    service: &WxString,
    key: &WxString,
    secret: &WxString,
) -> Result<bool> {
    let [display, secret, service, key] = store_buffers(host, service, key, secret)?;
    let (_, error) = host.secret_password_store_sync(
        &SCHEMA,
        SECRET_COLLECTION_DEFAULT,
        c_str(&display),
        c_str(&secret),
        &attributes(&service, &key),
    )?;
    if let Some(error) = error {
        host.g_error_free(error)?;
        return Ok(false);
    }
    Ok(true)
}

/// `static void storeFinished( GObject* aSource, GAsyncResult* aResult, gpointer aDone )`:
/// the completion of [`store_secret_async`]. It owns the heap function and calls it with the
/// result when it is not empty.
pub fn store_finished<H: UnixHost + ?Sized>(
    host: &mut H,
    _source: Option<Ptr>,
    result: Ptr,
    pending: Box<PendingStore>,
) -> Result<()> {
    let mut pending = pending;
    let stored = host.secret_password_store_finish(result)?;
    if let Some(done) = pending.done.as_mut() {
        done(stored);
    }
    Ok(())
}

/// `void StoreSecretAsync( aService, aKey, aSecret, aDone )`: completes on the default main
/// context through [`store_finished`].
pub fn store_secret_async<H: UnixHost + ?Sized>(
    host: &mut H,
    service: &WxString,
    key: &WxString,
    secret: &WxString,
    done: StoreDone,
) -> Result<()> {
    let [display, secret, service, key] = store_buffers(host, service, key, secret)?;
    host.secret_password_store(
        &SCHEMA,
        SECRET_COLLECTION_DEFAULT,
        c_str(&display),
        c_str(&secret),
        &attributes(&service, &key),
        Box::new(PendingStore { done }),
    )
}

/// `bool GetSecret( aService, aKey, aSecret )`.
pub fn get_secret<H: UnixHost + ?Sized>(
    host: &mut H,
    service: &WxString,
    key: &WxString,
    secret: &mut WxString,
) -> Result<bool> {
    let utf8_service = utf8_str(host, service)?;
    let utf8_key = utf8_str(host, key)?;
    let (value, error) =
        host.secret_password_lookup_sync(&SCHEMA, &attributes(&utf8_service, &utf8_key))?;
    if let Some(error) = error {
        host.g_error_free(error)?;
        return Ok(false);
    }
    // libsecret reports a missing entry as a null secret with no error.
    let Some(value) = value else {
        return Ok(false);
    };
    *secret = from_utf8(Some(&value.bytes))?;
    host.g_free(Some(value.ptr))?;
    Ok(true)
}

/// `static void lookupFinished( GObject* aSource, GAsyncResult* aResult, gpointer aDone )`:
/// the completion of [`get_secret_async`]. It calls the function without checking it, so an
/// empty one throws `std::bad_function_call` ([`UnixError::BadFunctionCall`]).
pub fn lookup_finished<H: UnixHost + ?Sized>(
    host: &mut H,
    _source: Option<Ptr>,
    result: Ptr,
    pending: Box<PendingLookup>,
) -> Result<()> {
    let mut pending = pending;
    let secret = host.secret_password_lookup_finish(result)?;
    let Some(secret) = secret else {
        let done = pending.done.as_mut().ok_or(UnixError::BadFunctionCall)?;
        done(false, &WxString::new());
        return Ok(());
    };
    let value = from_utf8(Some(&secret.bytes))?;
    host.g_free(Some(secret.ptr))?;
    let done = pending.done.as_mut().ok_or(UnixError::BadFunctionCall)?;
    done(true, &value);
    Ok(())
}

/// `void GetSecretAsync( aService, aKey, aDone )`: completes through [`lookup_finished`].
pub fn get_secret_async<H: UnixHost + ?Sized>(
    host: &mut H,
    service: &WxString,
    key: &WxString,
    done: GetDone,
) -> Result<()> {
    let utf8_service = utf8_str(host, service)?;
    let utf8_key = utf8_str(host, key)?;
    host.secret_password_lookup(
        &SCHEMA,
        &attributes(&utf8_service, &utf8_key),
        Box::new(PendingLookup { done }),
    )
}

/// `bool GetSecretNoPrompt( aService, aKey, aSecret )`: the first unlocked matching item's
/// text, without unlocking a collection.
pub fn get_secret_no_prompt<H: UnixHost + ?Sized>(
    host: &mut H,
    service: &WxString,
    key: &WxString,
    secret: &mut WxString,
) -> Result<bool> {
    let utf8_service = utf8_str(host, service)?;
    let utf8_key = utf8_str(host, key)?;
    let table = host.secret_attributes_build(&SCHEMA, &attributes(&utf8_service, &utf8_key))?;
    let (items, error) = host.secret_service_search_sync(
        &SCHEMA,
        table,
        SECRET_SEARCH_ALL | SECRET_SEARCH_LOAD_SECRETS,
    )?;
    host.g_hash_table_unref(table)?;
    if let Some(error) = error {
        host.g_error_free(error)?;
        return Ok(false);
    }
    let mut found = false;
    for &item in &items.items {
        if found {
            break;
        }
        if host.secret_item_get_locked(item)? {
            continue;
        }
        if let Some(value) = host.secret_item_get_secret(item)? {
            if let Some(text) = host.secret_value_get_text(value)? {
                *secret = from_utf8(Some(&text))?;
                found = true;
            }
            host.secret_value_unref(value)?;
        }
    }
    host.g_list_free_full_unref(&items)?;
    Ok(found)
}

/// `bool DeleteSecret( aService, aKey )`.
pub fn delete_secret<H: UnixHost + ?Sized>(
    host: &mut H,
    service: &WxString,
    key: &WxString,
) -> Result<bool> {
    let utf8_service = utf8_str(host, service)?;
    let utf8_key = utf8_str(host, key)?;
    let (_, error) =
        host.secret_password_clear_sync(&SCHEMA, &attributes(&utf8_service, &utf8_key))?;
    if let Some(error) = error {
        host.g_error_free(error)?;
        return Ok(false);
    }
    Ok(true)
}
