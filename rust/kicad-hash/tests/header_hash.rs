// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_hash::{
    fmix64, hash_bytes, hash_combine, hash_val, mmh3_rotl64, BlockReader, CxxNumericGrouping,
    DomainError, Hash128, Md5Hash, Mmh3Error, Mmh3Hash, StdHash, Transform, STD_HASH_SEED,
};

type Result = std::result::Result<(), Mmh3Error>;

#[test]
fn mmh3_matches_murmurhash3_x64_128_reference_vectors() -> Result {
    // MurmurHash3_x64_128 with seed 0: empty input is all zero; "foo" is the
    // widely published (h1, h2) = (0xe271865701f54561, 0x7eaf87e42bba7d87).
    let mut empty = Mmh3Hash::new();
    assert_eq!(empty.digest()?.value64(), [0, 0]);
    let mut foo = Mmh3Hash::new();
    foo.add_string(b"foo")?;
    let digest = foo.digest()?;
    assert_eq!(
        digest.value64(),
        [0xe271_8657_01f5_4561, 0x7eaf_87e4_2bba_7d87]
    );
    assert_eq!(digest.to_hex_string(), "E271865701F545617EAF87E42BBA7D87");
    Ok(())
}

#[test]
fn add_data_streams_in_pieces_of_any_size() -> Result {
    let data: Vec<u8> = (0..=200_u8).collect();
    let mut whole = Mmh3Hash::with_seed(0xA1B2_C3D4);
    whole.add_data(&data)?;
    for split in [1, 3, 15, 16, 17, 31, 32, 33, 100] {
        let mut pieces = Mmh3Hash::with_seed(0xA1B2_C3D4);
        for chunk in data.chunks(split) {
            pieces.add_data(chunk)?;
        }
        assert_eq!(pieces, whole, "split {split}");
    }
    assert_eq!(whole.len(), 201);
    Ok(())
}

#[test]
fn add_i32_slots_follow_len_not_byte_position() -> Result {
    let mut hash = Mmh3Hash::new();
    hash.add_data(&[0xAA; 13])?;
    hash.add_i32(0x0102_0304)?;
    // Slot (13 % 16) / 4 = 3 overwrites bytes 12..15; len 17 is not a block
    // boundary, so the block is not hashed.
    assert_eq!(hash.len(), 17);
    assert_eq!(
        &hash.blocks()[12..],
        &[Some(0x04), Some(0x03), Some(0x02), Some(0x01)]
    );
    assert_eq!((hash.h1(), hash.h2()), (0, 0));
    Ok(())
}

#[test]
fn add_data_v1_pads_to_words_and_counts_padding() {
    let mut hash = Mmh3Hash::new();
    hash.add_data_v1(&[1, 2, 3, 4]);
    // An aligned tail still gets four bytes of padding; only bytes 0..8 are written.
    assert_eq!(hash.len(), 8);
    assert_eq!(hash.blocks()[7], Some(0));
    assert_eq!(hash.blocks()[8], None);
    let mut odd = Mmh3Hash::new();
    odd.add_data_v1(&[9; 13]);
    assert_eq!(odd.len(), 16);
    assert_eq!((odd.h1(), odd.h2()), (0, 0));
}

#[test]
fn constructors_leave_every_block_byte_unwritten() {
    for hash in [Mmh3Hash::new(), Mmh3Hash::with_seed(7)] {
        assert_eq!(hash.blocks(), [None; 16]);
        assert_eq!(hash.len(), 0);
    }
}

#[test]
fn unwritten_reads_are_refused_without_changing_state() -> Result {
    // add( int32_t ) writes bytes 0..3; addDataV1 of one byte rewrites 0..3 and
    // counts len 8, so digest's hashTail would read the never-written bytes 4..7.
    let mut hash = Mmh3Hash::new();
    hash.add_i32(1)?;
    hash.add_data_v1(&[0xab]);
    let before = hash.clone();
    assert_eq!(
        hash.digest(),
        Err(Mmh3Error::IndeterminateRead {
            reader: BlockReader::HashTail,
            unwritten: 0x00F0
        })
    );
    assert_eq!(hash, before);
    assert_eq!(
        hash.get_block64(1).map_err(|error| error.to_string()),
        Err("getblock64 reads unwritten block bytes 8,9,10,11,12,13,14,15".to_owned())
    );
    assert!(hash.hash_block().is_err());
    assert_eq!(hash, before);

    // Completing a block through add( int32_t ) needs every byte written.
    let mut words = Mmh3Hash::new();
    words.add_data_v1(&[1, 2, 3, 4, 5]); // writes 0..8, len 8
    words.add_i32(9)?; // slot 2, len 12
    let before = words.clone();
    assert_eq!(
        words.add_i32(10),
        Ok(()),
        "slot 3 completes a fully written block"
    );
    assert_ne!(words, before);

    // addData completing a pending block whose earlier bytes were never written.
    let mut pending = Mmh3Hash::new();
    pending.add_i32(5)?; // bytes 0..4, len 4
    pending.add_data_v1(&[1]); // bytes 0..4 again, len 8; bytes 4..8 unwritten
    let before = pending.clone();
    assert_eq!(
        pending.add_data(&[0; 8]),
        Err(Mmh3Error::IndeterminateRead {
            reader: BlockReader::HashBlock,
            unwritten: 0x00F0
        })
    );
    assert_eq!(pending, before);
    Ok(())
}

