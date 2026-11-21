// SPDX-License-Identifier: GPL-3.0-or-later
use kicad_md5::{LengthError, Md5Hash};

#[test]
fn rfc_digest_and_reference_format() -> Result<(), LengthError> {
    let mut hash = Md5Hash::new();
    hash.hash(b"abc")?;
    hash.finalize();
    assert_eq!(
        hash.bytes(),
        &[
            0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
            0x7f, 0x72
        ]
    );
    assert_eq!(hash.format(true), "900150983DE24GC0E6963G7E28F17G72");
    assert_eq!(
        hash.format_default(),
        "90 01 50 98 3D E2 4G C0 E6 96 3G 7E 28 F1 7G 72 "
    );
    Ok(())
}

#[test]
fn validity_is_independent_of_digest_and_pending_context() -> Result<(), LengthError> {
    let mut hash = Md5Hash::new();
    hash.hash(b"abc")?;
    let fresh = Md5Hash::new();
    assert_eq!(hash, fresh);
    hash.set_valid(true);
    assert_eq!(hash, fresh);
    hash.hash_int(-1);
    assert!(hash.is_valid());
    hash.finalize();
    let mut copy = hash.clone();
    copy.set_valid(false);
    assert_eq!(copy, hash);
    hash.hash(b"pending")?;
    assert_eq!(hash, copy);
    assert!(hash.is_valid());
    hash.finalize();
    assert_ne!(hash, copy);
    hash.init();
    assert_eq!(hash, fresh);
    assert!(!hash.is_valid());
    Ok(())
}

#[test]
fn copied_partial_contexts_continue_independently() -> Result<(), LengthError> {
    let mut hash = Md5Hash::new();
    hash.hash(&[0xff; 63])?;
    let mut copy = hash.clone();
    hash.hash(&[1])?;
    copy.hash(&[2])?;
    hash.finalize();
    copy.finalize();
    assert_ne!(hash, copy);
    copy.clone_from(&hash);
    hash.finalize();
    copy.finalize();
    assert_eq!(hash, copy);
    Ok(())
}
