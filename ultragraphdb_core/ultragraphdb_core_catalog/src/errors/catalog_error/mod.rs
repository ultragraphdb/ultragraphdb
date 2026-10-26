/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod catalog_error_display;
mod catalog_error_error;

use ultragraphdb_error_store::StoreError;

/// Why a declaration or a write was refused against the catalog, or the catalog could not be read. Each variant names
/// what it refused. The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CatalogErrorEnum {
    /// A literal object for a predicate declared an edge.
    LiteralOnEdge { predicate: String, object: String },
    /// An IRI or blank-node object for a predicate declared a property.
    ResourceOnProperty { predicate: String, object: String },
    /// A literal of another datatype than the predicate's constraint.
    DatatypeMismatch {
        predicate: String,
        expected: String,
        found: String,
    },
    /// A tokenizer the registry does not know.
    UnknownTokenizer {
        predicate: String,
        tokenizer: String,
    },
    /// A datatype constraint none of whose values a declared tokenizer takes.
    TokenizerDatatype {
        predicate: String,
        tokenizer: String,
        datatype: String,
    },
    /// A vector index whose factory the registry does not know.
    UnknownVectorIndex { predicate: String, factory: String },
    /// An entity type declared with a parent other than the one it has.
    SecondParent {
        entity_type: String,
        parent: String,
        other: String,
    },
    /// A parent that would make the type its own ancestor.
    ParentCycle { entity_type: String, parent: String },
    /// The store failed, or holds a catalog entry that does not parse or decode: its `Corrupted`, naming the key.
    Store(StoreError),
}

/// See [`CatalogErrorEnum`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogError(pub CatalogErrorEnum);

impl CatalogError {
    #[allow(non_snake_case)]
    pub fn LiteralOnEdge(predicate: &str, object: String) -> Self {
        Self(CatalogErrorEnum::LiteralOnEdge {
            predicate: predicate.to_string(),
            object,
        })
    }

    #[allow(non_snake_case)]
    pub fn ResourceOnProperty(predicate: &str, object: String) -> Self {
        Self(CatalogErrorEnum::ResourceOnProperty {
            predicate: predicate.to_string(),
            object,
        })
    }

    #[allow(non_snake_case)]
    pub fn DatatypeMismatch(predicate: &str, expected: &str, found: &str) -> Self {
        Self(CatalogErrorEnum::DatatypeMismatch {
            predicate: predicate.to_string(),
            expected: expected.to_string(),
            found: found.to_string(),
        })
    }

    #[allow(non_snake_case)]
    pub fn UnknownTokenizer(predicate: &str, tokenizer: &str) -> Self {
        Self(CatalogErrorEnum::UnknownTokenizer {
            predicate: predicate.to_string(),
            tokenizer: tokenizer.to_string(),
        })
    }

    #[allow(non_snake_case)]
    pub fn TokenizerDatatype(predicate: &str, tokenizer: &str, datatype: &str) -> Self {
        Self(CatalogErrorEnum::TokenizerDatatype {
            predicate: predicate.to_string(),
            tokenizer: tokenizer.to_string(),
            datatype: datatype.to_string(),
        })
    }

    #[allow(non_snake_case)]
    pub fn UnknownVectorIndex(predicate: &str, factory: &str) -> Self {
        Self(CatalogErrorEnum::UnknownVectorIndex {
            predicate: predicate.to_string(),
            factory: factory.to_string(),
        })
    }

    #[allow(non_snake_case)]
    pub fn SecondParent(entity_type: &str, parent: &str, other: &str) -> Self {
        Self(CatalogErrorEnum::SecondParent {
            entity_type: entity_type.to_string(),
            parent: parent.to_string(),
            other: other.to_string(),
        })
    }

    #[allow(non_snake_case)]
    pub fn ParentCycle(entity_type: &str, parent: &str) -> Self {
        Self(CatalogErrorEnum::ParentCycle {
            entity_type: entity_type.to_string(),
            parent: parent.to_string(),
        })
    }

    /// A catalog entry of `key` that does not parse or decode, as the store's `Corrupted`, so a caller sees one kind of
    /// corruption whichever layer found it. Every entry is written from a declaration through the codec, so one that
    /// does not read back is a fault of the store or of the code, never data to skip.
    #[allow(non_snake_case)]
    pub fn Corrupted(key: &[u8], reason: impl Into<String>) -> Self {
        Self(CatalogErrorEnum::Store(StoreError::Corrupted(
            key.to_vec(),
            reason.into(),
        )))
    }
}

impl From<StoreError> for CatalogError {
    fn from(error: StoreError) -> Self {
        Self(CatalogErrorEnum::Store(error))
    }
}
