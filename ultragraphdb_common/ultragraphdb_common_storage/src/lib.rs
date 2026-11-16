/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The storage role: each partition on a runtime of its own over fjall and its write-ahead log, its log replayed before
//! it answers, listening at its own role address and joining the system group for itself.
//!
//! One task on each core owns the partition's host. A connection hands its requests to that task by message, whole, and
//! nothing is shared between them, so no lock stands on the write path; a Write is applied from its frame with its terms
//! borrowed. What the core asks of the system group, and of other processes' partitions, travels on role connections of
//! its own.
//!
//! Planes (Ultra rule 3): a connection's requests, Scans and registrations are the data plane. A request parked is copied
//! out of its connection's buffer once, a Scan's ranges and a registration's batch are built per request, and every frame
//! sent is the `Vec` the wire's builders return; the per-request allocations are bounded by the connection's held bytes,
//! its credit and the turn's budget. A partition's start and recovery, catalog versions installed, leases, reports and
//! retention are control plane.

mod types;
mod utils;

pub use crate::types::commit_mark::CommitMark;
pub use crate::types::commit_phase::CommitPhase;
pub use crate::types::commit_phases::CommitPhases;
pub use crate::types::core_ends::CoreEnds;
pub use crate::types::core_message::CoreMessage;
pub use crate::types::entries_fill::EntriesFill;
pub use crate::types::held_catalogs::HeldCatalogs;
pub use crate::types::log_io::LogIo;
pub use crate::types::log_io_totals::LogIoTotals;
pub use crate::types::parked_frame::ParkedFrame;
pub use crate::types::partition_core::{
    CLOSES_PER_TURN_MAX, CoreChannels, CoreParts, PartitionCore, Peers, REMOTE_PER_TURN_MAX,
    SCAN_STALL_MS_DEFAULT, StorageHost, StoragePartition, SystemAsk,
};
pub use crate::types::refill_blocks::RefillBlocks;
pub use crate::types::retention::Retention;
pub use crate::types::role_connection_phases::RoleConnectionPhases;
pub use crate::types::scan_board::ScanBoard;
pub use crate::types::scan_key::ScanKey;
pub use crate::types::storage_session::StorageSession;
pub use crate::types::timed_file::TimedFile;
pub use crate::types::timed_storage::TimedStorage;
pub use crate::types::turn_budget::{TURN_BYTES, TurnBudget};
pub use crate::utils::error_reply::{error_reply, registration_outcome, scan_refusal};
pub use crate::utils::scan_version::{scan_named, scan_version};
