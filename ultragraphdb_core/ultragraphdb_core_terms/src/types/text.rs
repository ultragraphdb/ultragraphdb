/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::hash128::Hash128;

/// A lexical form or a language tag as an encoding holds it: inline, or as the hash naming its dictionary entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Text<'a> {
    /// The text itself, held in the encoding.
    Inline(&'a str),
    /// The hash naming the text's dictionary entry.
    Hashed(Hash128),
}