#[test]
fn len_wraps_as_uint32() -> Result {
    let mut hash = Mmh3Hash::from_state(1, 2, [0; 16], u32::MAX - 3);
    hash.add_i32(-1)?;
    assert_eq!(hash.len(), 0);
    assert_ne!((hash.h1(), hash.h2()), (1, 2));
    Ok(())
}

#[test]
fn reset_keeps_block_bytes() -> Result {
    let mut hash = Mmh3Hash::new();
    hash.add_data(&[7; 5])?;
    hash.reset(9);
    assert_eq!((hash.h1(), hash.h2(), hash.len()), (9, 9, 0));
    assert_eq!(hash.blocks()[..5], [Some(7); 5]);
    hash.reset_default();
    assert_eq!((hash.h1(), hash.h2()), (0, 0));
    Ok(())
}

#[test]
fn checked_domains() {
    assert_eq!(mmh3_rotl64(1, 1), Ok(2));
    assert_eq!(mmh3_rotl64(1 << 63, 63), Ok(1 << 62));
    assert_eq!(mmh3_rotl64(1, 0), Err(DomainError::RotateCount(0)));
    assert_eq!(mmh3_rotl64(1, 64), Err(DomainError::RotateCount(64)));
    assert_eq!(mmh3_rotl64(1, -1), Err(DomainError::RotateCount(-1)));
    let hash = Mmh3Hash::from_state(0, 0, [1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0], 0);
    assert_eq!(hash.get_block64(0), Ok(1));
    assert_eq!(hash.get_block64(1), Ok(2));
    assert_eq!(
        hash.get_block64(2),
        Err(Mmh3Error::Domain(DomainError::BlockIndex(2)))
    );
    assert_eq!(fmix64(0), 0);
}

#[test]
fn hash_128_grouping_follows_libstdcxx_num_put() {
    let hash = Hash128::from_value64([0x1234_5678_9abc_def0, 0x1234]);
    assert_eq!(
        hash.to_string_in(&CxxNumericGrouping::classic()),
        hash.to_hex_string().into_bytes()
    );
    // The reviewer's macOS reproduction with numpunct grouping "\3" and '_'.
    let grouped = CxxNumericGrouping::new(b'_', &[3]);
    assert_eq!(
        hash.to_string_in(&grouped),
        b"1_234_567_89A_BCD_EF0000000000001_234".to_vec()
    );
    // Grouping that starts with 0, a negative char or CHAR_MAX is not used.
    for first in [0_u8, 0x80, 0xFF, 127] {
        let unused = CxxNumericGrouping::new(b'_', &[first, 2]);
        assert_eq!(
            hash.to_string_in(&unused),
            hash.to_hex_string().into_bytes()
        );
    }
    // A later non-positive group stops grouping; the last group repeats.
    let mixed = CxxNumericGrouping::new(b'.', &[1, 2, 0]);
    assert_eq!(
        Hash128::from_value64([0xABCDEF, 0]).to_string_in(&mixed),
        b"00000000ABC.DE.F0000000000000000".to_vec()
    );
    let repeat = CxxNumericGrouping::new(0xC3, &[4]);
    assert_eq!(
        Hash128::from_value64([u64::MAX, 1]).to_string_in(&repeat),
        [
            b"FFFF".as_slice(),
            &[0xC3],
            b"FFFF",
            &[0xC3],
            b"FFFF",
            &[0xC3],
            b"FFFF",
            b"0000000000000001"
        ]
        .concat()
    );
    // numpunct accepts a grouping of any length; a 64-bit word has at most 16 digits, so
    // only the leading bytes are ever consulted and the rest are ignored, not refused.
    let mut long = vec![1_u8; 15];
    long.extend_from_slice(&[0; 4000]);
    let every = CxxNumericGrouping::new(b'-', &[1; 4096]);
    assert_eq!(
        Hash128::from_value64([u64::MAX, 0]).to_string_in(&every),
        [
            b"F-F-F-F-F-F-F-F-F-F-F-F-F-F-F-F".as_slice(),
            b"0000000000000000"
        ]
        .concat()
    );
    assert_eq!(
        Hash128::from_value64([u64::MAX, 0]).to_string_in(&CxxNumericGrouping::new(b'-', &long)),
        [
            b"F-F-F-F-F-F-F-F-F-F-F-F-F-F-F-F".as_slice(),
            b"0000000000000000"
        ]
        .concat()
    );
}

