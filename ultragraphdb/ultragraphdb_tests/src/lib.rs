/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Asking the role binaries what they answer, and where each runs on one host, for the tests that run them end to end through
//! `service_utils`.

mod utils;

pub use crate::utils::harness::{
    ADMIN_KEY, CONFIG_BINARY, DOUBLE_REVISION, PARENT_PID, READ_KEY, SECRET, START_SECONDS, TARGET,
    answering, bootstrap, config_address, config_folder, config_folder_at, config_process,
    double_address, exit_is, fresh_data, frontend_holding, holding, key_file, members_at, process,
    role, role_of_service, scratch, served_by_double, start_config_service,
    start_config_service_at, start_double, storage_holding, until_answering, until_ports_free,
    with,
};
pub use crate::utils::http_status::{http_body, http_status};
pub use crate::utils::one_host::{
    CLUSTER, COMPUTE_BINARY, FRONTEND_BINARY, STORAGE_BINARY, client_address,
    compute_observability, compute_on_one_host, compute_role_address, frontend_observability,
    frontend_on_one_host, not_hosting, render_settings, storage_observability, storage_on_one_host,
    storage_role_address, system_role_address,
};
