/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Changesets applied to the quads of one store: index maintenance, catalog changes, index builds and drops.
//!
//! A [`ChangesetApplier`] holds the quad store, the catalog of each namespace and the executor its builds run on. A
//! changeset is one request of the posting lists: its terms resolved, its operations checked against the catalog, its
//! statements written to the forward, reverse and predicate lists with their value-index tokens, counts, statement
//! records, HNSW lists and statistics, committed whole or not at all. A declaration the data contradicts is refused; one
//! adding an index to a predicate holding data builds it in the background while writes maintain the old declaration's
//! indexes and the new one's, and swaps it in with one batch. Drops remove what they drop physically, in every graph.
//!
//! ```
//! use oxrdf::{GraphName, Literal, NamedNode, Term};
//! use ultragraphdb_core_mapping::{Change, Changeset, Declaration, PredicateDeclaration};
//! use ultragraphdb_store_apply::ChangesetApplier;
//! use ultragraphdb_store_posting::{Lists, ListsConfig};
//! use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, SteppedExecutor, Stream};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
//! let cache = SimListCache::new(64, Seed::new(1).stream(Stream::Cache));
//! let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
//! let applier = ChangesetApplier::new(Lists::new(store, cache, ListsConfig::default()), executor, 1)?;
//!
//! let name = NamedNode::new("http://ex.org/name")?;
//! let declared = PredicateDeclaration::new(name.clone()).with_tokenizers(vec!["exact".to_string()]);
//! let changeset = Changeset::new(0)
//!     .with(Change::Declare(Declaration::Predicate(declared)))
//!     .with(Change::Insert {
//!         graph: GraphName::DefaultGraph,
//!         subject: NamedNode::new("http://ex.org/a")?.into(),
//!         predicate: name.clone(),
//!         object: Term::from(Literal::new_simple_literal("Ann")),
//!     });
//! let applied = applier.apply(&changeset)?;
//! assert!(applied.commit_ts() > 0);
//! # Ok(())
//! # }
//! ```

mod errors;
mod types;
mod utils;

pub use crate::errors::changeset_error::{ChangesetError, ChangesetErrorEnum};
pub use crate::types::applied::Applied;
pub use crate::types::changeset_applier::ChangesetApplier;
pub use crate::types::level_draw::LevelDraw;
pub use crate::utils::token_values::token_value;