mod bezier {
    use kicad_hash::{Bezier, BezierPoly};
    use kicad_math::{Vector2, Vector2D, Vector2I, VectorDomainError};

    fn bits(point: Vector2D) -> (u64, u64) {
        (point.x.to_bits(), point.y.to_bits())
    }

    fn curve_bits(curve: &Bezier<f64>) -> [(u64, u64); 4] {
        [
            bits(curve.start),
            bits(curve.c1),
            bits(curve.c2),
            bits(curve.end),
        ]
    }

    fn s_curve() -> Bezier<f64> {
        Bezier::new(
            Vector2::new(0.0, 0.0),
            Vector2::new(1.0, 2.0),
            Vector2::new(3.0, -2.0),
            Vector2::new(4.0, 0.0),
        )
    }

    #[test]
    fn constructor_stores_every_bit_pattern() {
        let nan = f64::from_bits(0x7ff0_0000_0000_0005);
        let curve = Bezier::new(
            Vector2::new(nan, -0.0),
            Vector2::new(f64::INFINITY, 5e-324),
            Vector2::new(f64::MAX, 1.0),
            Vector2::new(-1.0, f64::NEG_INFINITY),
        );
        assert_eq!(curve.start.x.to_bits(), 0x7ff0_0000_0000_0005);
        assert_eq!(curve.start.y.to_bits(), 0x8000_0000_0000_0000);
        let ints = Bezier::new(
            Vector2::new(i32::MIN, i32::MAX),
            Vector2::new(0, -1),
            Vector2::new(1, 2),
            Vector2::new(3, 4),
        );
        assert_eq!((ints.start.x, ints.start.y), (i32::MIN, i32::MAX));
    }

    #[test]
    fn point_at_is_the_bernstein_sum_in_order() -> Result<(), VectorDomainError> {
        let curve = s_curve();
        assert_eq!(bits(curve.point_at(0.0)?), bits(Vector2::new(0.0, 0.0)));
        assert_eq!(bits(curve.point_at(1.0)?), bits(Vector2::new(4.0, 0.0)));
        // t = 0.5: 0.125 * 0 + 0.375 * 1 + 0.375 * 3 + 0.125 * 4 = 2, and y = 0.
        assert_eq!(bits(curve.point_at(0.5)?), bits(Vector2::new(2.0, 0.0)));
        // Outside [0, 1] the formula is evaluated as given: at t = 2,
        // x = -1 * 0 + 6 * 1 + -12 * 3 + 8 * 4 = 2 and y = 6 * 2 + -12 * -2 = 36.
        assert_eq!(bits(curve.point_at(2.0)?), bits(Vector2::new(2.0, 36.0)));
        Ok(())
    }

    #[test]
    fn int_point_at_promotes_then_truncates_and_saturates() -> Result<(), VectorDomainError> {
        let odd: Bezier<i32> = Bezier::new(
            Vector2::new(-7, 7),
            Vector2::new(3, -3),
            Vector2::new(-1, 11),
            Vector2::new(9, -5),
        );
        // x = 0.125 * -7 + 0.375 * 3 + 0.375 * -1 + 0.125 * 9 = 1.0;
        // y = 0.125 * 7 + 0.375 * -3 + 0.375 * 11 + 0.125 * -5 = 3.25 -> 3 (truncated).
        let mid = odd.point_at(0.5)?;
        assert_eq!((mid.x, mid.y), (1, 3));
        // A negative fraction truncates toward zero.
        let negative: Bezier<i32> = Bezier::new(
            Vector2::new(-3, -3),
            Vector2::new(-3, -3),
            Vector2::new(-3, -3),
            Vector2::new(-2, -2),
        );
        let point = negative.point_at(0.5)?;
        assert_eq!((point.x, point.y), (-2, -2)); // -2.875 -> -2
                                                  // Signs that make every term +inf saturate at INT_MAX instead of overflowing.
        let saturating: Bezier<i32> = Bezier::new(
            Vector2::new(-1, -1),
            Vector2::new(1, 1),
            Vector2::new(-1, -1),
            Vector2::new(1, 1),
        );
        let point = saturating.point_at(1e200)?;
        assert_eq!((point.x, point.y), (i32::MAX, i32::MAX));
        let point = saturating.point_at(-1e200)?;
        assert_eq!((point.x, point.y), (i32::MIN, i32::MIN));
        Ok(())
    }

