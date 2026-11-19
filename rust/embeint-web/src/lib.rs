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
//! The packed return value only has room for 32-bit pointers, so the exported functions exist
//! only on targets with 32-bit pointers (wasm32 among them).  A native build of this crate, which
//! the workspace makes for `cargo test` and clippy, compiles the packing and the request handling
//! but exports nothing.
//!
//! Errors in the request come back as JSON (`{"error": ...}`), never as a trap.  The layout code
//! does not panic on any input; should it ever, wasm32-unknown-unknown aborts, which the host sees
//! as a `WebAssembly.RuntimeError` and must treat as a lost module instance.

/// Pack a result pointer and length into the `u64` that `embeint_layout_box` returns.
pub const fn pack_parts(ptr: u32, len: u32) -> u64 {
    ((ptr as u64) << 32) | len as u64
}

/// Split a value from [`pack_parts`] into its pointer and length (what the host does).
pub const fn unpack_parts(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, packed as u32)
}

/// The response the ABI returns for one request (request bytes need not be UTF-8).
pub fn respond(request: &[u8]) -> Box<[u8]> {
    embeint_layout::layout_box_json_bytes(request)
        .into_bytes()
        .into_boxed_slice()
}

#[cfg(target_pointer_width = "32")]
mod abi {
    use std::alloc::{alloc, dealloc, Layout};
    use std::ptr::NonNull;

    // Compile-time guard: the packing below casts pointers and lengths to u32.
    const _: () = assert!(
        usize::BITS == 32,
        "the embeint-web ABI needs 32-bit pointers"
    );

    /// Allocate `len` bytes for the host to write a request into.  Returns null on failure, and
    /// a dangling non-null pointer, which owns no memory, for `len == 0`.
    #[no_mangle]
    pub extern "C" fn embeint_alloc(len: usize) -> *mut u8 {
        if len == 0 {
            return NonNull::<u8>::dangling().as_ptr();
        }

        match Layout::array::<u8>(len) {
            // SAFETY: the layout has a non-zero size.
            Ok(layout) => unsafe { alloc(layout) },
            Err(_) => std::ptr::null_mut(),
        }
    }

    /// Free a buffer from `embeint_alloc` or a result from `embeint_layout_box`.  A zero length
    /// owns no memory and is ignored.
    ///
    /// # Safety
    /// `ptr` and `len` must describe a buffer this module returned and that was not freed yet.
    #[no_mangle]
    pub unsafe extern "C" fn embeint_free(ptr: *mut u8, len: usize) {
        if ptr.is_null() || len == 0 {
            return;
        }

        if let Ok(layout) = Layout::array::<u8>(len) {
            // SAFETY: the caller guarantees ptr came from this allocator with this length.  A
            // result is a Box<[u8]> of exactly len bytes, whose layout is Layout::array(len).
            unsafe { dealloc(ptr, layout) };
        }
    }

    /// Lay out a symbol (see `embeint_layout::layout_box_json`).  Takes ownership of the request
    /// buffer and returns the packed result pointer and length.  The result is handed over in its
    /// own allocation, without a copy; free it with `embeint_free`.
    ///
    /// # Safety
    /// `ptr` and `len` must describe a buffer from `embeint_alloc` holding the request.
    #[no_mangle]
    pub unsafe extern "C" fn embeint_layout_box(ptr: *mut u8, len: usize) -> u64 {
        let response = {
            // SAFETY: the caller guarantees ptr/len describe an initialised buffer from
            // embeint_alloc (dangling but non-null and aligned when len is 0).
            let request = unsafe { std::slice::from_raw_parts(ptr, len) };
            super::respond(request)
        };

        // SAFETY: the request buffer is no longer borrowed.
        unsafe { embeint_free(ptr, len) };
        pack(response)
    }

    fn pack(response: Box<[u8]>) -> u64 {
        let len = response.len();
        // An empty box owns no memory and its pointer is dangling, as embeint_free expects.
        let ptr = Box::into_raw(response).cast::<u8>();
        super::pack_parts(ptr as usize as u32, len as u32)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn round_trip_through_the_abi() {
            let request = br#"{"pins": [{"number": "1", "name": "VIN", "type": "power_in"}]}"#;
            let input = embeint_alloc(request.len());
            unsafe { std::ptr::copy_nonoverlapping(request.as_ptr(), input, request.len()) };
            let packed = unsafe { embeint_layout_box(input, request.len()) };
            let (ptr, len) = crate::unpack_parts(packed);
            let response =
                unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize) };
            let expected = embeint_layout::layout_box_json(std::str::from_utf8(request).unwrap());
            assert_eq!(response, expected.as_bytes());
            unsafe { embeint_free(ptr as usize as *mut u8, len as usize) };
        }

        #[test]
        fn zero_length_buffers_own_no_memory() {
            let empty = embeint_alloc(0);
            assert!(!empty.is_null());
            unsafe { embeint_free(empty, 0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_keeps_pointer_and_length_apart() {
        for (ptr, len) in [
            (0, 0),
            (1, 2),
            (0x1234_5678, 0x9abc_def0),
            (u32::MAX, u32::MAX),
        ] {
            assert_eq!(unpack_parts(pack_parts(ptr, len)), (ptr, len));
        }
        assert_eq!(pack_parts(0x10, 0x20), 0x0000_0010_0000_0020);
    }

    #[test]
    fn response_is_the_layout_json() {
        let request = br#"{"pins": [{"number": "1", "name": "VIN", "type": "power_in"}]}"#;
        let expected = embeint_layout::layout_box_json(std::str::from_utf8(request).unwrap());
        assert_eq!(&*respond(request), expected.as_bytes());
        assert_eq!(
            &*respond(b"\xff"),
            br#"{"error":{"type":"RequestError","message":"request is not valid JSON"}}"#
        );
    }
}
