// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_hash::{
    fmix64, hash_bytes, hash_combine, hash_val, mmh3_rotl64, BlockReader, CxxNumericGrouping,
    DomainError, GroupingTooLong, Hash128, Md5Hash, Mmh3Error, Mmh3Hash, StdHash, Transform,
    STD_HASH_SEED,
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
fn hash_128_grouping_follows_libstdcxx_num_put() -> std::result::Result<(), GroupingTooLong> {
    let hash = Hash128::from_value64([0x1234_5678_9abc_def0, 0x1234]);
    assert_eq!(
        hash.to_string_in(&CxxNumericGrouping::classic()),
        hash.to_hex_string().into_bytes()
    );
    // The reviewer's macOS reproduction with numpunct grouping "\3" and '_'.
    let grouped = CxxNumericGrouping::new(b'_', &[3])?;
    assert_eq!(
        hash.to_string_in(&grouped),
        b"1_234_567_89A_BCD_EF0000000000001_234".to_vec()
    );
    // Grouping that starts with 0, a negative char or CHAR_MAX is not used.
    for first in [0_u8, 0x80, 0xFF, 127] {
        let unused = CxxNumericGrouping::new(b'_', &[first, 2])?;
        assert_eq!(
            hash.to_string_in(&unused),
            hash.to_hex_string().into_bytes()
        );
    }
    // A later non-positive group stops grouping; the last group repeats.
    let mixed = CxxNumericGrouping::new(b'.', &[1, 2, 0])?;
    assert_eq!(
        Hash128::from_value64([0xABCDEF, 0]).to_string_in(&mixed),
        b"00000000ABC.DE.F0000000000000000".to_vec()
    );
    let repeat = CxxNumericGrouping::new(0xC3, &[4])?;
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
    assert_eq!(
        CxxNumericGrouping::new(b',', &[1; 65]),
        Err(GroupingTooLong(65))
    );
    Ok(())
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