    #[test]
    fn int_undefined_evaluations_are_refused() {
        let curve: Bezier<i32> = Bezier::new(
            Vector2::new(0, 0),
            Vector2::new(1, 1),
            Vector2::new(2, 2),
            Vector2::new(3, 3),
        );
        assert_eq!(
            curve.point_at(f64::NAN).err(),
            Some(VectorDomainError::NanToInteger)
        );
        let extremes: Bezier<i32> = Bezier::new(
            Vector2::new(i32::MIN, 0),
            Vector2::new(i32::MAX, 0),
            Vector2::new(0, 0),
            Vector2::new(0, 0),
        );
        // C1 - Start overflows an int.
        assert_eq!(
            extremes.split(0.5).err(),
            Some(VectorDomainError::IntegerOverflow)
        );
        // The first level saturates at INT_MAX; inf * 0 at the second level is NaN.
        assert_eq!(
            curve.split(f64::INFINITY).err(),
            Some(VectorDomainError::NanToInteger)
        );
    }

    #[test]
    fn split_is_one_de_casteljau_step() -> Result<(), VectorDomainError> {
        let (left, right) = s_curve().split(0.5)?;
        assert_eq!(
            curve_bits(&left),
            [
                bits(Vector2::new(0.0, 0.0)),
                bits(Vector2::new(0.5, 1.0)),
                bits(Vector2::new(1.25, 0.5)),
                bits(Vector2::new(2.0, 0.0)),
            ]
        );
        assert_eq!(
            curve_bits(&right),
            [
                bits(Vector2::new(2.0, 0.0)),
                bits(Vector2::new(2.75, -0.5)),
                bits(Vector2::new(3.5, -1.0)),
                bits(Vector2::new(4.0, 0.0)),
            ]
        );
        // The int instantiation stores each intermediate (truncated) before using it.
        let ints: Bezier<i32> = Bezier::new(
            Vector2::new(0, 0),
            Vector2::new(1, 1),
            Vector2::new(2, -1),
            Vector2::new(3, 0),
        );
        let (left, right) = ints.split(0.5)?;
        let pairs = |curve: &Bezier<i32>| {
            [curve.start, curve.c1, curve.c2, curve.end].map(|p: Vector2I| (p.x, p.y))
        };
        // startC1 = (0.5, 0.5) -> (0, 0), c1C2 = (1.5, 0) -> (1, 0),
        // c2End = (2.5, -0.5) -> (2, 0), leftC2 = (0.5, 0) -> (0, 0),
        // rightC1 = (1.5, 0) -> (1, 0), shared = (0.5, 0) -> (0, 0).
        assert_eq!(pairs(&left), [(0, 0), (0, 0), (0, 0), (0, 0)]);
        assert_eq!(pairs(&right), [(0, 0), (1, 0), (2, 0), (3, 0)]);
        Ok(())
    }

    #[test]
    fn sub_curve_branches() -> Result<(), VectorDomainError> {
        let curve = s_curve();
        // Full range, also after clamping and with -0.0.
        for (t0, t1) in [(0.0, 1.0), (-1.0, 2.0), (-0.0, 1.0)] {
            assert_eq!(curve_bits(&curve.sub_curve(t0, t1)?), curve_bits(&curve));
        }
        // t0 == 0 and t1 == 1 take the left and right halves of a single split.
        let (left, right) = curve.split(0.5)?;
        assert_eq!(curve_bits(&curve.sub_curve(0.0, 0.5)?), curve_bits(&left));
        assert_eq!(curve_bits(&curve.sub_curve(0.5, 1.0)?), curve_bits(&right));
        // An interior range splits at t1, then the left part at t0 / t1.
        let (first, _) = curve.split(0.75)?;
        let (_, inner) = first.split(0.25 / 0.75)?;
        assert_eq!(
            curve_bits(&curve.sub_curve(0.25, 0.75)?),
            curve_bits(&inner)
        );
        // An empty or reversed range is the degenerate curve at PointAt( t0 ).
        let point = curve.point_at(0.75)?;
        let degenerate = curve.sub_curve(0.75, 0.25)?;
        assert_eq!(curve_bits(&degenerate), [bits(point); 4]);
        let clamped = curve.sub_curve(1.5, 2.0)?;
        assert_eq!(curve_bits(&clamped), [bits(curve.point_at(1.0)?); 4]);
        // A NaN passes the clamps: (NaN, 0.5) splits at 0.5, then the left half at NaN / 0.5;
        // the right part keeps the finite split point as its end.
        let nan = f64::from_bits(0x7ff8_0000_0000_00c3);
        let nan_curve = curve.sub_curve(nan, 0.5)?;
        assert_eq!(nan_curve.start.x.to_bits(), 0x7ff8_0000_0000_00c3);
        assert_eq!(bits(nan_curve.end), bits(left.end));
        Ok(())
    }

