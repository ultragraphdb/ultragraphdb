/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The settings every role binary runs on: [`ServiceConfig`], one type per table of the settings file and one field
//! per setting, so a table and key name one field and a field names one table and key.
//!
//! The file is optional. With none named the settings come from the environment alone, and an
//! `ULTRAGRAPHDB_<TABLE>_<KEY>` variable overrides the file's value either way. A setting neither names is absent:
//! the crate invents no default, and a role refuses a setting it requires through
//! [`ServiceConfigError::missing`]. Every table, key and variable is taken or refused, so a misspelling stops the start.
//!
//! The crate checks a value's kind and that it fits its field. A setting's bound belongs to the constructor of the domain
//! type it configures, the one place each bound is stated. The environment is passed in as data, and a role reads its
//! settings once, at start.

mod errors;
mod types;
mod utils;

pub use crate::errors::service_config_error::{ServiceConfigError, ServiceConfigErrorEnum};
pub use crate::types::admin_table::AdminTable;
pub use crate::types::cluster_table::ClusterTable;
pub use crate::types::compute_table::ComputeTable;
pub use crate::types::config_table::ConfigTable;
pub use crate::types::endpoint_security::EndpointSecurity;
pub use crate::types::endpoint_table::EndpointTable;
pub use crate::types::frontend_table::FrontendTable;
pub use crate::types::metadata_table::MetadataTable;
pub use crate::types::observability_table::ObservabilityTable;
pub use crate::types::runtime_table::RuntimeTable;
pub use crate::types::sequencer_table::SequencerTable;
pub use crate::types::service_config::ServiceConfig;
pub use crate::types::storage_table::StorageTable;
pub use crate::utils::partition_address::partition_address;
