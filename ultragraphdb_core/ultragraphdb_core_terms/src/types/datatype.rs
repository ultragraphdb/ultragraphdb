/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

use crate::types::known_datatype::KnownDatatype;

/// The datatype of a typed literal an encoding holds: a known datatype by its code, or any other by its IRI's system
/// ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Datatype {
    /// A known datatype, by its code.
    Known(KnownDatatype),
    /// Any other datatype, by its IRI's system ID.
    Id(SystemId),
}
