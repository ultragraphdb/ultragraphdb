/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What every role binary does around its role: reading its settings, a runtime per core, `/health` and `/ready`, and
//! stopping on a signal. A binary holds only `main`, which names its role and calls [`run_service`].

mod types;
mod utils;

pub use crate::types::admin_service::AdminService;
pub use crate::types::bootstrap::Bootstrap;
pub use crate::types::cpu_binding::CpuBinding;
pub use crate::types::health_service::HealthService;
pub use crate::types::readiness::Readiness;
pub use crate::types::role_identity::RoleIdentity;
pub use crate::types::role_kind::RoleKind;
pub use crate::types::role_start::RoleStart;
pub use crate::types::service_end::ServiceEnd;
pub use crate::types::stopping::Stopping;
pub use crate::utils::cpu_list::{allowed_in_status, parse_cpu_list};
pub use crate::utils::fetch_settings::fetch_settings;
pub use crate::utils::listen::listen;
pub use crate::utils::run_service::{
    binding, core_runtime, driver_name, exit_status, role_help, run_service, serve_service,
    stop_flag,
};
