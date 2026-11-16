/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What every role does on its role connections (`PROTOCOL.md` §11.12): reading the cluster's settings and the role
//! secret, verifying a peer locally against it, dialling and authenticating a peer as [`RoleClient`], and serving what
//! its endpoint takes through [`serve_connection`]. Storage and compute both join the frontend their settings name,
//! [`frontend_address`], through [`retry_until`]; [`system_address`] is the system group's address the settings name,
//! which every role dials.
//!
//! Planes (Ultra rule 3): [`serve_connection`], [`RoleClient`]'s requests and the halves it splits into carry frames,
//! the data plane, each read into a buffer of a pool sized when the connection starts; a frame sent is the `Vec` the
//! wire's builders return. The settings, the role secret, dialling, authenticating and joining are control plane.

mod traits;
mod types;
mod utils;

pub use crate::traits::session::{Answer, Push, Session};
pub use crate::types::connection_bound::ConnectionBound;
pub use crate::types::open_frame::OpenFrame;
pub use crate::types::open_requests::OpenRequests;
pub use crate::types::parked_bytes::ParkedBytes;
pub use crate::types::role_client::RoleClient;
pub use crate::types::role_cluster::RoleCluster;
pub use crate::types::role_credentials::RoleCredentials;
pub use crate::types::role_reader::RoleReader;
pub use crate::types::role_secret::{
    COMPUTE_WELCOME_CREDIT, PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleSecret,
};
pub use crate::types::role_writer::RoleWriter;
pub use crate::utils::finish_open::finish_open;
pub use crate::utils::joining_role::{frontend_address, retry_until, system_address};
pub use crate::utils::serve::{Server, serve_connection, serve_connection_with};
pub use ultragraphdb_common_service::listen;
