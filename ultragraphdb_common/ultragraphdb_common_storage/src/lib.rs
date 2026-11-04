/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The storage role: each partition on a runtime of its own over fjall and its write-ahead log, its log replayed before
//! it answers, listening at its own role address and joining the frontend for itself.
//!
//! One task on each core owns the partition's host. A connection hands its requests to that task by message, whole, and
//! nothing is shared between them, so no lock stands on the write path; a Write is applied from its frame with its terms
//! borrowed. What the core asks of the frontend, and of other processes' partitions, travels on role connections of its own.

mod types;
mod utils;

pub use crate::types::refill_blocks::RefillBlocks;
pub use crate::utils::error_reply::error_reply;
pub use crate::utils::storage_role::storage_role;
