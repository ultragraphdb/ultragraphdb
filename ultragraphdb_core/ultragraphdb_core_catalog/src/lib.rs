/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The catalog: per namespace, the declarations of its predicates, entity types and relations, which both engines,
//! index maintenance and the write checks read, kept in the catalog keyspace and loaded per namespace.
//!
//! A [`Catalog`] declares a predicate's kind, cardinality, datatype constraint, unique values, indexes and domains,
//! an entity type's one parent, numbered in preorder, and a relation's roles; it checks a write's object against a
//! declaration. [`Catalog::entry`] is a declaration's store entry, and [`Catalog::load`] reads a namespace's catalog
//! back, refusing it whole when an entry does not read back.
//!
//! ```
//! use oxrdf::NamedNode;
//! use ultragraphdb_core_catalog::Catalog;
//! use ultragraphdb_core_mapping::{PredicateDeclaration, PredicateKind};
//!
//! let mut catalog = Catalog::new(0);
//! let knows = PredicateDeclaration::new(NamedNode::new_unchecked("http://ex.org/knows"))
//!     .with_kind(PredicateKind::Edge);
//! catalog.declare_predicate(7, knows).unwrap();
//! assert_eq!(catalog.predicate(7).map(|d| d.kind()), Some(PredicateKind::Edge));
//! ```

mod errors;
mod types;
mod utils;

pub use crate::errors::catalog_error::{CatalogError, CatalogErrorEnum};
pub use crate::types::catalog::Catalog;
pub use crate::types::catalog::catalog_checks::datatype_of;
pub use crate::utils::catalog_codec::{
    ENTITY_TYPE, PREDICATE, RELATION, decode as decode_declaration, encode as encode_declaration,
};
pub use crate::utils::tokenizer_datatypes::tokenizer_takes;
