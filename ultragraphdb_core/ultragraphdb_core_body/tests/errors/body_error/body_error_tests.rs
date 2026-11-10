/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Each constructor builds its variant with the fields it was given.

use ultragraphdb_core_body::{BodyError, BodyErrorEnum};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_each_constructor_builds_its_variant() -> TestResult {
    assert_eq!(
        BodyError::body_too_short(3, 16).0,
        BodyErrorEnum::BodyTooShort { len: 3, need: 16 }
    );
    assert_eq!(
        BodyError::section_out_of_bounds(1, 24, 4, 26).0,
        BodyErrorEnum::SectionOutOfBounds {
            section: 1,
            offset: 24,
            len: 4,
            body: 26
        }
    );
    assert_eq!(
        BodyError::section_offset(1, 20, 24).0,
        BodyErrorEnum::SectionOffset {
            section: 1,
            offset: 20,
            expected: 24
        }
    );
    assert_eq!(
        BodyError::padding_not_zero(21).0,
        BodyErrorEnum::PaddingNotZero { offset: 21 }
    );
    assert_eq!(
        BodyError::trailing_bytes(9, 10).0,
        BodyErrorEnum::TrailingBytes { end: 9, len: 10 }
    );
    assert_eq!(
        BodyError::section_length(4, 3, 4).0,
        BodyErrorEnum::SectionLength {
            section: 4,
            len: 3,
            unit: 4
        }
    );
    assert_eq!(
        BodyError::count_mismatch("string text", 4, 2).0,
        BodyErrorEnum::CountMismatch {
            field: "string text",
            count: 4,
            expected: 2
        }
    );
    assert_eq!(
        BodyError::not_utf8(3, 1).0,
        BodyErrorEnum::NotUtf8 {
            section: 3,
            offset: 1
        }
    );
    assert_eq!(
        BodyError::index_out_of_range(0, 1, 2, 4).0,
        BodyErrorEnum::IndexOutOfRange {
            section: 0,
            at: 1,
            index: 2,
            limit: 4
        }
    );
    Ok(())
}
