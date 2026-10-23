/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;

#[test]
fn test_too_old_is_dgraphs_text() {
    assert_eq!(StoreError::TsTooOld().to_string(), "Transaction is too old");
}

#[test]
fn test_key_too_large_is_badgers_text() {
    // The text Go 1.27 prints for Badger's exceedsSize("Key", 65000, key) over a 65,001-byte key whose byte i is
    // i*7: the size and limit, then hex.Dump of the first 1,024 bytes, 64 lines of 16.
    let key: Vec<u8> = (0..65_001usize).map(|i| (i * 7) as u8).collect();
    let text = StoreError::KeyTooLarge(key.len(), 65_000, key[..1024].to_vec()).to_string();
    assert!(text.starts_with(
        "Key with size 65001 exceeded 65000 limit. Key:\n00000000  00 07 0e 15 1c 23 2a 31  38 3f 46 4d 54 5b \
         62 69  |.....#*18?FMT[bi|\n00000010  70 77 7e 85 8c 93 9a a1  a8 af b6 bd c4 cb d2 d9  |pw~........."
    ));
    assert!(text.ends_with(
        "000003f0  90 97 9e a5 ac b3 ba c1  c8 cf d6 dd e4 eb f2 f9  |................|\n"
    ));
    assert_eq!(text.len(), 5103);
}

#[test]
fn test_read_failures_name_their_key_in_hex() {
    assert_eq!(
        StoreError::ReadFailed(vec![0x00, 0xab, 0x10], "block checksum mismatch".into())
            .to_string(),
        "Read of key 00ab10 failed: block checksum mismatch"
    );
    assert_eq!(
        StoreError::Corrupted(vec![0x04, 0xff], "value too short".into()).to_string(),
        "Corrupted value of key 04ff: value too short"
    );
}

#[test]
fn test_write_failures_say_the_store_must_be_reopened() {
    assert_eq!(
        StoreError::WriteFailed("injected".into()).to_string(),
        "Write to the store failed: injected"
    );
    assert_eq!(
        StoreError::Poisoned().to_string(),
        "The store is poisoned by an earlier failed write and must be reopened"
    );
}

#[test]
fn test_open_failures_give_their_reason() {
    assert_eq!(
        StoreError::OpenFailed("locked".into()).to_string(),
        "Opening the store failed: locked"
    );
}
