/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `hex.Dump`, reached through the key-size error that prints it.

use ultragraphdb_error_store::StoreError;

fn dump(data: &[u8]) -> String {
    let text = StoreError::KeyTooLarge(0, 0, data.to_vec()).to_string();
    text["Key with size 0 exceeded 0 limit. Key:\n".len()..].to_string()
}

#[test]
fn test_dump_matches_go_for_a_full_and_a_partial_line() {
    // Go 1.27: fmt.Printf("%q", hex.Dump([]byte("abc\x00\x7f~ 0123456789"))).
    assert_eq!(
        dump(b"abc\x00\x7f~ 0123456789"),
        "00000000  61 62 63 00 7f 7e 20 30  31 32 33 34 35 36 37 38  |abc..~ 012345678|\n\
         00000010  39                                                |9|\n"
    );
}

#[test]
fn test_dump_pads_a_line_shorter_than_eight_bytes() {
    // Go 1.27: hex.Dump([]byte("abcdefg")).
    assert_eq!(
        dump(b"abcdefg"),
        "00000000  61 62 63 64 65 66 67                              |abcdefg|\n"
    );
}

#[test]
fn test_dump_of_nothing_is_empty() {
    assert_eq!(dump(b""), "");
}

#[test]
fn test_exported_dump_is_the_one_the_error_prints() {
    let data = b"\x00\x01key with a namespace";
    assert_eq!(ultragraphdb_error_store::go_hex_dump(data), dump(data));
}
