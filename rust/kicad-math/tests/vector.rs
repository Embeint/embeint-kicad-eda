// SPDX-License-Identifier: GPL-3.0-or-later
//! Representation checks for the minimal `VECTOR2`/`VECTOR3` co-port.
//!
//! Only the C++ declarations listed in `TYPE_SUPPORT.md` are covered: value-initialising
//! default construction, component construction, copy construction/assignment and the
//! specialisation typedefs. Each of these stores the given components unchanged, so floats are
//! compared by their bits (NaN payloads and signs included) and integers at their extremes.

use std::any::TypeId;

use kicad_math::vector;
use kicad_math::{Vector2, Vector2D, Vector2I, Vector2L, Vector3, Vector3D, Vector3I, Vector3U};

/// f64 bit patterns that a value-preserving copy must keep exactly: quiet and signalling NaNs of
/// both signs with payloads, signed zeros, infinities, subnormals and the finite extremes.
const F64_BITS: [u64; 14] = [
    0x7ff8_0000_0000_0000,
    0x7ff8_0000_dead_beef,
    0xfff8_0000_0000_0001,
    0x7ff0_0000_0000_0001,
    0xfff7_ffff_ffff_ffff,
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x7ff0_0000_0000_0000,
    0xfff0_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x800f_ffff_ffff_ffff,
    0x7fef_ffff_ffff_ffff,
    0xffef_ffff_ffff_ffff,
    0x3ff0_0000_0000_0001,
];

/// f32 patterns for the generic component type: quiet and signalling NaNs with payloads and -0.
const F32_BITS: [u32; 5] = [
    0x7fc0_1234,
    0x7fa0_0001,
    0xff80_0001,
    0x8000_0000,
    0x0000_0001,
];

fn bits2(v: &Vector2<f64>) -> [u64; 2] {
    [v.x.to_bits(), v.y.to_bits()]
}

