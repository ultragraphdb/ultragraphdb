/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::error_code::ErrorCode;

/// A transaction's decision in a Round: committed, its write-set record the round's next, or aborted with the reason
/// a reply names and its detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decision {
    Committed,
    Aborted { code: ErrorCode, detail: u64 },
}
