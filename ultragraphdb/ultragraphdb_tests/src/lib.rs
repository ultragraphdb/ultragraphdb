/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Starting the role binaries as processes, and asking them what they answer, for the tests that run them end to end.

mod types;
mod utils;

pub use crate::types::role_process::RoleProcess;
pub use crate::utils::http_status::http_status;
