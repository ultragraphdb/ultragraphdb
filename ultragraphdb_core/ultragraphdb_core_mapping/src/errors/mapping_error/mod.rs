/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

mod mapping_error_display;
mod mapping_error_error;

/// Why a changeset could not be applied to the canonical statements, or a stored contract version is refused. The
/// classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MappingErrorEnum {
    /// A store written under another major version of the mapping contract.
    ContractVersion {
        /// The stored version, major and minor.
        stored: (u16, u16),
        /// The version implemented, major and minor.
        implemented: (u16, u16),
    },
    /// The bytes of a stored contract version cannot be read.
    BadContractVersion {
        /// The stored bytes.
        bytes: Vec<u8>,
    },
    /// A statement-property change names a statement that does not exist; `statement` describes it.
    NoSuchStatement {
        /// The statement named.
        statement: String,
    },
    /// A statement-property change names a triple held by several statements, which only their IDs tell apart.
    AmbiguousStatement {
        /// The triple named.
        statement: String,
    },
    /// An object the contract does not hold: a triple term, which RDF 1.2's reifiers present but no write stores.
    UnsupportedObject {
        /// The object.
        object: String,
    },
    /// A base IRI the property-graph view cannot name IRIs under: not an absolute IRI, with a query or a fragment, or
    /// not ending in `/`.
    InvalidBase {
        /// The base IRI.
        base: String,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// A statement ID whose system ID is not of the `Statement` type.
    NotAStatementId {
        /// The system ID given.
        id: SystemId,
    },
}

/// See [`MappingErrorEnum`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MappingError(pub MappingErrorEnum);

impl MappingError {
    /// A [`MappingErrorEnum::ContractVersion`] for the `stored` and `implemented` versions.
    pub fn contract_version(stored: (u16, u16), implemented: (u16, u16)) -> Self {
        Self(MappingErrorEnum::ContractVersion {
            stored,
            implemented,
        })
    }

    /// A [`MappingErrorEnum::BadContractVersion`] for `bytes`.
    pub fn bad_contract_version(bytes: &[u8]) -> Self {
        Self(MappingErrorEnum::BadContractVersion {
            bytes: bytes.to_vec(),
        })
    }

    /// A [`MappingErrorEnum::NoSuchStatement`] for `statement`.
    pub fn no_such_statement(statement: String) -> Self {
        Self(MappingErrorEnum::NoSuchStatement { statement })
    }

    /// A [`MappingErrorEnum::AmbiguousStatement`] for `statement`.
    pub fn ambiguous_statement(statement: String) -> Self {
        Self(MappingErrorEnum::AmbiguousStatement { statement })
    }

    /// A [`MappingErrorEnum::UnsupportedObject`] for `object`.
    pub fn unsupported_object(object: String) -> Self {
        Self(MappingErrorEnum::UnsupportedObject { object })
    }

    /// A [`MappingErrorEnum::InvalidBase`] for `base`, saying why.
    pub fn invalid_base(base: String, reason: &'static str) -> Self {
        Self(MappingErrorEnum::InvalidBase { base, reason })
    }

    /// A [`MappingErrorEnum::NotAStatementId`] for `id`.
    pub fn not_a_statement_id(id: SystemId) -> Self {
        Self(MappingErrorEnum::NotAStatementId { id })
    }
}
