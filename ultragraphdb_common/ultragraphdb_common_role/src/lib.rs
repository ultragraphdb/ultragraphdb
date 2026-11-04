/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What every role does on its role connections (`PROTOCOL.md` §11.12): reading the cluster's settings and the role
//! secret, verifying a peer locally against it, dialling and authenticating a peer as [`RoleClient`], and serving what
//! its endpoint takes through [`serve_connection`]. Storage and compute both join the frontend, which
//! [`storage_role`] and [`compute_role`] do.

mod types;
mod utils;

pub use crate::types::role_client::RoleClient;
pub use crate::types::role_cluster::RoleCluster;
pub use crate::types::role_credentials::RoleCredentials;
pub use crate::types::role_secret::{ROLE_ITERATIONS, RoleSecret};
pub use crate::utils::joining_role::{compute_role, storage_role};
pub use crate::utils::serve::{Server, listen, serve_connection};
