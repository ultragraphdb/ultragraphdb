/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What every role does on its role connections (`PROTOCOL.md` §11.12): reading the cluster's settings and the role
//! secret, verifying a peer locally against it, dialling and authenticating a peer as [`RoleClient`], and serving what
//! its endpoint takes through [`serve_connection`]. Storage and compute both join the frontend, which
//! [`compute_role`] does, and storage's own crate through [`retry_until`].

mod traits;
mod types;
mod utils;

pub use crate::traits::session::{Answer, Session};
pub use crate::types::connection_bound::ConnectionBound;
pub use crate::types::open_requests::OpenRequests;
pub use crate::types::role_client::{RoleClient, RoleReader, RoleWriter};
pub use crate::types::role_cluster::RoleCluster;
pub use crate::types::role_credentials::RoleCredentials;
pub use crate::types::role_secret::{
    COMPUTE_WELCOME_CREDIT, PARTITION_INITIAL_CREDIT, ROLE_ITERATIONS, RoleSecret,
};
pub use crate::utils::finish_open::finish_open;
pub use crate::utils::joining_role::{frontend_address, retry_until};
pub use crate::utils::serve::{Server, serve_connection};
pub use ultragraphdb_common_service::listen;
