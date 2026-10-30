/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphNameRef, NamedNodeRef};

/// What a Drop command asks to remove, by name, as the store prepares it ([`crate::ChangesetApplier::prepare_drop`]):
/// a predicate, a graph, a namespace's data, a namespace, or every namespace (`PROTOCOL.md` §6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropRequest<'a> {
    Predicate(NamedNodeRef<'a>),
    Graph(GraphNameRef<'a>),
    Data,
    Namespace,
    All,
}
