// SPDX-License-Identifier: GPL-3.0-or-later
//! WebAssembly binding of the portable Embeint KiCad components.
//!
//! A deliberately small C-style ABI, so the module needs no binding generator and loads with the
//! plain `WebAssembly` API in a browser, a Web Worker or Node.js (see `js/embeint_layout.mjs`):
//!
//! 1. `embeint_alloc(len)` returns a buffer; the host copies the UTF-8 request into it.
//! 2. `embeint_layout_box(ptr, len)` consumes that buffer and returns a packed `u64`: the result
//!    pointer in the high 32 bits and its byte length in the low 32 bits.  The result is UTF-8 JSON.
//! 3. `embeint_free(ptr, len)` releases the result.
//!
//! Errors in the request come back as JSON (`{"error": ...}`), never as a trap.  The layout code
//! does not panic on any input; should it ever, wasm32-unknown-unknown aborts, which the host sees
//! as a `WebAssembly.RuntimeError` and must treat as a lost module instance.

use std::alloc::{alloc, dealloc, Layout};

/// Allocate `len` bytes for the host to write a request into.  Returns null on failure.
#[no_mangle]
pub extern "C" fn embeint_alloc(len: usize) -> *mut u8 {
    match Layout::array::<u8>(len.max(1)) {
        // SAFETY: the layout has a non-zero size.
        Ok(layout) => unsafe { alloc(layout) },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a buffer from `embeint_alloc` or a result from `embeint_layout_box`.
///
/// # Safety
/// `ptr` and `len` must describe a buffer this module returned and that was not freed yet.
#[no_mangle]
pub unsafe extern "C" fn embeint_free(ptr: *mut u8, len: usize) {
    if let (false, Ok(layout)) = (ptr.is_null(), Layout::array::<u8>(len.max(1))) {
        // SAFETY: the caller guarantees ptr came from this allocator with this length.
        unsafe { dealloc(ptr, layout) };
    }
}

/// Lay out a symbol (see `embeint_layout::layout_box_json`).  Takes ownership of the request
/// buffer and returns the packed result pointer and length, or 0 if the result cannot be allocated.
///
/// # Safety
/// `ptr` and `len` must describe a buffer from `embeint_alloc` holding the request.
#[no_mangle]
pub unsafe extern "C" fn embeint_layout_box(ptr: *mut u8, len: usize) -> u64 {
    let response = {
        // SAFETY: the caller guarantees ptr/len describe an initialised buffer from embeint_alloc.
        let request = unsafe { std::slice::from_raw_parts(ptr, len) };
        embeint_layout::layout_box_json_bytes(request)
    };

    // SAFETY: the request buffer is no longer borrowed.
    unsafe { embeint_free(ptr, len) };
    pack(response.into_bytes())
}

fn pack(bytes: Vec<u8>) -> u64 {
    let len = bytes.len();
    let out = embeint_alloc(len);

    if out.is_null() {
        return 0;
    }

    // SAFETY: out has room for len bytes and does not overlap bytes.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, len) };
    ((out as usize as u64) << 32) | len as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_through_the_abi() {
        let request = br#"{"pins": [{"number": "1", "name": "VIN", "type": "power_in"}]}"#;
        let input = embeint_alloc(request.len());
        unsafe { std::ptr::copy_nonoverlapping(request.as_ptr(), input, request.len()) };
        // Native pointers are 64-bit, so only check the length half here; the WebAssembly
        // build, where pointers are 32-bit, is exercised by rust/parity.
        let packed = unsafe { embeint_layout_box(input, request.len()) };
        let expected = embeint_layout::layout_box_json(std::str::from_utf8(request).unwrap());
        assert_eq!((packed & 0xffff_ffff) as usize, expected.len());
    }
}
