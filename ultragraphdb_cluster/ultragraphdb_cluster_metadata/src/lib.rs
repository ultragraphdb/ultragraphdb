/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata group: a state machine over an ordered log holding the cluster's catalog versions, sequence-block
//! high-water marks, placement map, principals and session registry, with no I/O of its own.
//!
//! The [`MetadataGroup`] appends every command as a [`MetadataCommand`], one round each, and applies it to its
//! [`MetadataState`] once a sync has made it durable, so an acknowledged command is durable and the state is a function
//! of the log, rebuilt by [`MetadataGroup::recover`] from the last [`MetadataSnapshot`] and the rounds after it. Every
//! read of the state names the log position it reads at ([`MetadataGroup::applied`]).
//!
//! - **Catalog versions.** Each namespace's catalog is a chain of numbered versions, each a [`CatalogRecord`] whose
//!   bytes, in `ultragraphdb_core_catalog`'s encoding, the group does not read. A version is committed as pending; the
//!   sequencer's next close `c + 1` announces it, and the group learns from that close ([`MetadataGroup::learn_close`])
//!   that it takes effect at `c + 2`. Beside the versions, the group holds the default-entry records keyed by
//!   (namespace, predicate) that a predicate's first write asks for.
//! - **Sequence blocks.** Blocks are leased per (type, partition) from a per-type high-water mark that only rises;
//!   a partition takes them through [`LeasedBlocks`], its `BlockSource`.
//! - **The placement map**, `ultragraphdb_cluster_placement::PlacementMap`, initialised evenly and changed from an
//!   epoch.
//! - **Principals**, each a [`Principal`] holding its SCRAM-SHA-256 verifier, the first created from the bootstrap
//!   secret file; the state answers a SCRAM server's lookups as its `Credentials`.
//! - **Sessions**, registered with their principal and bounded by `sessions_max`, the oldest evicted; each eviction is
//!   carried to the partitions by the sequencer's next close.
//!
//! ```
//! use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataState};
//! use ultragraphdb_core_ids::Namespace;
//!
//! let mut state = MetadataState::new();
//! let offer = MetadataCommand::OfferCatalog {
//!     namespace: Namespace::new(1),
//!     predecessor: 0,
//!     bytes: b"version 1".to_vec(),
//! };
//! assert!(state.apply(&offer).is_ok());
//! assert_eq!(state.current_version(Namespace::new(1)), 1);
//! assert_eq!(state.pending_versions(), vec![(Namespace::new(1), 1)]);
//! ```

mod types;

pub use crate::types::catalog_record::CatalogRecord;
pub use crate::types::leased_blocks::LeasedBlocks;
pub use crate::types::metadata_command::MetadataCommand;
pub use crate::types::metadata_config::MetadataConfig;
pub use crate::types::metadata_group::MetadataGroup;
pub use crate::types::metadata_snapshot::MetadataSnapshot;
pub use crate::types::metadata_state::MetadataState;
pub use crate::types::principal::Principal;
