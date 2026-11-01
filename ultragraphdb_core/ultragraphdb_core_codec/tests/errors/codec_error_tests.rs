/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_core_codec::{CodecError, CodecErrorEnum, encode_from_buffer};

#[test]
fn a_truncated_varint_is_refused() {
    // Go's loop never ends on it: binary.Uvarint returns 0 bytes read.
    let err = encode_from_buffer(&[0x01, 0x80], 256).expect_err("truncated");
    assert_eq!(err.0, CodecErrorEnum::TruncatedVarint { offset: 1 });
    assert_eq!(err, CodecError::TruncatedVarint(1));
    assert_eq!(err.to_string(), "truncated uvarint at byte 1 of the buffer");
    assert!(err.source().is_none());
}

#[test]
fn an_overlong_varint_is_refused() {
    // Go panics slicing the buffer at the negative length binary.Uvarint returns.
    let mut buf = vec![0xff; 9];
    buf.push(0x02);
    let err = encode_from_buffer(&buf, 256).expect_err("overflows");
    assert_eq!(err, CodecError::OverflowingVarint(0));
    assert_eq!(
        err.to_string(),
        "uvarint at byte 0 of the buffer overflows a u64"
    );
    let err = encode_from_buffer(&[0xff; 11], 256).expect_err("overlong");
    assert_eq!(err.0, CodecErrorEnum::OverflowingVarint { offset: 0 });
}

#[test]
fn a_valid_buffer_packs() {
    let pack = encode_from_buffer(&[0x01, 0x01], 256)
        .expect("valid varints")
        .expect("two UIDs");
    assert_eq!(pack.blocks()[0].base(), 1);
    assert_eq!(pack.blocks()[0].num_uids(), 2);
    assert_eq!(encode_from_buffer(&[], 256), Ok(None));
}

#[test]
fn ten_continuation_bytes_overflow_rather_than_truncate() {
    // A u64 takes at most ten base-128 groups, and a tenth byte with the continuation bit set promises an eleventh,
    // which starts at bit 70, so no byte that follows could complete a u64: it overflows, wherever the buffer ends.
    let err = encode_from_buffer(&[0x80; 10], 256).expect_err("overflows");
    assert_eq!(err, CodecError::OverflowingVarint(0));
    // One byte fewer can still end in a valid varint, so it is truncated.
    let err = encode_from_buffer(&[0x80; 9], 256).expect_err("truncated");
    assert_eq!(err, CodecError::TruncatedVarint(0));
}
