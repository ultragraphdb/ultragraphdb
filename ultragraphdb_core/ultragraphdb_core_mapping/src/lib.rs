/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The mapping contract: the canonical statement model, statement IDs, its RDF and property-graph views, and
//! `Changeset`, the write format the engines produce and storage applies.

mod errors;
mod types;
mod utils;

pub use crate::errors::mapping_error::{MappingError, MappingErrorEnum};
pub use crate::types::canonical_dataset::CanonicalDataset;
pub use crate::types::cardinality::Cardinality;
pub use crate::types::change::Change;
pub use crate::types::changeset::Changeset;
pub use crate::types::contract_version::{CONTRACT_VERSION, ContractVersion};
pub use crate::types::declaration::Declaration;
pub use crate::types::names::Names;
pub use crate::types::pg_edge::PgEdge;
pub use crate::types::pg_node::PgNode;
pub use crate::types::pg_value::PgValue;
pub use crate::types::predicate_declaration::PredicateDeclaration;
pub use crate::types::predicate_kind::PredicateKind;
pub use crate::types::property_graph::PropertyGraph;
pub use crate::types::relation_declaration::RelationDeclaration;
pub use crate::types::statement::Statement;
pub use crate::types::statement_id::StatementId;
pub use crate::types::statement_ref::StatementRef;
pub use crate::types::type_declaration::TypeDeclaration;
pub use crate::types::vector_index_spec::VectorIndexSpec;
pub use crate::utils::property_graph_view::{property_graph_to_changeset, property_graph_view};
pub use crate::utils::rdf_view::{quads_to_changeset, rdf_view, statement_iri};
