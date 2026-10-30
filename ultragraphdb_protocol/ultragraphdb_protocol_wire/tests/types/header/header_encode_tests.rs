/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{Command, FrameView, Header, MAX_FRAME_SIZE};

use crate::utils::golden::{CLUSTER, GOLDEN, GoldenFrame, HELLO, READ};

/// The golden frame's header as the builder takes it.
fn header_of(golden: &GoldenFrame) -> Header {
    Header::new(
        CLUSTER,
        Command::from_number(golden.command).expect("a golden command"),
    )
    .with_sequence(golden.sequence)
    .with_view(golden.view)
    .with_release(golden.release)
    .with_protocol(golden.protocol)
    .with_stream(golden.stream)
    .with_replica(golden.replica)
    .with_flags(golden.flags)
    .with_command_header(golden.command_header())
}

#[test]
fn test_encode_writes_the_reference_header_bytes() {
    // The reference generator laid the fields out from the spec's offsets and checksummed with xxHash v0.8.3.
    for golden in [HELLO, READ] {
        let bytes = golden.bytes();
        let encoded = header_of(&golden).encode(golden.size, golden.checksum_body);
        assert_eq!(encoded.as_slice(), &bytes[..256], "{}", golden.name);
    }
}

#[test]
fn test_encode_places_size_and_checksum_body_as_given() {
    let encoded = Header::new(CLUSTER, Command::Hello).encode(0x0403_0201, 0x0807_0605_0403_0201);
    assert_eq!(&encoded[40..44], &[0x01, 0x02, 0x03, 0x04]);
    assert_eq!(&encoded[8..16], &[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        &encoded[16..32],
        &[0u8; 16],
        "a new header's sequence is zero"
    );
    assert_eq!(&encoded[66..128], &[0u8; 62], "the reserved bytes are zero");
}

#[test]
fn test_encode_writes_the_sequence_little_endian_at_16() {
    let sequence = 0x0F0E_0D0C_0B0A_0908_0706_0504_0302_0100u128;
    let encoded = Header::new(CLUSTER, Command::Read)
        .with_sequence(sequence)
        .encode(256, 0);
    assert_eq!(
        &encoded[16..32],
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
    );
    assert_eq!(
        &encoded[32..40],
        &CLUSTER.to_le_bytes(),
        "cluster follows it"
    );
}

#[test]
fn test_encode_frame_writes_every_golden_frame() {
    for golden in GOLDEN {
        let frame = header_of(&golden)
            .encode_frame(&golden.data())
            .expect("a golden frame fits");
        assert_eq!(frame, golden.bytes(), "{}", golden.name);
    }
}

#[test]
fn test_the_largest_frame_is_one_mebibyte() {
    // 1,046,272 bytes take 256 chunks (3,840 + 255 × 4,096 = 1,048,320 ≥ 1,046,272 > 3,840 + 254 × 4,096) and a
    // 2,048-byte table: 256 + 1,046,272 + 2,048 = 1,048,576.
    let header = Header::new(CLUSTER, Command::Result);
    let frame = header
        .encode_frame(&vec![0x5Au8; 1_046_272])
        .expect("1 MiB fits");
    assert_eq!(frame.len(), 1_048_576);
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)
        .expect("the builder's frame validates");
    assert_eq!(view.body().len(), 1_046_272);
}

#[test]
fn test_a_frame_past_one_mebibyte_is_refused_naming_the_limit() {
    // One byte more still takes 256 chunks (1,042,433 / 4,096 rounds up to 255 after the first): 1,048,577 bytes.
    let header = Header::new(CLUSTER, Command::Write);
    assert_eq!(
        header.encode_frame(&vec![0u8; 1_046_273]),
        Err(ProtocolError(ProtocolErrorEnum::FrameTooLarge {
            size: 1_048_577,
            limit: 1_048_576
        }))
    );
    // A body as long as the frame's room before its table: 1,048,320 bytes take 256 chunks and a 2,048-byte table.
    assert_eq!(
        header.encode_frame(&vec![0u8; 1_048_320]).map_err(|e| e.0),
        Err(ProtocolErrorEnum::FrameTooLarge {
            size: 1_050_624,
            limit: 1_048_576
        })
    );
}

#[test]
fn test_a_sequence_stamped_onto_a_built_frame_keeps_it_sound() {
    for golden in GOLDEN {
        let mut frame = golden.bytes();
        Header::stamp_sequence(&mut frame, 0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10).unwrap();
        let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)
            .expect("the stamped frame validates");
        assert_eq!(
            view.header().sequence(),
            0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10
        );
        // Every byte but the sequence and the checksum is the frame's.
        assert_eq!(&frame[8..16], &golden.bytes()[8..16], "{}", golden.name);
        assert_eq!(&frame[32..], &golden.bytes()[32..], "{}", golden.name);
        // Stamped back, it is the golden frame again.
        Header::stamp_sequence(&mut frame, golden.sequence).unwrap();
        assert_eq!(frame, golden.bytes(), "{}", golden.name);
    }
    assert_eq!(
        Header::stamp_sequence(&mut [0u8; 255], 1),
        Err(ProtocolError(ProtocolErrorEnum::ShortHeader { len: 255 }))
    );
}
