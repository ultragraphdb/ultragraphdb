/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The system group: the metadata group, the sequencer and who joined, on one core of their own.
//!
//! The [`SystemGroup`] holds the metadata group, the sequencer and the [`Membership`] over any
//! [`ultragraphdb_traits_log::OrderedLog`]. Its sessions hold no copy of what it decides: they ask it through a
//! [`SystemHandle`], so an eviction takes effect for the next commit asked from anywhere. A turn of the group answers
//! everything its log's next sync makes durable, so the asks of a turn share one sync. [`start_system`] starts it on a
//! thread and runtime of its own over write-ahead logs with snapshots; a [`SystemSession`] serves one role connection.
//!
//! Planes (Ultra rule 3): the asks on a commit's path (the session checks, the commit's begin and end, a tenant's facts)
//! are the data plane, each allocating its reply's channel; a [`SystemSession`]'s frames are the `Vec` the wire's builders
//! return. The group's start, recovery and snapshots, joining, leases, closes and the building of a catalog version are
//! control plane.

mod types;
mod utils;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::types::membership::Membership;
pub use crate::types::report_asked::ReportAsked;
pub use crate::types::system_asked::DropStep;
pub use crate::types::system_group::SystemGroup;
pub use crate::types::system_handle::SystemHandle;
pub use crate::types::system_session::{SystemSession, SystemShared};
pub use crate::types::system_start::SystemStart;
pub use crate::types::tenant_facts::TenantFacts;
pub use crate::utils::system_thread::{SystemThread, start_system};
#[cfg(feature = "test-utils")]
pub use crate::utils_test::system_run::system_run;
