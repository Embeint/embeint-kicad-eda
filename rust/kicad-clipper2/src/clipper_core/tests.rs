// SPDX-License-Identifier: GPL-3.0-or-later AND BSL-1.0
use super::*;

/// A fixed-seed generator (SplitMix64) for deterministic property tests.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn coordinate(&mut self) -> i64 {
        let raw = self.next() as i64;
        match self.next() % 4 {
            0 => raw >> 2,
            1 => raw >> 33,
            2 => (raw % 7) - 3,
            _ => raw >> 1,
        }
    }
}

#[test]
fn multiply_u64_is_the_exact_product() {
    let mut rng = Rng(1);
    for _ in 0..20_000 {
        let (a, b) = (rng.next(), rng.next());
        let product = u128::from(a) * u128::from(b);
        let words = multiply_u64(a, b);
        assert_eq!(words.lo, product as u64);
        assert_eq!(words.hi, (product >> 64) as u64);
    }
}

#[test]
fn portable_branches_agree_with_int128_branches() {
    let mut rng = Rng(2);
    for _ in 0..20_000 {
        let v: Vec<i64> = (0..4).map(|_| rng.coordinate()).collect();
        let portable = products_are_equal_without_int128(v[0], v[1], v[2], v[3]);
        if v.contains(&i64::MIN) {
            assert!(portable.is_err());
        } else {
            assert_eq!(portable, Ok(products_are_equal(v[0], v[1], v[2], v[3])));
        }
        let p: Vec<Point64> = (0..3)
            .map(|_| Point64::xy(rng.coordinate(), rng.coordinate()))
            .collect();
        let exact = cross_product_sign(&p[0], &p[1], &p[2]);
        let portable = cross_product_sign_without_int128(&p[0], &p[1], &p[2]);
        if let (Ok(exact), Ok(portable)) = (&exact, &portable) {
            assert_eq!(exact, portable);
        }
    }
    assert!(products_are_equal(3, 4, 2, 6));
    assert_eq!(products_are_equal_without_int128(-3, 4, 2, -6), Ok(true));
    assert_eq!(products_are_equal_without_int128(0, 4, 0, -6), Ok(true));
}

#[test]
fn conversions_refuse_undefined_values() {
    assert_eq!(i64::init_from(2.5), Ok(3));
    assert_eq!(i64::init_from(-2.5), Ok(-3));
    assert_eq!(i64::closest_offset(2.5), Ok(2));
    assert_eq!(i64::from_f64_cast(-2.9), Ok(-2));
    assert_eq!(
        i64::from_f64_cast(-9_223_372_036_854_775_808.0),
        Ok(i64::MIN)
    );
    assert!(i64::from_f64_cast(9_223_372_036_854_775_808.0).is_err());
    assert!(i64::from_f64_cast(f64::NAN).is_err());
    assert!(i64::init_from(f64::INFINITY).is_err());
    assert!(1i64.add(i64::MAX).is_err());
}

#[test]
fn area_and_point_in_polygon() {
    let square: Path64 = [(0, 0), (10, 0), (10, 10), (0, 10)]
        .iter()
        .map(|&(x, y)| Point64::xy(x, y))
        .collect();
    assert_eq!(area(&square), Ok(100.0));
    let triangle = &square[..3];
    assert_eq!(area(triangle), Ok(50.0));
    assert_eq!(
        point_in_polygon(&Point64::xy(5, 5), &square),
        Ok(PointInPolygonResult::IsInside)
    );
    assert_eq!(
        point_in_polygon(&Point64::xy(10, 5), &square),
        Ok(PointInPolygonResult::IsOn)
    );
    assert_eq!(
        point_in_polygon(&Point64::xy(11, 5), &square),
        Ok(PointInPolygonResult::IsOutside)
    );
}

#[test]
fn stream_insertion() {
    let mut state = StreamState::default();
    let path = vec![PointD::xy(1.5, -0.0), PointD::xy(1e20, 2.0)];
    assert_eq!(
        insert_path(&path, &mut state),
        Ok("1.5,-0,0, 1e+20,2,0\n".into())
    );
    let rect = Rect64::invalid_rect();
    assert_eq!(
        rect.insert(&mut state),
        Ok(
            "(9223372036854775807,9223372036854775807,-9223372036854775808,-9223372036854775808) "
                .into()
        )
    );
}

#[test]
fn exception_description_is_a_c_string() {
    // `std::string(const char*)` stops at the first NUL.
    assert_eq!(
        Clipper2Exception::new("a\0b").map(|e| e.what().len()),
        Ok(1)
    );
    assert_eq!(Clipper2Exception::new("\0").map(|e| e.what().len()), Ok(0));
    assert_eq!(
        Clipper2Exception::new("ab").map(|e| e.what() == "ab"),
        Ok(true)
    );
}

#[test]
fn strip_duplicates_keeps_the_first_of_each_run() {
    let mut path = vec![
        Point64 { x: 1, y: 1, z: 1 },
        Point64 { x: 1, y: 1, z: 2 },
        Point64 { x: 2, y: 2, z: 3 },
        Point64 { x: 1, y: 1, z: 4 },
    ];
    strip_duplicates_path(&mut path, true);
    assert_eq!(path.iter().map(|p| p.z).collect::<Vec<_>>(), vec![1, 3]);
}
