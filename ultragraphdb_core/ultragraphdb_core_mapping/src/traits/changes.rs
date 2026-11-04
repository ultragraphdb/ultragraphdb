/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::change_ref::ChangeRef;

/// What a write applies, in order and with its terms borrowed: a [`crate::Changeset`], or the changes a frame carries.
/// One path takes either, so the two give one record.
pub trait Changes {
    /// The write's namespace.
    fn namespace(&self) -> u64;

    /// The changes, in order.
    fn changes(&self) -> impl Iterator<Item = ChangeRef<'_>>;
}
