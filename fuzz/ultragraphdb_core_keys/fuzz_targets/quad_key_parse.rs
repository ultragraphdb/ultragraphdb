/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The quad key's parser over arbitrary bytes. A key parses into its fields and a split part's start, or is refused
//! with an error naming the bytes it was given; nothing panics. An accepted key encodes back to the bytes it was
//! parsed from: the base key, or the split key with its start appended.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_keys::{QuadKey, QuadKeyErrorEnum};

/// The bytes an error names.
fn named(error: &QuadKeyErrorEnum) -> &[u8] {
    match error {
        QuadKeyErrorEnum::Truncated { key }
        | QuadKeyErrorEnum::TrailingBytes { key }
        | QuadKeyErrorEnum::UnknownKeyspace { key }
        | QuadKeyErrorEnum::InvalidId { key, .. }
        | QuadKeyErrorEnum::UnknownDirection { key }
        | QuadKeyErrorEnum::BadObject { key, .. }
        | QuadKeyErrorEnum::VectorObject { key }
        | QuadKeyErrorEnum::NotAList { key }
        | QuadKeyErrorEnum::GroupRange { key, .. }
        | QuadKeyErrorEnum::FingerprintStart { key } => key,
    }
}

fuzz_target!(|data: &[u8]| {
    match QuadKey::parse(data) {
        Ok((key, None)) => assert_eq!(
            key.to_bytes(),
            data,
            "an accepted key encodes back to its bytes"
        ),
        Ok((key, Some(start))) => assert_eq!(
            key.split_bytes(start).map_err(|e| e.0),
            Ok(data.to_vec()),
            "an accepted split key encodes back to its bytes"
        ),
        Err(error) => assert_eq!(named(&error.0), data, "a refusal names the key's bytes"),
    }
});
