/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The validation of settings files, which the config service and `ultragraphdb_config_cli` both call, so the two cannot
//! disagree. It needs no store, no network and no key.
//!
//! [`read_central`] reads a central file and every file it declares; [`check_target`] checks one target whole, every defect
//! found and not the first; [`check_central`] does both over a folder. A [`Defect`] states on its own where it stands, which
//! target, instance and setting, what was found and what is required, since this is the first thing an administrator meets.

mod types;
mod utils;

pub use crate::types::check_report::CheckReport;
pub use crate::types::defect::Defect;
pub use crate::types::instance::Instance;
pub use crate::types::target_files::TargetFiles;
pub use crate::utils::check_central::{check_central, read_central};
pub use crate::utils::check_target::{check_config_service, check_target};
pub use crate::utils::merge::instances;
