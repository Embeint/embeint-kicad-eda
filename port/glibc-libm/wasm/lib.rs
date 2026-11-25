// SPDX-License-Identifier: GPL-3.0-or-later
//! CI-only bit transport. Compile the exact private modules, enforcing safe Rust
//! on them; the exported no_mangle symbols live outside those restrictions.
#[path = "../../../rust/kicad-math/src/glibc_libm.rs"]
#[forbid(unsafe_code)]
mod glibc_libm;
#[cfg(test)]
use kicad_math::matrix3x3;
#[path = "../../../rust/kicad-math/src/glibc_trig.rs"]
#[forbid(unsafe_code)]
#[allow(dead_code)]
mod glibc_trig;

#[no_mangle]
pub extern "C" fn angle_libm(x: u64, y: u64, function: u32) -> u64 {
    let x = f64::from_bits(x);
    let y = f64::from_bits(y);
    let z = match function {
        0 => glibc_libm::atan(x),
        1 => glibc_libm::atan2(y, x),
        2 => glibc_libm::tan(x),
        3 => glibc_libm::asin(x),
        _ => glibc_libm::acos(x),
    };
    z.to_bits()
}
