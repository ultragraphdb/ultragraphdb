/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `hex.Dump`, called directly and reached through the key-size error that prints it. Every expected text is
//! Go's own output, from `fmt.Printf("%q", hex.Dump(…))`, or Go's `TestDump` in `src/encoding/hex/hex_test.go`.

use ultragraphdb_error_store::{StoreError, go_hex_dump};

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
    assert_eq!(go_hex_dump(b""), "");
}

#[test]
fn test_exported_dump_is_the_one_the_error_prints() {
    // Go 1.27: hex.Dump([]byte("\x00\x01key with a namespace")).
    let data = b"\x00\x01key with a namespace";
    let want = "00000000  00 01 6b 65 79 20 77 69  74 68 20 61 20 6e 61 6d  |..key with a nam|\n\
                00000010  65 73 70 61 63 65                                 |espace|\n";
    assert_eq!(go_hex_dump(data), want);
    assert_eq!(dump(data), want);
}

#[test]
fn test_dump_matches_gos_test_dump() {
    // Go's TestDump: forty bytes, byte i being i+30, and its expectedHexDump.
    let input: Vec<u8> = (0..40u8).map(|i| i + 30).collect();
    assert_eq!(
        go_hex_dump(&input),
        "00000000  1e 1f 20 21 22 23 24 25  26 27 28 29 2a 2b 2c 2d  |.. !\"#$%&'()*+,-|\n\
         00000010  2e 2f 30 31 32 33 34 35  36 37 38 39 3a 3b 3c 3d  |./0123456789:;<=|\n\
         00000020  3e 3f 40 41 42 43 44 45                           |>?@ABCDE|\n"
    );
}

#[test]
fn test_dump_matches_go_at_every_line_length_boundary() {
    // Go 1.27: hex.Dump of n bytes, byte i being 'A'+i: around the extra space after the eighth byte and around
    // the end of a sixteen-byte line.
    let cases: [(usize, &str); 7] = [
        (
            1,
            "00000000  41                                                |A|\n",
        ),
        (
            8,
            "00000000  41 42 43 44 45 46 47 48                           |ABCDEFGH|\n",
        ),
        (
            9,
            "00000000  41 42 43 44 45 46 47 48  49                       |ABCDEFGHI|\n",
        ),
        (
            15,
            "00000000  41 42 43 44 45 46 47 48  49 4a 4b 4c 4d 4e 4f     |ABCDEFGHIJKLMNO|\n",
        ),
        (
            16,
            "00000000  41 42 43 44 45 46 47 48  49 4a 4b 4c 4d 4e 4f 50  |ABCDEFGHIJKLMNOP|\n",
        ),
        (
            17,
            "00000000  41 42 43 44 45 46 47 48  49 4a 4b 4c 4d 4e 4f 50  |ABCDEFGHIJKLMNOP|\n\
             00000010  51                                                |Q|\n",
        ),
        (
            32,
            "00000000  41 42 43 44 45 46 47 48  49 4a 4b 4c 4d 4e 4f 50  |ABCDEFGHIJKLMNOP|\n\
             00000010  51 52 53 54 55 56 57 58  59 5a 5b 5c 5d 5e 5f 60  |QRSTUVWXYZ[\\]^_`|\n",
        ),
    ];
    for (n, want) in cases {
        let input: Vec<u8> = (0..n as u8).map(|i| b'A' + i).collect();
        assert_eq!(go_hex_dump(&input), want, "{n} bytes");
    }
}

#[test]
fn test_dump_prints_only_32_to_126_as_characters() {
    // Go 1.27: hex.Dump([]byte{0x1f, 0x20, 0x7e, 0x7f, 0x80, 0xff}).
    assert_eq!(
        go_hex_dump(&[0x1f, 0x20, 0x7e, 0x7f, 0x80, 0xff]),
        "00000000  1f 20 7e 7f 80 ff                                 |. ~...|\n"
    );
}

#[test]
fn test_dump_of_every_byte_value_matches_go() {
    // Go 1.27: hex.Dump of the 256 bytes 0x00..=0xff, one line each.
    let want = [
        "00000000  00 01 02 03 04 05 06 07  08 09 0a 0b 0c 0d 0e 0f  |................|\n",
        "00000010  10 11 12 13 14 15 16 17  18 19 1a 1b 1c 1d 1e 1f  |................|\n",
        "00000020  20 21 22 23 24 25 26 27  28 29 2a 2b 2c 2d 2e 2f  | !\"#$%&'()*+,-./|\n",
        "00000030  30 31 32 33 34 35 36 37  38 39 3a 3b 3c 3d 3e 3f  |0123456789:;<=>?|\n",
        "00000040  40 41 42 43 44 45 46 47  48 49 4a 4b 4c 4d 4e 4f  |@ABCDEFGHIJKLMNO|\n",
        "00000050  50 51 52 53 54 55 56 57  58 59 5a 5b 5c 5d 5e 5f  |PQRSTUVWXYZ[\\]^_|\n",
        "00000060  60 61 62 63 64 65 66 67  68 69 6a 6b 6c 6d 6e 6f  |`abcdefghijklmno|\n",
        "00000070  70 71 72 73 74 75 76 77  78 79 7a 7b 7c 7d 7e 7f  |pqrstuvwxyz{|}~.|\n",
        "00000080  80 81 82 83 84 85 86 87  88 89 8a 8b 8c 8d 8e 8f  |................|\n",
        "00000090  90 91 92 93 94 95 96 97  98 99 9a 9b 9c 9d 9e 9f  |................|\n",
        "000000a0  a0 a1 a2 a3 a4 a5 a6 a7  a8 a9 aa ab ac ad ae af  |................|\n",
        "000000b0  b0 b1 b2 b3 b4 b5 b6 b7  b8 b9 ba bb bc bd be bf  |................|\n",
        "000000c0  c0 c1 c2 c3 c4 c5 c6 c7  c8 c9 ca cb cc cd ce cf  |................|\n",
        "000000d0  d0 d1 d2 d3 d4 d5 d6 d7  d8 d9 da db dc dd de df  |................|\n",
        "000000e0  e0 e1 e2 e3 e4 e5 e6 e7  e8 e9 ea eb ec ed ee ef  |................|\n",
        "000000f0  f0 f1 f2 f3 f4 f5 f6 f7  f8 f9 fa fb fc fd fe ff  |................|\n",
    ]
    .concat();
    let input: Vec<u8> = (0..=255u8).collect();
    assert_eq!(go_hex_dump(&input), want);
}
