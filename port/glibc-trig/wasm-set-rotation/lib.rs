// SPDX-License-Identifier: GPL-3.0-or-later
//! wasm32 export shim (CI only). The exported symbol needs `no_mangle`, so this wrapper lives
//! outside the `forbid(unsafe_code)` portable crate; it only moves bits.
use kicad_math::Matrix3x3D;

/// `m_data[index / 3][index % 3]` after `SetRotation(angle)` on a zero matrix, as u64 bits.
#[no_mangle]
pub extern "C" fn set_rotation_entry(angle_bits: u64, index: u32) -> u64 {
    let mut m = Matrix3x3D::new();
    m.set_rotation(f64::from_bits(angle_bits));
    let index = index as usize % 9;
    m.data[index / 3][index % 3].to_bits()
}
