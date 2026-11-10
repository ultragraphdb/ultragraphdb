/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

use crate::types::hash128::Hash128;
use crate::types::known_datatype::KnownDatatype;
use crate::types::tagged_datatype::TaggedDatatype;

/// The datatype of a typed literal an encoding holds: a known datatype by its code, one of the three a Result holds by
/// value under tags of their own by its code, or any other by its IRI's system ID as the store names it, or by its
/// IRI's hash as a Result names it (`PROTOCOL.md` §10.1, §10.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Datatype {
    /// A known datatype, by its code.
    Known(KnownDatatype),
    /// `ugdb:bfloat16vector`, `ugdb:measure` or `ugdb:precision`, by its code, 34 to 36: a lexical form a peer's tags
    /// `0x30` and `0x31` hold.
    Tagged(TaggedDatatype),
    /// Any other datatype, by its IRI's system ID: the store's encoding, read by [`crate::EncodedTerm::view`].
    Id(SystemId),
    /// Any other datatype, by the hash of its IRI: a Result's encoding, read by [`crate::EncodedTerm::result_view`].
    Hashed(Hash128),
}
