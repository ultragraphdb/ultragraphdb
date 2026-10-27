/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::HeaderView;

use crate::utils::golden::{CLUSTER, GOLDEN, HELLO, READ, put};

#[test]
fn test_each_field_reads_its_golden_value() {
    // READ's fields are distinct and nonzero, so a field read from a neighbour's offset fails.
    for golden in GOLDEN {
        let bytes = golden.bytes();
        let view = HeaderView::new(&bytes).expect("a golden header");
        assert_eq!(view.checksum(), golden.checksum, "{}", golden.name);
        assert_eq!(
            view.checksum_body(),
            golden.checksum_body,
            "{}",
            golden.name
        );
        assert_eq!(view.nonce_reserved(), 0, "{}", golden.name);
        assert_eq!(view.cluster(), CLUSTER, "{}", golden.name);
        assert_eq!(view.size(), golden.size, "{}", golden.name);
        assert_eq!(view.view(), golden.view, "{}", golden.name);
        assert_eq!(view.release(), golden.release, "{}", golden.name);
        assert_eq!(view.protocol(), golden.protocol, "{}", golden.name);
        assert_eq!(view.command(), golden.command, "{}", golden.name);
        assert_eq!(view.stream(), golden.stream, "{}", golden.name);
        assert_eq!(view.replica(), golden.replica, "{}", golden.name);
        assert_eq!(view.flags(), golden.flags, "{}", golden.name);
        assert_eq!(view.reserved(), &[0u8; 62][..], "{}", golden.name);
        assert_eq!(
            view.command_header(),
            &golden.command_header(),
            "{}",
            golden.name
        );
    }
}

#[test]
fn test_fields_are_little_endian() {
    let mut bytes = HELLO.bytes();
    bytes[40..44].copy_from_slice(&[0x01, 0x02, 0x03, 0x04]);
    bytes[16] = 0x01;
    bytes[31] = 0x80;
    bytes[54..56].copy_from_slice(&[0x34, 0x12]);
    let view = HeaderView::new(&bytes).expect("a header");
    assert_eq!(view.size(), 0x0403_0201);
    assert_eq!(view.nonce_reserved(), (1u128 << 127) | 1);
    assert_eq!(view.command(), 0x1234);
}

#[test]
fn test_reserved_spans_bytes_66_to_128() {
    let mut bytes = READ.bytes();
    bytes[65] = 0xEE;
    bytes[66] = 0x01;
    bytes[127] = 0x02;
    bytes[128] = 0xEE;
    let view = HeaderView::new(&bytes).expect("a header");
    let reserved = view.reserved();
    assert_eq!(reserved.len(), 62);
    assert_eq!((reserved[0], reserved[61]), (0x01, 0x02));
    assert!(reserved[1..61].iter().all(|b| *b == 0));
}

#[test]
fn test_fewer_than_256_bytes_is_a_short_header() {
    let bytes = HELLO.bytes();
    for len in [0, 1, 8, 128, 255] {
        assert_eq!(
            HeaderView::new(&bytes[..len]),
            Err(ProtocolError(ProtocolErrorEnum::ShortHeader { len }))
        );
    }
}

#[test]
fn test_bytes_past_the_header_are_not_read() {
    let bytes = HELLO.bytes();
    let whole = HeaderView::new(&bytes).expect("a header");
    let header_only = HeaderView::new(&bytes[..256]).expect("a header");
    assert_eq!(whole, header_only);
    let mut garbage = bytes[..256].to_vec();
    garbage.extend_from_slice(&[0xFF; 1000]);
    assert_eq!(HeaderView::new(&garbage).expect("a header"), header_only);
}

#[test]
fn test_a_view_reads_whatever_is_there_until_verified() {
    // Construction checks the length alone: a header of zeros reads as zeros.
    let zeros = [0u8; 256];
    let view = HeaderView::new(&zeros).expect("256 bytes are a header");
    assert_eq!((view.checksum(), view.size(), view.command()), (0, 0, 0));
    let mut ones = [0xFFu8; 256];
    put(&mut ones, 40, 7, 4);
    let view = HeaderView::new(&ones).expect("256 bytes are a header");
    assert_eq!(view.size(), 7);
    assert_eq!(view.cluster(), u64::MAX);
    assert_eq!(view.nonce_reserved(), u128::MAX);
}
