/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend role: the client endpoint, and where storage and compute join.
//!
//! Endpoint cores hold no copy of what the system group decides: they ask it through an
//! [`ultragraphdb_common_system::SystemHandle`], so an eviction takes effect for the next commit on any core.
//!
//! Planes (Ultra rule 3): a [`ClientSession`]'s requests, the asks of the system group on a commit's path and the
//! [`RoleLinks`] they relay through carry frames, the data plane; a frame sent is the `Vec` the wire's builders return,
//! and each ask allocates its reply's channel. Authentication and its throttle, and joining, are control plane.

mod traits;
mod types;
mod utils;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::traits::authorizer::Authorizer;
pub use crate::types::asked_credentials::AskedCredentials;
pub use crate::types::auth_throttle::AuthThrottle;
pub use crate::types::client_session::ClientSession;
pub use crate::types::core_joins::CoreJoins;
pub use crate::types::endpoint_core::EndpointCore;
pub use crate::types::endpoint_limits::EndpointLimits;
pub use crate::types::every_authenticated::EveryAuthenticated;
pub use crate::types::gate_scope::GateScope;
pub use crate::types::join_fan_out::JoinFanOut;
pub use crate::types::joined_roles::JoinedRoles;
pub use crate::types::login_limits::LoginLimits;
pub use crate::types::one_frame_phases::{ONE_FRAME_PHASES, OneFramePhases};
pub use crate::types::role_links::RoleLinks;
pub use crate::types::tenant_gate::TenantGate;
pub use crate::types::throttle_handle::ThrottleHandle;
pub use crate::utils::iri_place::{IriPlace, iri_place};
pub use crate::utils::term_rules::{check_change, check_operation, check_table};
#[cfg(feature = "test-utils")]
pub use crate::utils_test::double_frame::DoubleFrame;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::role_double::{
    RoleDouble, batch_frame, empty_batch, last_frame, partition_answer, refusal_of, view_of,
};