    #[test]
    fn nan_results_follow_the_oracle_platform() -> Result<(), VectorDomainError> {
        let curve = s_curve();
        // inf - inf generates x86's negative indefinite NaN; a NaN input keeps its payload.
        let point = curve.point_at(f64::INFINITY)?;
        assert_eq!(point.x.to_bits(), 0xfff8_0000_0000_0000);
        let point = curve.point_at(f64::from_bits(0x7ff0_0000_0000_0009))?;
        assert_eq!(point.x.to_bits(), 0x7ff8_0000_0000_0009);
        Ok(())
    }

    #[test]
    fn bezier_poly_copies_control_points_and_zeroes_min_seg_len() {
        let nan = f64::from_bits(0xfff8_0000_0000_00a5);
        let points = [Vector2::new(nan, -0.0), Vector2::new(1.0, 2.0)];
        let poly = BezierPoly::from_vector2d(&points);
        assert_eq!(poly.min_seg_len().to_bits(), 0);
        assert_eq!(poly.ctrl_pts().len(), 2);
        assert_eq!(poly.ctrl_pts()[0].x.to_bits(), 0xfff8_0000_0000_00a5);
        assert_eq!(poly.ctrl_pts()[0].y.to_bits(), 0x8000_0000_0000_0000);
        assert!(BezierPoly::from_vector2d(&[]).ctrl_pts().is_empty());
    }
}

#[test]
fn hash_128_views_clear_and_format() {
    let mut hash = Hash128::from_value64([0x0123_4567_89AB_CDEF, 0xF]);
    assert_eq!(hash.to_hex_string(), "0123456789ABCDEF000000000000000F");
    assert_eq!(hash.value32(), [0x89AB_CDEF, 0x0123_4567, 0xF, 0]);
    assert_eq!(hash.value8()[0], 0xEF);
    assert!(!hash.equals(&Hash128::new()));
    hash.clear();
    assert!(hash.equals(&Hash128::new()));
    assert_eq!(hash.to_string(), "0".repeat(32));
}

#[test]
fn std_hash_and_hash_combine_follow_libstdcxx() {
    assert_eq!((-1_i32).std_hash(), u64::MAX);
    assert_eq!(true.std_hash(), 1);
    assert_eq!(0.0_f64.std_hash(), 0);
    assert_eq!((-0.0_f64).std_hash(), 0);
    assert_eq!(
        1.5_f64.std_hash(),
        hash_bytes(&1.5_f64.to_le_bytes(), STD_HASH_SEED)
    );
    assert_eq!("abc".std_hash(), b"abc"[..].std_hash());

    let mut seed = 5_u64;
    hash_combine(&mut seed, &[]);
    assert_eq!(seed, 5);
    hash_combine(&mut seed, &[&7_i32]);
    assert_eq!(seed, 5 ^ (7 + 0x9e37_79b9 + (5 << 6) + (5 >> 2)));

    let mut chained = 0xa82d_e1c0_u64;
    hash_combine(&mut chained, &[&1_i32]);
    hash_combine(&mut chained, &[&"x"]);
    assert_eq!(hash_val(&[&1_i32, &"x"]), chained);
    assert_eq!(hash_val(&[]), 0xa82d_e1c0);
}

#[test]
fn transform_constructors_and_inequality() {
    let normal = Transform::default();
    assert_eq!((normal.x1, normal.y1, normal.x2, normal.y2), (1, 0, 0, 1));
    assert!(!(normal != Transform::new(1, 0, 0, 1)));
    assert!(normal != Transform::new(1, 0, 0, -1));
    assert!(Transform::new(0, 1, -1, 0) != Transform::new(0, -1, 1, 0));
}

#[test]
fn md5_validity_accessors_are_reused() {
    let mut hash = Md5Hash::new();
    assert!(!hash.is_valid());
    hash.set_valid(true);
    assert!(hash.is_valid());
    hash.init();
    assert!(!hash.is_valid());
    hash.finalize();
    assert!(hash.is_valid());
}
