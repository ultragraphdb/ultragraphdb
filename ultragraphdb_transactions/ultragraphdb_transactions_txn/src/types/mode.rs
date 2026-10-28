/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Whether a transaction writes: a read-only one is never validated and never aborts for a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    ReadWrite,
    ReadOnly,
}
