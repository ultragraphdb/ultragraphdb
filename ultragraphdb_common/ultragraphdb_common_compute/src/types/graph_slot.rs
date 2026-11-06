/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A read's graph position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GraphSlot {
    Unbound,
    Default,
    Named(String),
}
