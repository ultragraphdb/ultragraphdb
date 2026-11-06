/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A value an operation claims, as its bucket is computed: the predicate's system ID, which the frontend learns from its home, and
/// the value's encoding.
#[derive(Debug, Clone)]
pub(crate) struct Claim {
    pub predicate: String,
    pub value: Vec<u8>,
}
