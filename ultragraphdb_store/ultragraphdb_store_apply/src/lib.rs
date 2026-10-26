/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph's application of edges, schema changes and drops to posting lists, without Raft: `posting/index.go`'s
//! index maintenance, rebuilds and drops, and `worker`'s `runMutation`, `ValidateAndConvert`, schema changes and
//! `applyMutations`.
//!
//! An [`Applier`] holds the posting lists of one store, the schema state and the executor rebuilds run on. Edges are
//! applied in an [`ApplyRequest`] with their index tokens, reverse edges and count keys, and committed with an
//! [`ApplyBatch`]; a predicate an edge creates is written in its request's commit batch. A schema change that adds an
//! index builds it on the executor while writes maintain both schemas' indexes, and swaps it in with one batch; a
//! failed build leaves the previous schema's indexes complete. Drops remove what they drop physically.
//!
//! ```
//! use ultragraphdb_core_keys::attr_in_root_namespace;
//! use ultragraphdb_core_catalog::parse;
//! use ultragraphdb_core_types::TypeId;
//! use ultragraphdb_proto_pb::pb::DirectedEdge;
//! use ultragraphdb_store_apply::{IndexOp, IndexRebuild, validate_and_convert};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let old = parse("name: string @index(term) .")?.into_parts().0.remove(0);
//! let new = parse("name: string @index(exact) .")?.into_parts().0.remove(0);
//! let change = IndexRebuild::new(&old.predicate, 12, Some(old.clone()), new.clone());
//! let info = change.needs_tok_index_rebuild()?;
//! assert_eq!(info.op(), IndexOp::Rebuild);
//! assert_eq!(info.tokenizers_to_delete(), ["term"]);
//! assert_eq!(info.tokenizers_to_rebuild(), ["exact"]);
//! // Queries see neither index until the exact one is built.
//! assert!(change.get_query_schema()?.tokenizer.is_empty());
//!
//! let age = parse("age: int .")?.into_parts().0.remove(0);
//! let attr = attr_in_root_namespace("age");
//! let mut edge = DirectedEdge { entity: 1, attr, value: b"12".to_vec(), ..DirectedEdge::default() };
//! validate_and_convert(&mut edge, &age)?;
//! assert_eq!(TypeId::new(edge.value_type), TypeId::INT);
//! assert_eq!(edge.value, 12i64.to_le_bytes());
//! # Ok(())
//! # }
//! ```

mod errors;
mod types;
mod utils;

pub use crate::errors::apply_error::{ApplyError, ApplyErrorEnum};
pub use crate::errors::changeset_error::{ChangesetError, ChangesetErrorEnum};
pub use crate::types::applied::Applied;
pub use crate::types::applier::Applier;
pub use crate::types::apply_batch::ApplyBatch;
pub use crate::types::apply_request::ApplyRequest;
pub use crate::types::changeset_applier::ChangesetApplier;
pub use crate::types::index_op::IndexOp;
pub use crate::types::index_rebuild::IndexRebuild;
pub use crate::types::index_rebuild_info::IndexRebuildInfo;
pub use crate::types::index_schema::IndexSchema;
pub use crate::types::level_draw::LevelDraw;
pub use crate::utils::counts::{count_after_mutation, should_add_count_edge};
pub use crate::utils::divide::{divide_and_rule, edge_batches};
pub use crate::utils::prefixes::prefixes_to_delete_tokens_for;
pub use crate::utils::token_values::token_value;
pub use crate::utils::validate::{type_sanity_check, validate_and_convert};
