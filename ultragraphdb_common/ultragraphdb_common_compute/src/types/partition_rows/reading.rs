/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// What the Scan under way reads.
#[derive(Debug)]
pub(super) enum Reading {
    /// Rows, into the page.
    Rows,
    /// A predicates' list, into the keys read next.
    Predicates(Vec<SystemId>),
}