fn bits3(v: &Vector3<f64>) -> [u64; 3] {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

/// `VECTOR2()` is `x{}, y{}` and `VECTOR3() = default` with `T x{}, y{}, z{}`: value
/// initialisation, which is +0.0 (all bits clear, never -0.0) for floats and 0 for integers.
#[test]
fn default_value_initialises_every_component() {
    assert_eq!(bits2(&Vector2D::default()), [0, 0]);
    assert_eq!(bits3(&Vector3D::default()), [0, 0, 0]);

    let i = Vector2I::default();
    assert_eq!((i.x, i.y), (0, 0));
    let l = Vector2L::default();
    assert_eq!((l.x, l.y), (0, 0));
    let i3 = Vector3I::default();
    assert_eq!((i3.x, i3.y, i3.z), (0, 0, 0));
    let u3 = Vector3U::default();
    assert_eq!((u3.x, u3.y, u3.z), (0, 0, 0));

    let f = Vector3::<f32>::default();
    assert_eq!([f.x.to_bits(), f.y.to_bits(), f.z.to_bits()], [0, 0, 0]);
}

/// `VECTOR2( T, T )` and `VECTOR3( T, T, T )` store each argument in its own component, in
/// order, without changing a bit: every pair and triple of the patterns is checked.
#[test]
fn constructors_keep_float_bits_in_component_order() {
    for &a in &F64_BITS {
        for &b in &F64_BITS {
            let v = Vector2D::new(f64::from_bits(a), f64::from_bits(b));
            assert_eq!(bits2(&v), [a, b]);
            for &c in &F64_BITS {
                let w = Vector3D::new(f64::from_bits(a), f64::from_bits(b), f64::from_bits(c));
                assert_eq!(bits3(&w), [a, b, c]);
            }
        }
    }
}

#[test]
fn constructors_keep_integer_extremes() {
    for (x, y) in [(i32::MIN, i32::MAX), (i32::MAX, i32::MIN), (-1, 0), (0, -1)] {
        let v = Vector2I::new(x, y);
        assert_eq!((v.x, v.y), (x, y));
        let w = Vector3I::new(x, y, x);
        assert_eq!((w.x, w.y, w.z), (x, y, x));
    }
    for (x, y) in [
        (i64::MIN, i64::MAX),
        (i64::MAX, i64::MIN),
        (i64::MIN + 1, -1),
    ] {
        let v = Vector2L::new(x, y);
        assert_eq!((v.x, v.y), (x, y));
    }
    let u = Vector3U::new(u32::MAX, 0, u32::MAX - 1);
    assert_eq!((u.x, u.y, u.z), (u32::MAX, 0, u32::MAX - 1));
}

/// The C++ constructors are `constexpr` for `VECTOR2`; `new` is usable in constants for both.
#[test]
fn constructors_are_const() {
    const V: Vector2L = Vector2L::new(i64::MIN, i64::MAX);
    const W: Vector3U = Vector3U::new(1, 2, u32::MAX);
    assert_eq!((V.x, V.y), (i64::MIN, i64::MAX));
    assert_eq!((W.x, W.y, W.z), (1, 2, u32::MAX));
}

/// `VECTOR2( const VECTOR2<T>& )`, `VECTOR2<T>::operator=` and `VECTOR3`'s implicit copy are
/// memberwise copies. A copy, a clone and an assignment over an existing value all keep every
/// bit, and the source remains usable and unchanged afterwards.
#[test]
fn copy_clone_and_assignment_keep_bits() {
    for &a in &F64_BITS {
        for &b in &F64_BITS {
            let source = Vector2D::new(f64::from_bits(a), f64::from_bits(b));
            let copied = source;
            #[allow(clippy::clone_on_copy)]
            let cloned = source.clone();
            let mut assigned = Vector2D::new(1.0, -1.0);
            assert_eq!(bits2(&assigned), [1.0f64.to_bits(), (-1.0f64).to_bits()]);
            assigned = source;
            assert_eq!(bits2(&copied), [a, b]);
            assert_eq!(bits2(&cloned), [a, b]);
            assert_eq!(bits2(&assigned), [a, b]);
            assert_eq!(bits2(&source), [a, b]);

            let source3 = Vector3D::new(f64::from_bits(b), f64::from_bits(a), f64::from_bits(b));
            let copied3 = source3;
            #[allow(clippy::clone_on_copy)]
            let cloned3 = source3.clone();
            assert_eq!(bits3(&copied3), [b, a, b]);
            assert_eq!(bits3(&cloned3), [b, a, b]);
        }
    }

    let l = Vector2L::new(i64::MIN, i64::MAX);
    let mut m = Vector2L::default();
    assert_eq!((m.x, m.y), (0, 0));
    m = l;
    assert_eq!((m.x, m.y), (i64::MIN, i64::MAX));
    assert_eq!((l.x, l.y), (i64::MIN, i64::MAX));
}

/// The template parameter is the component type (`T x, y`): coordinates keep it, so f32 NaN
/// payloads survive and narrow or non-`Copy` component types work through the same `new`.
#[test]
fn components_keep_the_type_parameter() {
    for &a in &F32_BITS {
        for &b in &F32_BITS {
            let v = Vector2::<f32>::new(f32::from_bits(a), f32::from_bits(b));
            let w = v;
            assert_eq!([w.x.to_bits(), w.y.to_bits()], [a, b]);
            let t = Vector3::<f32>::new(f32::from_bits(b), f32::from_bits(a), f32::from_bits(b));
            assert_eq!([t.x.to_bits(), t.y.to_bits(), t.z.to_bits()], [b, a, b]);
        }
    }

    let narrow = Vector2::<u8>::new(u8::MAX, 0);
    let x: u8 = narrow.x;
    assert_eq!((x, narrow.y), (u8::MAX, 0));
    let signed = Vector3::<i16>::new(i16::MIN, -1, i16::MAX);
    assert_eq!((signed.x, signed.y, signed.z), (i16::MIN, -1, i16::MAX));

    // A non-Copy component type is still a VECTOR2<T>: Clone is available, Copy is not needed.
    let owned = Vector2::new(String::from("x"), String::from("y"));
    let cloned = owned.clone();
    assert_eq!((cloned.x.as_str(), cloned.y.as_str()), ("x", "y"));
    assert_eq!((owned.x.as_str(), owned.y.as_str()), ("x", "y"));
}

/// The aliases are the C++ typedefs (`VECTOR2D = VECTOR2<double>`, `VECTOR2I = VECTOR2<int32_t>`,
/// `VECTOR2L = VECTOR2<int64_t>`, `VECTOR3D = VECTOR3<double>`, `VECTOR3I = VECTOR3<int>`,
/// `VECTOR3U = VECTOR3<unsigned int>`): the same types, not wrappers, at both export paths.
#[test]
fn aliases_are_the_generic_types() {
    assert_eq!(TypeId::of::<Vector2D>(), TypeId::of::<Vector2<f64>>());
    assert_eq!(TypeId::of::<Vector2I>(), TypeId::of::<Vector2<i32>>());
    assert_eq!(TypeId::of::<Vector2L>(), TypeId::of::<Vector2<i64>>());
    assert_eq!(TypeId::of::<Vector3D>(), TypeId::of::<Vector3<f64>>());
    assert_eq!(TypeId::of::<Vector3I>(), TypeId::of::<Vector3<i32>>());
    assert_eq!(TypeId::of::<Vector3U>(), TypeId::of::<Vector3<u32>>());
    assert_ne!(TypeId::of::<Vector2I>(), TypeId::of::<Vector2L>());
    assert_ne!(TypeId::of::<Vector3I>(), TypeId::of::<Vector3U>());

    assert_eq!(
        TypeId::of::<vector::Vector2<i64>>(),
        TypeId::of::<Vector2<i64>>()
    );
    assert_eq!(
        TypeId::of::<vector::Vector3<u32>>(),
        TypeId::of::<Vector3<u32>>()
    );
    assert_eq!(TypeId::of::<vector::Vector2D>(), TypeId::of::<Vector2D>());
    assert_eq!(TypeId::of::<vector::Vector3U>(), TypeId::of::<Vector3U>());

    // Values move between alias and generic spellings without conversion.
    fn takes_generic(v: Vector2<f64>) -> Vector2<f64> {
        v
    }
    fn takes_alias(v: Vector3I) -> Vector3I {
        v
    }
    let d: Vector2D = takes_generic(Vector2D::new(-0.0, f64::from_bits(F64_BITS[1])));
    assert_eq!(bits2(&d), [0x8000_0000_0000_0000, F64_BITS[1]]);
    let i = takes_alias(Vector3::<i32>::new(i32::MIN, 0, i32::MAX));
    assert_eq!((i.x, i.y, i.z), (i32::MIN, 0, i32::MAX));
}
