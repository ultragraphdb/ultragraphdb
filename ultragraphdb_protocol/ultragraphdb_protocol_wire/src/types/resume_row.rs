/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The last row a resume key continues after, each position an index into the key's term table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeRow {
    graph: Option<u32>,
    subject: u32,
    predicate: u32,
    object: u32,
    reifier: Option<u32>,
}

impl ResumeRow {
    /// The row of `graph`, `None` for the default graph, `subject`, `predicate`, `object` and `reifier`, `None` for a statement
    /// without a statement ID.
    #[must_use]
    pub fn new(
        graph: Option<u32>,
        subject: u32,
        predicate: u32,
        object: u32,
        reifier: Option<u32>,
    ) -> Self {
        Self {
            graph,
            subject,
            predicate,
            object,
            reifier,
        }
    }

    /// The row's graph; `None` for the default graph.
    #[must_use]
    pub fn graph(&self) -> Option<u32> {
        self.graph
    }

    /// The row's subject.
    #[must_use]
    pub fn subject(&self) -> u32 {
        self.subject
    }

    /// The row's predicate.
    #[must_use]
    pub fn predicate(&self) -> u32 {
        self.predicate
    }

    /// The row's object.
    #[must_use]
    pub fn object(&self) -> u32 {
        self.object
    }

    /// The row's reifier; `None` for a statement without a statement ID.
    #[must_use]
    pub fn reifier(&self) -> Option<u32> {
        self.reifier
    }
}
