/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_terms::EncodedTerm;

/// A resume key's last row by what a partition's keys hold: where a read continues (`PROTOCOL.md` §11.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResumeAt {
    pub(crate) graph: Graph,
    pub(crate) subject: SystemId,
    pub(crate) predicate: SystemId,
    pub(crate) object: EncodedTerm,
    pub(crate) statement: Option<SystemId>,
}
