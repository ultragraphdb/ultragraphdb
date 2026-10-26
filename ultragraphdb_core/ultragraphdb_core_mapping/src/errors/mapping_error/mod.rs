/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod mapping_error_display;
mod mapping_error_error;

/// Why a changeset could not be applied to the canonical statements, or a stored contract version is refused. The
/// classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MappingErrorEnum {
    /// A store written under another major version of the mapping contract.
    ContractVersion {
        stored: (u16, u16),
        implemented: (u16, u16),
    },
    /// The bytes of a stored contract version cannot be read.
    BadContractVersion { bytes: Vec<u8> },
    /// A statement-property change names a statement that does not exist; `statement` describes it.
    NoSuchStatement { statement: String },
    /// A statement-property change names a triple held by several statements, which only their IDs tell apart.
    AmbiguousStatement { statement: String },
    /// An object the contract does not hold: a triple term, which RDF 1.2's reifiers present but no write stores.
    UnsupportedObject { object: String },
}

/// See [`MappingErrorEnum`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MappingError(pub MappingErrorEnum);

impl MappingError {
    #[allow(non_snake_case)]
    pub fn ContractVersion(stored: (u16, u16), implemented: (u16, u16)) -> Self {
        Self(MappingErrorEnum::ContractVersion {
            stored,
            implemented,
        })
    }

    #[allow(non_snake_case)]
    pub fn BadContractVersion(bytes: &[u8]) -> Self {
        Self(MappingErrorEnum::BadContractVersion {
            bytes: bytes.to_vec(),
        })
    }

    #[allow(non_snake_case)]
    pub fn NoSuchStatement(statement: String) -> Self {
        Self(MappingErrorEnum::NoSuchStatement { statement })
    }

    #[allow(non_snake_case)]
    pub fn AmbiguousStatement(statement: String) -> Self {
        Self(MappingErrorEnum::AmbiguousStatement { statement })
    }

    #[allow(non_snake_case)]
    pub fn UnsupportedObject(object: String) -> Self {
        Self(MappingErrorEnum::UnsupportedObject { object })
    }
}
