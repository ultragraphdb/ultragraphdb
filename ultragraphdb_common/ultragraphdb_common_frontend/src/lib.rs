/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend role: where storage and compute join, and the hosts of the metadata group and the sequencer.
//!
//! The [`Owner`] holds the metadata group, the sequencer and who joined on one core, over any
//! [`ultragraphdb_traits_log::OrderedLog`]. Endpoint cores hold no copy of what it decides: they ask it through an
//! [`OwnerHandle`], so an eviction takes effect for the next commit on any core. A turn of the owner answers everything
//! its log's next sync makes durable, so the asks of a turn share one sync.

mod traits;
mod types;
mod utils;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::traits::authorizer::Authorizer;
pub use crate::types::asked_credentials::AskedCredentials;
pub use crate::types::auth_throttle::AuthThrottle;
pub use crate::types::client_session::ClientSession;
pub use crate::types::endpoint_core::EndpointCore;
pub use crate::types::endpoint_limits::EndpointLimits;
pub use crate::types::every_authenticated::EveryAuthenticated;
pub use crate::types::gate_scope::GateScope;
pub use crate::types::membership::Membership;
pub use crate::types::owner::Owner;
pub use crate::types::owner_handle::DropStep;
pub use crate::types::owner_handle::OwnerHandle;
pub use crate::types::owner_start::OwnerStart;
pub use crate::types::role_links::RoleLinks;
pub use crate::types::role_session::{RoleSession, RoleShared};
pub use crate::types::tenant_facts::TenantFacts;
pub use crate::types::tenant_gate::TenantGate;
pub use crate::utils::iri_place::{IriPlace, iri_place};
pub use crate::utils::owner_thread::{OwnerThread, start_owner};
#[cfg(feature = "test-utils")]
pub use crate::utils_test::role_double::{
    DoubleFrame, RoleDouble, batch_frame, empty_batch, last_frame, partition_answer, refusal_of,
    view_of,
};
