/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt::Display;
use std::str::FromStr;

/// The value of `lexical` when it is the canonical form of that value: it parses, and formatting the value gives
/// `lexical` back. Only then is a literal encoded by value, so decoding the value returns the literal exactly.
pub fn canonical<T: FromStr + Display>(lexical: &str) -> Option<T> {
    lexical
        .parse::<T>()
        .ok()
        .filter(|value| value.to_string() == lexical)
}
