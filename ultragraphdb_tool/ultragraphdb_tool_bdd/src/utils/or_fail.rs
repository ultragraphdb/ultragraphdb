/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The value `result` holds; a test fails on its fault, with the fault as the whole message.
///
/// # Panics
///
/// Panics when `result` is an `Err`, with exactly the text it holds.
pub(crate) fn or_fail<T>(result: Result<T, String>) -> T {
    assert!(
        result.is_ok(),
        "{}",
        result.as_ref().err().map_or("", String::as_str)
    );
    result.expect("the assertion above fails on an Err")
}
