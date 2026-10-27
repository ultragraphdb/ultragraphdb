/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The statement a property change names: by its statement ID, or by its triple's terms, each a term-table index;
/// the graph `None` for the default graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatementTarget {
    Id {
        graph: Option<u32>,
        id: u64,
    },
    Triple {
        graph: Option<u32>,
        subject: u32,
        predicate: u32,
        object: u32,
    },
}
