/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_traits_store::{
    MAX_STORED_KEY_SIZE, MAX_USER_KEY_SIZE, check_key_size, stored_key_size,
};

/// The parts of a `KeyTooLarge` refusal, or a failure naming the error that came instead.
fn too_large(err: &StoreError) -> (usize, usize, &[u8]) {
    match &err.0 {
        StoreErrorEnum::KeyTooLarge { size, limit, head } => (*size, *limit, head.as_slice()),
        other => panic!("expected KeyTooLarge, got {other:?}"),
    }
}

#[test]
fn test_the_limits_are_badgers_and_fjalls() {
    // Badger's txn.go:352 `const maxKeySize = 65000`; fjall writes a key's length as a u16.
    assert_eq!((MAX_USER_KEY_SIZE, MAX_STORED_KEY_SIZE), (65_000, 65_535));
}

#[test]
fn test_the_stored_size_counts_escapes_terminator_and_version() {
    // (user key, its length escaped plus the terminator's two bytes and the version's eight)
    let cases: &[(&[u8], usize)] = &[
        (b"", 10),
        (b"abc", 13),
        (&[0], 12),
        (&[0, 1, 0], 15),
        (&[0xFF], 11),
        (&[0xFF, 0x00, 0xFF], 14),
        (&[0; 5], 20),
    ];
    for &(key, expected) in cases {
        assert_eq!(stored_key_size(key), expected, "key {key:?}");
    }
}

#[test]
fn test_a_key_within_both_limits_is_accepted() {
    for key in [
        Vec::new(),
        vec![7],
        vec![7; MAX_USER_KEY_SIZE],
        vec![0xFF; MAX_USER_KEY_SIZE],
    ] {
        assert_eq!(check_key_size(&key), Ok(()), "key of {} bytes", key.len());
    }
}

#[test]
fn test_a_user_key_over_badgers_limit_is_refused_with_badgers_text() {
    let key: Vec<u8> = (0..MAX_USER_KEY_SIZE + 1)
        .map(|i| (i % 251) as u8)
        .collect();
    let err = check_key_size(&key).unwrap_err();
    let (size, limit, head) = too_large(&err);
    assert_eq!((size, limit), (65_001, 65_000));
    assert_eq!(head, &key[..1024]);

    // Badger's exceedsSize("Key", 65000, key) over this key, printed by Go 1.x from badger/v4@v4.9.6 txn.go:346.
    let text = err.to_string();
    let lines: Vec<&str> = text.split('\n').collect();
    assert_eq!((text.len(), lines.len()), (5103, 66));
    assert_eq!(
        lines[..4],
        [
            "Key with size 65001 exceeded 65000 limit. Key:",
            "00000000  00 01 02 03 04 05 06 07  08 09 0a 0b 0c 0d 0e 0f  |................|",
            "00000010  10 11 12 13 14 15 16 17  18 19 1a 1b 1c 1d 1e 1f  |................|",
            "00000020  20 21 22 23 24 25 26 27  28 29 2a 2b 2c 2d 2e 2f  | !\"#$%&'()*+,-./|",
        ]
    );
    // The printable column: 0x20 to 0x7e as themselves, 0x7f and above as dots.
    assert_eq!(
        lines[4..10],
        [
            "00000030  30 31 32 33 34 35 36 37  38 39 3a 3b 3c 3d 3e 3f  |0123456789:;<=>?|",
            "00000040  40 41 42 43 44 45 46 47  48 49 4a 4b 4c 4d 4e 4f  |@ABCDEFGHIJKLMNO|",
            "00000050  50 51 52 53 54 55 56 57  58 59 5a 5b 5c 5d 5e 5f  |PQRSTUVWXYZ[\\]^_|",
            "00000060  60 61 62 63 64 65 66 67  68 69 6a 6b 6c 6d 6e 6f  |`abcdefghijklmno|",
            "00000070  70 71 72 73 74 75 76 77  78 79 7a 7b 7c 7d 7e 7f  |pqrstuvwxyz{|}~.|",
            "00000080  80 81 82 83 84 85 86 87  88 89 8a 8b 8c 8d 8e 8f  |................|",
        ]
    );
    assert_eq!(
        lines[63..],
        [
            "000003e0  ef f0 f1 f2 f3 f4 f5 f6  f7 f8 f9 fa 00 01 02 03  |................|",
            "000003f0  04 05 06 07 08 09 0a 0b  0c 0d 0e 0f 10 11 12 13  |................|",
            "",
        ]
    );
}

#[test]
fn test_the_user_limit_is_checked_before_the_stored_one() {
    // 65,001 zero bytes are over both limits; the user key's refusal is Badger's and names the user length.
    let err = check_key_size(&vec![0; MAX_USER_KEY_SIZE + 1]).unwrap_err();
    let (size, limit, head) = too_large(&err);
    assert_eq!((size, limit), (65_001, 65_000));
    assert_eq!(head, &[0; 1024][..]);
}

#[test]
fn test_a_stored_key_over_fjalls_limit_is_refused_with_its_escaped_head() {
    let err = check_key_size(&vec![0; 40_000]).unwrap_err();
    let (size, limit, head) = too_large(&err);
    assert_eq!((size, limit), (80_010, MAX_STORED_KEY_SIZE));
    assert_eq!(head.len(), 1024);
    assert!(head.chunks(2).all(|pair| pair == [0x00, 0xFF]));
    assert!(err.to_string().starts_with(
        "Key with size 80010 exceeded 65535 limit. Key:\n\
             00000000  00 ff 00 ff 00 ff 00 ff  00 ff 00 ff 00 ff 00 ff  |................|\n"
    ));
}

#[test]
fn test_the_escaped_head_interleaves_escapes_with_the_other_bytes() {
    // 0x00 0x01 repeated: 43,684 bytes hold 21,842 zeros, so 65,526 escaped bytes and 65,536 stored.
    let key: Vec<u8> = (0..43_684).map(|i| (i % 2) as u8).collect();
    let err = check_key_size(&key).unwrap_err();
    let (size, limit, head) = too_large(&err);
    assert_eq!((size, limit), (65_536, MAX_STORED_KEY_SIZE));
    let expected: Vec<u8> = [0x00, 0xFF, 0x01]
        .iter()
        .copied()
        .cycle()
        .take(1024)
        .collect();
    assert_eq!(head, expected.as_slice());
    // One byte fewer drops a one, which is not escaped: exactly 65,535 stored, accepted.
    assert_eq!(stored_key_size(&key[..43_683]), MAX_STORED_KEY_SIZE);
    assert_eq!(check_key_size(&key[..43_683]), Ok(()));
}

#[test]
fn test_the_stored_limit_is_inclusive() {
    // 32,762 zero bytes and a one are 65,535 bytes stored; one zero byte more escapes past the limit.
    let mut key = vec![0; 32_762];
    key.push(1);
    assert_eq!(stored_key_size(&key), MAX_STORED_KEY_SIZE);
    assert_eq!(check_key_size(&key), Ok(()));
    let err = check_key_size(&vec![0; 32_763]).unwrap_err();
    let (size, limit, _) = too_large(&err);
    assert_eq!((size, limit), (65_536, MAX_STORED_KEY_SIZE));
}
