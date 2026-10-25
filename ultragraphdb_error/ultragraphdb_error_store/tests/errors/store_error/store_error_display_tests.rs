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
    // i*7: the size and limit, then hex.Dump of the first 1,024 bytes, 64 lines of 16. Byte i*7 repeats every 256
    // bytes, so Go's line k is its line k % 16 at offset 16*k; Go confirms that, and these are its sixteen lines
    // after the offset.
    const BODY: [&str; 16] = [
        "  00 07 0e 15 1c 23 2a 31  38 3f 46 4d 54 5b 62 69  |.....#*18?FMT[bi|\n",
        "  70 77 7e 85 8c 93 9a a1  a8 af b6 bd c4 cb d2 d9  |pw~.............|\n",
        "  e0 e7 ee f5 fc 03 0a 11  18 1f 26 2d 34 3b 42 49  |..........&-4;BI|\n",
        "  50 57 5e 65 6c 73 7a 81  88 8f 96 9d a4 ab b2 b9  |PW^elsz.........|\n",
        "  c0 c7 ce d5 dc e3 ea f1  f8 ff 06 0d 14 1b 22 29  |..............\")|\n",
        "  30 37 3e 45 4c 53 5a 61  68 6f 76 7d 84 8b 92 99  |07>ELSZahov}....|\n",
        "  a0 a7 ae b5 bc c3 ca d1  d8 df e6 ed f4 fb 02 09  |................|\n",
        "  10 17 1e 25 2c 33 3a 41  48 4f 56 5d 64 6b 72 79  |...%,3:AHOV]dkry|\n",
        "  80 87 8e 95 9c a3 aa b1  b8 bf c6 cd d4 db e2 e9  |................|\n",
        "  f0 f7 fe 05 0c 13 1a 21  28 2f 36 3d 44 4b 52 59  |.......!(/6=DKRY|\n",
        "  60 67 6e 75 7c 83 8a 91  98 9f a6 ad b4 bb c2 c9  |`gnu|...........|\n",
        "  d0 d7 de e5 ec f3 fa 01  08 0f 16 1d 24 2b 32 39  |............$+29|\n",
        "  40 47 4e 55 5c 63 6a 71  78 7f 86 8d 94 9b a2 a9  |@GNU\\cjqx.......|\n",
        "  b0 b7 be c5 cc d3 da e1  e8 ef f6 fd 04 0b 12 19  |................|\n",
        "  20 27 2e 35 3c 43 4a 51  58 5f 66 6d 74 7b 82 89  | '.5<CJQX_fmt{..|\n",
        "  90 97 9e a5 ac b3 ba c1  c8 cf d6 dd e4 eb f2 f9  |................|\n",
    ];
    let key: Vec<u8> = (0..65_001usize).map(|i| (i * 7) as u8).collect();
    let text = StoreError::KeyTooLarge(key.len(), 65_000, key[..1024].to_vec()).to_string();
    let want: String =
        std::iter::once("Key with size 65001 exceeded 65000 limit. Key:\n".to_string())
            .chain((0..64).map(|k| format!("{:08x}{}", k * 16, BODY[k % 16])))
            .collect();
    assert_eq!(text, want);
    assert_eq!(text.len(), 5103);
}

#[test]
fn test_key_too_large_dumps_a_head_shorter_than_badgers() {
    // fjall's limit: a stored key of 65,536 bytes, whose head here is the three bytes the caller kept. Go's
    // hex.Dump([]byte{0x00, 0x01, 0x41}) for the dump.
    assert_eq!(
        StoreError::KeyTooLarge(65_536, 65_535, vec![0x00, 0x01, 0x41]).to_string(),
        "Key with size 65536 exceeded 65535 limit. Key:\n\
         00000000  00 01 41                                          |..A|\n"
    );
    assert_eq!(
        StoreError::KeyTooLarge(usize::MAX, 0, Vec::new()).to_string(),
        format!("Key with size {} exceeded 0 limit. Key:\n", usize::MAX)
    );
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
fn test_keys_print_as_gos_percent_x() {
    // Go's fmt.Sprintf("%x", key): two lowercase digits a byte, nothing for an empty key.
    let cases: [(&[u8], &str); 4] = [
        (b"", ""),
        (&[0x00], "00"),
        (&[0x0f, 0xf0, 0xff], "0ff0ff"),
        (b"\x01k\xab", "016bab"),
    ];
    for (key, want) in cases {
        assert_eq!(
            StoreError::ReadFailed(key.to_vec(), "r".into()).to_string(),
            format!("Read of key {want} failed: r")
        );
        assert_eq!(
            StoreError::Corrupted(key.to_vec(), "c".into()).to_string(),
            format!("Corrupted value of key {want}: c")
        );
    }
}

#[test]
fn test_reasons_are_printed_verbatim() {
    assert_eq!(
        StoreError::ReadFailed(vec![0x01], String::new()).to_string(),
        "Read of key 01 failed: "
    );
    assert_eq!(
        StoreError::Corrupted(Vec::new(), "längd ≠ 8: {}".into()).to_string(),
        "Corrupted value of key : längd ≠ 8: {}"
    );
    assert_eq!(
        StoreError::WriteFailed("disk\nfull".into()).to_string(),
        "Write to the store failed: disk\nfull"
    );
    assert_eq!(
        StoreError::OpenFailed(String::new()).to_string(),
        "Opening the store failed: "
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

#[test]
fn test_an_unsupported_operation_names_itself() {
    assert_eq!(
        StoreError::Unsupported("rebuild records").to_string(),
        "The store does not support rebuild records"
    );
}

#[test]
fn test_an_unknown_temporary_keyspace_names_its_number() {
    assert_eq!(
        StoreError::UnknownTemporary(7).to_string(),
        "Temporary keyspace 7 does not exist: it was dropped, or the store was reopened since it was created"
    );
    assert_eq!(
        StoreError::UnknownTemporary(0).to_string(),
        "Temporary keyspace 0 does not exist: it was dropped, or the store was reopened since it was created"
    );
    assert_eq!(
        StoreError::UnknownTemporary(u64::MAX).to_string(),
        "Temporary keyspace 18446744073709551615 does not exist: it was dropped, or the store was reopened since \
         it was created"
    );
}
