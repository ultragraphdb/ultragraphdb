/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{Command, Header, HeaderView};

use crate::utils::golden::{CLUSTER, GOLDEN, HELLO, READ, put, reseal};

fn verify(bytes: &[u8]) -> Result<Header, ProtocolErrorEnum> {
    HeaderView::new(bytes)
        .expect("a header")
        .verify(CLUSTER)
        .map_err(|e| e.0)
}

/// HELLO's header with `edit` applied and its checksum recomputed.
fn resealed(edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut bytes = HELLO.bytes()[..256].to_vec();
    edit(&mut bytes);
    reseal(&mut bytes);
    bytes
}

#[test]
fn test_golden_headers_verify_to_their_fields() {
    for golden in GOLDEN {
        let header = verify(&golden.bytes()).expect("a golden header verifies");
        let want = Header::new(CLUSTER, Command::from_number(golden.command).unwrap())
            .with_view(golden.view)
            .with_release(golden.release)
            .with_protocol(golden.protocol)
            .with_stream(golden.stream)
            .with_replica(golden.replica)
            .with_flags(golden.flags)
            .with_command_header(golden.command_header());
        assert_eq!(header, want, "{}", golden.name);
    }
}

#[test]
fn test_a_changed_checksum_names_both_values() {
    let mut bytes = HELLO.bytes();
    bytes[0] ^= 0x01;
    assert_eq!(
        verify(&bytes),
        Err(ProtocolErrorEnum::HeaderChecksum {
            stored: 0x8B60_6B87_756C_942E,
            computed: 0x8B60_6B87_756C_942F
        })
    );
    // Byte 100 flipped: the reference's XXH3-64 of the changed bytes 8..256 (Python's xxhash, libxxhash 0.8.2).
    let mut bytes = HELLO.bytes();
    bytes[100] ^= 0x01;
    assert_eq!(
        verify(&bytes),
        Err(ProtocolErrorEnum::HeaderChecksum {
            stored: 0x8B60_6B87_756C_942F,
            computed: 0xD0E4_2B61_6601_E446
        })
    );
}

#[test]
fn test_every_bit_flip_in_the_header_fails_its_checksum() {
    let golden = READ.bytes();
    for bit in 0..256 * 8 {
        let mut bytes = golden.clone();
        bytes[bit / 8] ^= 1 << (bit % 8);
        assert!(
            matches!(
                verify(&bytes),
                Err(ProtocolErrorEnum::HeaderChecksum { .. })
            ),
            "bit {bit}"
        );
    }
}

#[test]
fn test_an_unknown_command_is_refused_by_number() {
    for number in [0u16, 18, 256, 511, u16::MAX] {
        let bytes = resealed(|b| put(b, 54, number.into(), 2));
        assert_eq!(
            verify(&bytes),
            Err(ProtocolErrorEnum::UnknownCommand { command: number })
        );
    }
}

#[test]
fn test_each_reserved_byte_must_be_zero() {
    for offset in (16..32).chain(66..128) {
        let bytes = resealed(|b| b[offset] = 0x01);
        assert_eq!(
            verify(&bytes),
            Err(ProtocolErrorEnum::ReservedNotZero { offset }),
            "offset {offset}"
        );
    }
}

#[test]
fn test_the_first_nonzero_reserved_byte_is_named() {
    let bytes = resealed(|b| {
        b[100] = 1;
        b[70] = 1;
    });
    assert_eq!(
        verify(&bytes),
        Err(ProtocolErrorEnum::ReservedNotZero { offset: 70 })
    );
    let bytes = resealed(|b| {
        b[90] = 1;
        b[20] = 1;
    });
    assert_eq!(
        verify(&bytes),
        Err(ProtocolErrorEnum::ReservedNotZero { offset: 20 })
    );
}

#[test]
fn test_the_neighbours_of_the_reserved_bytes_are_not_reserved() {
    // Byte 65 is `flags` and 128 starts the command header: either may be anything.
    let bytes = resealed(|b| {
        b[65] = 0xFF;
        b[128] = 0xFF;
        b[255] = 0xFF;
    });
    let header = verify(&bytes).expect("flags and the command header are free");
    assert_eq!(header.flags(), 0xFF);
    // Byte 15 ends `checksum_body` and 32 starts `cluster`: they are not reserved bytes either.
    let bytes = resealed(|b| b[15] ^= 0x80);
    assert!(verify(&bytes).is_ok());
}

#[test]
fn test_another_cluster_is_refused_naming_both() {
    let bytes = resealed(|b| put(b, 32, 0x0123_4567_89AB_CDEE, 8));
    assert_eq!(
        verify(&bytes),
        Err(ProtocolErrorEnum::WrongCluster {
            expected: CLUSTER,
            found: 0x0123_4567_89AB_CDEE
        })
    );
    let golden = HELLO.bytes();
    let view = HeaderView::new(&golden).unwrap();
    assert_eq!(
        view.verify(0),
        Err(ProtocolError(ProtocolErrorEnum::WrongCluster {
            expected: 0,
            found: CLUSTER
        }))
    );
}

#[test]
fn test_size_is_at_least_a_header_and_at_most_one_mebibyte() {
    for size in [0u32, 1, 255] {
        let bytes = resealed(|b| put(b, 40, size.into(), 4));
        assert_eq!(
            verify(&bytes),
            Err(ProtocolErrorEnum::SizeBelowHeader { size })
        );
    }
    for size in [256u32, 4_096, 1_048_576] {
        let bytes = resealed(|b| put(b, 40, size.into(), 4));
        assert!(verify(&bytes).is_ok(), "size {size}");
    }
    for size in [1_048_577u32, 2_097_152, u32::MAX] {
        let bytes = resealed(|b| put(b, 40, size.into(), 4));
        assert_eq!(
            verify(&bytes),
            Err(ProtocolErrorEnum::FrameTooLarge {
                size: size as usize,
                limit: 1_048_576
            })
        );
    }
}

#[test]
fn test_rules_are_checked_in_the_protocols_order() {
    // Each step breaks one more rule earlier in the order; the earliest broken is the one reported.
    let all_broken = |b: &mut Vec<u8>| {
        put(b, 40, u32::MAX.into(), 4);
        put(b, 32, 7, 8);
        b[66] = 1;
        put(b, 54, 999, 2);
    };
    let bytes = resealed(|b| put(b, 40, u32::MAX.into(), 4));
    assert!(matches!(
        verify(&bytes),
        Err(ProtocolErrorEnum::FrameTooLarge { .. })
    ));
    let bytes = resealed(|b| {
        put(b, 40, 10, 4);
        put(b, 32, 7, 8);
    });
    assert!(matches!(
        verify(&bytes),
        Err(ProtocolErrorEnum::WrongCluster { .. })
    ));
    let bytes = resealed(|b| {
        put(b, 40, u32::MAX.into(), 4);
        put(b, 32, 7, 8);
        b[66] = 1;
    });
    assert!(matches!(
        verify(&bytes),
        Err(ProtocolErrorEnum::ReservedNotZero { .. })
    ));
    let bytes = resealed(all_broken);
    assert_eq!(
        verify(&bytes),
        Err(ProtocolErrorEnum::UnknownCommand { command: 999 })
    );
    let mut bytes = resealed(all_broken);
    bytes[0] ^= 1;
    assert!(matches!(
        verify(&bytes),
        Err(ProtocolErrorEnum::HeaderChecksum { .. })
    ));
}
