/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Asking the role binaries what they answer, and where each runs on one host, for the tests that run them end to end through
//! `service_utils`.

mod utils;

pub use crate::utils::harness::{
    ADMIN_KEY, CONFIG_ADDRESS, CONFIG_BINARY, DOUBLE_ADDRESS, DOUBLE_REVISION, READ_KEY, SECRET,
    START_SECONDS, TARGET, answering, bootstrap, config_folder, config_process, exit_is,
    fresh_data, frontend_holding, holding, key_file, members_at, process, role, role_of_service,
    scratch, served_by_double, start_config_service, start_double, storage_holding,
    until_answering, with,
};
pub use crate::utils::http_status::{http_body, http_status};
pub use crate::utils::one_host::{
    CLIENT_ADDRESS, CLUSTER, COMPUTE_BINARY, COMPUTE_OBSERVABILITY, COMPUTE_ROLE_ADDRESS,
    FRONTEND_BINARY, FRONTEND_OBSERVABILITY, FRONTEND_ROLE_ADDRESS, STORAGE_BINARY,
    STORAGE_OBSERVABILITY, STORAGE_ROLE_ADDRESS, compute_on_one_host, frontend_on_one_host,
    render_settings, storage_on_one_host,
};
