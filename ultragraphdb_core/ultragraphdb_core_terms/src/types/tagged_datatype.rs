/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A datatype `PROTOCOL.md` §10.4 names by a code of 34 to 36, whose values a Result holds by value under a tag of its
/// own (§10.1): `ugdb:bfloat16vector` (`0x41`), `ugdb:measure` (`0x50`) and `ugdb:precision` (`0x51`). A peer's
/// encoding names one in tags `0x30` and `0x31` with a lexical form; the store encodes no literal by them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaggedDatatype {
    /// `ugdb:bfloat16vector`, code 34: a `Vector<BF16, n>`.
    Bfloat16Vector,
    /// `ugdb:measure`, code 35: a number with its unit.
    Measure,
    /// `ugdb:precision`, code 36: a temporal bound's precision.
    Precision,
}

impl TaggedDatatype {
    /// The datatype of `code`, if one of the three has it.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            34 => Some(Self::Bfloat16Vector),
            35 => Some(Self::Measure),
            36 => Some(Self::Precision),
            _ => None,
        }
    }

    /// The code §10.4 gives it.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Bfloat16Vector => 34,
            Self::Measure => 35,
            Self::Precision => 36,
        }
    }

    /// Its IRI.
    #[must_use]
    pub fn iri(self) -> &'static str {
        match self {
            Self::Bfloat16Vector => "https://w3id.org/ultragraphdb/ns/bfloat16vector",
            Self::Measure => "https://w3id.org/ultragraphdb/ns/measure",
            Self::Precision => "https://w3id.org/ultragraphdb/ns/precision",
        }
    }
}
