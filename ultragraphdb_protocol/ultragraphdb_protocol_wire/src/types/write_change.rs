/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::declared::Declared;
use crate::types::statement_target::StatementTarget;

/// One change of a Write, in order: its terms term-table indices of the Write, its graph `None` for the default
/// graph. The server session converts it to the mapping crate's `Change`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WriteChange {
    Insert {
        graph: Option<u32>,
        subject: u32,
        predicate: u32,
        object: u32,
    },
    InsertEdge {
        graph: Option<u32>,
        subject: u32,
        predicate: u32,
        object: u32,
        properties: Vec<(u32, u32)>,
    },
    Delete {
        graph: Option<u32>,
        subject: u32,
        predicate: u32,
        object: u32,
    },
    DeleteStatement {
        graph: Option<u32>,
        id: u64,
    },
    DeletePattern {
        graph: Option<u32>,
        subject: u32,
        predicate: u32,
    },
    SetProperty {
        statement: StatementTarget,
        predicate: u32,
        value: u32,
    },
    DeleteProperty {
        statement: StatementTarget,
        predicate: u32,
        value: Option<u32>,
    },
    Declare(Declared),
}
