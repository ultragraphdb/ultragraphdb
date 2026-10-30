/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{ChunkLayout, ChunkedBody};

use crate::utils::golden::{CREDIT, GOLDEN, GoldenFrame, HELLO, READ, ROUND};

fn body_of(golden: &GoldenFrame) -> Vec<u8> {
    golden.bytes()[256..].to_vec()
}

fn verify(body: &[u8], checksum_body: u64) -> Result<ChunkLayout, ProtocolErrorEnum> {
    ChunkedBody::verify(body, checksum_body)
        .map(|b| b.layout())
        .map_err(|e| e.0)
}

#[test]
fn test_golden_bodies_verify() {
    for golden in GOLDEN {
        let body = body_of(&golden);
        let checked = ChunkedBody::verify(&body, golden.checksum_body).expect("a golden body");
        assert_eq!(checked.data(), &golden.data()[..], "{}", golden.name);
        assert_eq!(
            checked.layout().chunks(),
            golden.leaves.len().max(1),
            "{}",
            golden.name
        );
    }
}

#[test]
fn test_encode_writes_the_reference_table_and_checksum_body() {
    for golden in GOLDEN {
        let mut out = Vec::new();
        let checksum_body = ChunkedBody::encode(&golden.data(), &mut out);
        assert_eq!(checksum_body, golden.checksum_body, "{}", golden.name);
        assert_eq!(out, body_of(&golden), "{}", golden.name);
        let table: Vec<u64> = out[golden.data_len..]
            .chunks(8)
            .map(|leaf| u64::from_le_bytes(leaf.try_into().unwrap()))
            .collect();
        assert_eq!(table, golden.leaves, "{}", golden.name);
    }
}

#[test]
fn test_encode_appends_to_what_out_holds() {
    let mut out = b"prefix".to_vec();
    let checksum_body = ChunkedBody::encode(&HELLO.data(), &mut out);
    assert_eq!(checksum_body, HELLO.checksum_body);
    assert_eq!(&out[..6], b"prefix");
    assert_eq!(&out[6..], &body_of(&HELLO)[..]);
}

#[test]
fn test_a_corrupted_chunk_is_named_with_its_range() {
    let cases = [
        (0, 0, 3_840),
        (1, 3_840, 7_936),
        (3, 12_032, 16_128),
        (5, 20_224, 20_432),
    ];
    for (index, start, end) in cases {
        for at in [start, (start + end) / 2, end - 1] {
            let mut body = body_of(&READ);
            body[at] ^= 0x10;
            assert_eq!(
                verify(&body, READ.checksum_body),
                Err(ProtocolErrorEnum::Chunk { index, start, end }),
                "byte {at}"
            );
        }
    }
}

#[test]
fn test_the_first_failing_chunk_is_reported() {
    let mut body = body_of(&READ);
    body[17_000] ^= 1;
    body[8_000] ^= 1;
    assert_eq!(
        verify(&body, READ.checksum_body),
        Err(ProtocolErrorEnum::Chunk {
            index: 2,
            start: 7_936,
            end: 12_032
        })
    );
}

#[test]
fn test_swapped_chunks_fail_at_the_first() {
    let mut body = body_of(&READ);
    let (one, two) = (body[3_840..7_936].to_vec(), body[7_936..12_032].to_vec());
    body[3_840..7_936].copy_from_slice(&two);
    body[7_936..12_032].copy_from_slice(&one);
    assert_eq!(
        verify(&body, READ.checksum_body),
        Err(ProtocolErrorEnum::Chunk {
            index: 1,
            start: 3_840,
            end: 7_936
        })
    );
}

#[test]
fn test_a_corrupted_table_trusts_no_chunk() {
    // The table's XXH3-64 after its first byte's low bit flips, by Python's xxhash (libxxhash 0.8.2).
    let mut body = body_of(&READ);
    body[20_432] ^= 0x01;
    assert_eq!(
        verify(&body, READ.checksum_body),
        Err(ProtocolErrorEnum::ChunkTable {
            stored: 0xB5B8_0037_54D2_87ED,
            computed: 0xB86B_11AC_D2C2_98E9
        })
    );
    // A wrong checksum_body for a sound body is the same failure.
    assert_eq!(
        verify(&body_of(&READ), 0),
        Err(ProtocolErrorEnum::ChunkTable {
            stored: 0,
            computed: 0xB5B8_0037_54D2_87ED
        })
    );
}

#[test]
fn test_a_missing_table_is_a_table_failure() {
    // Without its 48 bytes, the 20,432 bytes split as 6 chunks over 20,384 and a table of the last 48 bytes of data;
    // their XXH3-64 by Python's xxhash.
    let body = &body_of(&READ)[..20_432];
    assert_eq!(
        verify(body, READ.checksum_body),
        Err(ProtocolErrorEnum::ChunkTable {
            stored: 0xB5B8_0037_54D2_87ED,
            computed: 0x060D_B6D8_96F8_6A24
        })
    );
}

#[test]
fn test_a_one_chunk_body_is_checked_against_checksum_body() {
    let mut body = body_of(&HELLO);
    body[99] ^= 0x80;
    assert_eq!(
        verify(&body, HELLO.checksum_body),
        Err(ProtocolErrorEnum::Chunk {
            index: 0,
            start: 0,
            end: 100
        })
    );
    assert_eq!(
        verify(&[], CREDIT.checksum_body ^ 1),
        Err(ProtocolErrorEnum::Chunk {
            index: 0,
            start: 0,
            end: 0
        })
    );
    assert_eq!(verify(&[], CREDIT.checksum_body), Ok(ChunkLayout::of_data(0)));
    assert_eq!(ChunkLayout::of_data(0).chunks(), 1);
}

#[test]
fn test_every_byte_of_a_body_is_covered() {
    // ROUND's body: two full chunks, then its 16-byte table at 7,936.
    let golden = body_of(&ROUND);
    for at in 0..golden.len() {
        let mut body = golden.clone();
        body[at] ^= 0x04;
        let refused = verify(&body, ROUND.checksum_body);
        match at {
            0..3_840 => assert_eq!(
                refused,
                Err(ProtocolErrorEnum::Chunk {
                    index: 0,
                    start: 0,
                    end: 3_840
                }),
                "byte {at}"
            ),
            3_840..7_936 => assert_eq!(
                refused,
                Err(ProtocolErrorEnum::Chunk {
                    index: 1,
                    start: 3_840,
                    end: 7_936
                }),
                "byte {at}"
            ),
            _ => assert!(
                matches!(
                    refused,
                    Err(ProtocolErrorEnum::ChunkTable { stored, computed })
                        if stored == ROUND.checksum_body && computed != stored
                ),
                "byte {at}: {refused:?}"
            ),
        }
    }
}

#[test]
fn test_a_length_with_no_split_is_refused_first() {
    assert_eq!(
        ChunkedBody::verify(&[0u8; 3_841], 0),
        Err(ProtocolError(ProtocolErrorEnum::BodyLength { len: 3_841 }))
    );
}
