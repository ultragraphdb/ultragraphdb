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
    LiteralOnEdge {
        /// The predicate.
        predicate: String,
        /// The object.
        object: String,
    },
    /// An IRI or blank-node object for a predicate declared a property.
    ResourceOnProperty {
        /// The predicate.
        predicate: String,
        /// The object.
        object: String,
    },
    /// A literal of another datatype than the predicate's constraint.
    DatatypeMismatch {
        /// The predicate.
        predicate: String,
        /// The datatype the constraint names.
        expected: String,
        /// The literal's datatype.
        found: String,
    },
    /// A tokenizer the registry does not know.
    UnknownTokenizer {
        /// The predicate.
        predicate: String,
        /// The tokenizer.
        tokenizer: String,
    },
    /// A datatype constraint none of whose values a declared tokenizer takes.
    TokenizerDatatype {
        /// The predicate.
        predicate: String,
        /// The tokenizer.
        tokenizer: String,
        /// The datatype constraint.
        datatype: String,
    },
    /// A vector index whose factory the registry does not know.
    UnknownVectorIndex {
        /// The predicate.
        predicate: String,
        /// The index's factory.
        factory: String,
    },
    /// An entity type declared with a parent other than the one it has.
    SecondParent {
        /// The entity type.
        entity_type: String,
        /// The parent declared.
        parent: String,
        /// The parent it has.
        other: String,
    },
    /// A parent that would make the type its own ancestor.
    ParentCycle {
        /// The entity type.
        entity_type: String,
        /// The parent declared.
        parent: String,
    },
    /// The store failed, or holds a catalog entry that does not parse or decode: its `Corrupted`, naming the key.
    Store(StoreError),
}

/// See [`CatalogErrorEnum`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogError(pub CatalogErrorEnum);

impl CatalogError {
    /// A [`CatalogErrorEnum::LiteralOnEdge`] naming what it refused.
    pub fn literal_on_edge(predicate: &str, object: String) -> Self {
        Self(CatalogErrorEnum::LiteralOnEdge {
            predicate: predicate.to_string(),
            object,
        })
    }

    /// A [`CatalogErrorEnum::ResourceOnProperty`] naming what it refused.
    pub fn resource_on_property(predicate: &str, object: String) -> Self {
        Self(CatalogErrorEnum::ResourceOnProperty {
            predicate: predicate.to_string(),
            object,
        })
    }

    /// A [`CatalogErrorEnum::DatatypeMismatch`] naming what it refused.
    pub fn datatype_mismatch(predicate: &str, expected: &str, found: &str) -> Self {
        Self(CatalogErrorEnum::DatatypeMismatch {
            predicate: predicate.to_string(),
            expected: expected.to_string(),
            found: found.to_string(),
        })
    }

    /// A [`CatalogErrorEnum::UnknownTokenizer`] naming what it refused.
    pub fn unknown_tokenizer(predicate: &str, tokenizer: &str) -> Self {
        Self(CatalogErrorEnum::UnknownTokenizer {
            predicate: predicate.to_string(),
            tokenizer: tokenizer.to_string(),
        })
    }

    /// A [`CatalogErrorEnum::TokenizerDatatype`] naming what it refused.
    pub fn tokenizer_datatype(predicate: &str, tokenizer: &str, datatype: &str) -> Self {
        Self(CatalogErrorEnum::TokenizerDatatype {
            predicate: predicate.to_string(),
            tokenizer: tokenizer.to_string(),
            datatype: datatype.to_string(),
        })
    }

    /// A [`CatalogErrorEnum::UnknownVectorIndex`] naming what it refused.
    pub fn unknown_vector_index(predicate: &str, factory: &str) -> Self {
        Self(CatalogErrorEnum::UnknownVectorIndex {
            predicate: predicate.to_string(),
            factory: factory.to_string(),
        })
    }

    /// A [`CatalogErrorEnum::SecondParent`] naming what it refused.
    pub fn second_parent(entity_type: &str, parent: &str, other: &str) -> Self {
        Self(CatalogErrorEnum::SecondParent {
            entity_type: entity_type.to_string(),
            parent: parent.to_string(),
            other: other.to_string(),
        })
    }

    /// A [`CatalogErrorEnum::ParentCycle`] naming what it refused.
    pub fn parent_cycle(entity_type: &str, parent: &str) -> Self {
        Self(CatalogErrorEnum::ParentCycle {
            entity_type: entity_type.to_string(),
            parent: parent.to_string(),
        })
    }

    /// A catalog entry of `key` that does not parse or decode, as the store's `Corrupted`, so a caller sees one kind of
    /// corruption whichever layer found it. Every entry is written from a declaration through the codec, so one that
    /// does not read back is a fault of the store or of the code, never data to skip.
    pub fn corrupted(key: &[u8], reason: impl Into<String>) -> Self {
        Self(CatalogErrorEnum::Store(StoreError::corrupted(
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
