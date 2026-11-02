/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_core_codec::{CodecError, CodecErrorEnum};
use ultragraphdb_core_ids::IdError;

#[test]
fn test_each_constructor_builds_its_variant_and_names_its_place() {
    let cases = [
        (
            CodecError::out_of_order(2),
            CodecErrorEnum::OutOfOrder { position: 2 },
            "the ID at position 2 is not greater than the one before it",
        ),
        (
            CodecError::truncated_varint(1),
            CodecErrorEnum::TruncatedVarint { offset: 1 },
            "truncated varint at byte 1",
        ),
        (
            CodecError::overflowing_varint(0),
            CodecErrorEnum::OverflowingVarint { offset: 0 },
            "varint at byte 0 overflows its width",
        ),
        (
            CodecError::non_canonical_varint(3),
            CodecErrorEnum::NonCanonicalVarint { offset: 3 },
            "varint at byte 3 is not in its shortest form",
        ),
        (
            CodecError::delta_overflow(4),
            CodecErrorEnum::DeltaOverflow { position: 4 },
            "the delta of the ID at position 4 carries it past 104 bits",
        ),
        (
            CodecError::invalid_id(5, IdError::zero_sequence()),
            CodecErrorEnum::InvalidId {
                position: 5,
                error: IdError::zero_sequence(),
            },
            "the ID at position 5 is invalid: invalid system ID: its sequence is 0",
        ),
        (
            CodecError::trailing_deltas(6),
            CodecErrorEnum::TrailingDeltas { offset: 6 },
            "bytes after a block's last delta, at byte 6",
        ),
        (
            CodecError::truncated(7),
            CodecErrorEnum::Truncated { offset: 7 },
            "a pack's field at byte 7 overruns its bytes",
        ),
        (
            CodecError::unknown_version(9),
            CodecErrorEnum::UnknownVersion { version: 9 },
            "unknown packed ID list version 9",
        ),
        (
            CodecError::trailing_bytes(8),
            CodecErrorEnum::TrailingBytes { offset: 8 },
            "bytes after a pack's last block, at byte 8",
        ),
        (
            CodecError::blocks_out_of_order(10),
            CodecErrorEnum::BlocksOutOfOrder { block: 10 },
            "block 10 does not start above the block before it",
        ),
    ];
    for (error, variant, text) in cases {
        assert_eq!(error.0, variant);
        assert_eq!(error.to_string(), text);
        assert!(error.source().is_none());
    }
}
