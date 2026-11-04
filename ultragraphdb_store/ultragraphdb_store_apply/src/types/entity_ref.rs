/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// The entity a registration names at its home: an IRI by its text, for an IRI the registrant holds no ID for, or a
/// known entity by its ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityRef<'a> {
    /// An IRI by its text.
    Iri(&'a str),
    /// An entity by its ID.
    Id(SystemId),
}
