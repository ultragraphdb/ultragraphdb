/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend role: where storage and compute join, the role address serving Join and Members, the frontend's
//! connection back to each role that joined, and the client endpoint's authentication.

mod types;
mod utils;

pub use crate::types::membership::Membership;
pub use crate::utils::frontend_role::frontend_role;
