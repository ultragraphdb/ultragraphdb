/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compute role's reads over storage's read kernels (`PROTOCOL.md` §11.11, §11.12, §13.12): a ComputeRead routed by its
//! bound positions, each partition it asks read by Scans at one version, every Entries stream held by an
//! [`ultragraphdb_protocol_wire::EntriesSequence`] and granted credit as its frames are consumed, the rows merged in key order
//! into Result batches naming every term by its text. One [`ComputeCore`] per core, whose connections and reads stay on it.
//! [`ComputeCore::value_index`] and [`ComputeCore::reverse_counted`] are the lookups the query core calls.

mod types;
mod utils;

pub use crate::types::compute_core::ComputeCore;
pub use crate::types::compute_session::ComputeSession;
pub use crate::types::core_listener::CoreListener;
pub use crate::types::frontier::Frontier;
pub use crate::types::lookup_at::LookupAt;
pub use crate::types::read_times::ReadTimes;
pub use crate::utils::keys::prefix_end;
pub use crate::utils::read_times_of::read_times_of;
