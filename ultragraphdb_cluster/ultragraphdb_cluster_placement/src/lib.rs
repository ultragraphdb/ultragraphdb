/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The placement map, with no I/O: which partition owns each of the 2^24 buckets, from which epoch.
//!
//! A [`BucketRange`] is a half-open range of bucket numbers. A namespace's buckets are its [`NamespaceRun`], the
//! 65,536 buckets whose top 8 bits are its hash, divided into 2^10 bucket groups of 64 buckets (`ultragraphdb_core_ids::BucketGroup`), the unit a
//! referrer set names. The [`PlacementMap`] assigns every bucket to exactly one [`PartitionId`] as ranges, keeps each
//! layout with the epoch it applies from, so a bucket or a bucket group resolves at any epoch a read names, and holds
//! each partition's preferred leader, a [`NodeId`]. The metadata group holds the map and stores it in the encoding
//! [`PlacementMap::encode`] writes.

mod types;

pub use crate::types::bucket_range::BucketRange;
pub use crate::types::namespace_run::NamespaceRun;
pub use crate::types::node_id::NodeId;
pub use crate::types::partition_id::PartitionId;
pub use crate::types::placement_map::PlacementMap;
