/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Where an ID got or created for an IRI came from, which decides whether the write naming it writes its dictionary
/// entries and whether any list can hold it yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TermOrigin {
    /// The dictionary held it: its entries are written, and lists may hold it.
    Held,
    /// A concurrent execution created it and no commit has written its entries yet: the write holds it and writes
    /// them, whether or not its creator commits, and lists written since the write's snapshot may hold it.
    Pending,
    /// The write created it: the write writes its entries, and no list of its snapshot holds it.
    Created,
}
