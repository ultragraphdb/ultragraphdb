/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compute role's reads over storage's read kernels (`PROTOCOL.md` §11.11, §11.12, §13.12): a ComputeRead routed by its
//! bound positions, each partition it asks read by Scans at one version, every Entries stream held by an
//! [`ultragraphdb_protocol_wire::EntriesSequence`] and granted credit as its frames are consumed, the rows merged in key order
//! into Result batches naming every term by its text. One [`ComputeCore`] per core, whose connections and reads stay on it.
//! [`ComputeCore::value_index`] and [`ComputeCore::reverse_counted`] are the lookups the query core calls.
//!
//! Planes (Ultra rule 3): a read's Scans, the Entries frames its links hand on and the Result batches its session pushes
//! carry frames, the data plane; a frame sent is the `Vec` the wire's builders return, and a page of rows is filled from
//! the frame it reads. Dialling a partition and reading where it listens and the placement map are control plane.
//!
//! # The client endpoint
//!
//! The client endpoint, which this crate holds beside the read executor.
//!
//! Endpoint cores hold no copy of what the system group decides: each asks it through its [`SystemLink`], so an eviction
//! takes effect for the next commit on any core.
//!
//! Planes (Ultra rule 3): a [`ClientSession`]'s requests, the asks of the system group on a commit's path and the
//! [`RoleLinks`] they relay through carry frames, the data plane; a frame sent is the `Vec` the wire's builders return,
//! and each ask allocates its reply's channel. Authentication and its throttle, and the system group's link, are control plane.

mod traits;
mod types;
mod utils;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::traits::authorizer::Authorizer;
pub use crate::types::asked_credentials::AskedCredentials;
pub use crate::types::auth_throttle::AuthThrottle;
pub use crate::types::check_round_trips::CheckRoundTrips;
pub use crate::types::client_session::ClientSession;
pub use crate::types::compute_core::ComputeCore;
pub use crate::types::compute_session::ComputeSession;
pub use crate::types::endpoint_core::EndpointCore;
pub use crate::types::endpoint_limits::EndpointLimits;
pub use crate::types::endpoint_view::EndpointView;
pub use crate::types::every_authenticated::EveryAuthenticated;
pub use crate::types::frontier::Frontier;
pub use crate::types::gate_scope::GateScope;
pub use crate::types::hold_outcome::HoldOutcome;
pub use crate::types::login_limits::LoginLimits;
pub use crate::types::lookup_at::LookupAt;
pub use crate::types::one_frame_phases::{ONE_FRAME_PHASES, OneFramePhases};
pub use crate::types::read_times::ReadTimes;
pub use crate::types::recall_answer::RecallAnswer;
pub use crate::types::recall_decision::RecallDecision;
pub use crate::types::relay_authenticator::RelayAuthenticator;
pub use crate::types::relay_mechanism::RelayMechanism;
pub use crate::types::role_links::RoleLinks;
pub use crate::types::system_link::SystemLink;
pub use crate::types::system_turns::SystemTurns;
pub use crate::types::tenant_gate::TenantGate;
pub use crate::types::throttle_handle::ThrottleHandle;
pub use crate::types::turn::Turn;
pub use crate::utils::declare_bound::declare_fits;
pub use crate::utils::iri_place::{IriPlace, iri_place};
pub use crate::utils::joining::{hold_peers, join_group};
pub use crate::utils::keys::prefix_end;
pub use crate::utils::read_times_of::read_times_of;
pub use crate::utils::recall_order::{check_refusal, recall_decision, second_check_answer};
pub use crate::utils::term_rules::{check_change, check_operation, check_table};
#[cfg(feature = "test-utils")]
pub use crate::utils_test::double_frame::DoubleFrame;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::role_double::{
    RoleDouble, batch_frame, committed, empty_batch, last_frame, partition_answer, refusal_of,
    view_of,
};
#[cfg(feature = "test-utils")]
pub use crate::utils_test::system_play::SystemPlay;
