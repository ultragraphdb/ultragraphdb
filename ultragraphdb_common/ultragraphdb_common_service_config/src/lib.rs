/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The settings every role binary runs on: [`ServiceConfig`], one type per table of the settings file and one field
//! per setting, so a table and key name one field and a field names one table and key. `[frontend]` is a table with no
//! setting: the frontend holds nothing of its own since the system group left it, and a key there is refused by its name.
//!
//! The settings are the bytes of one file, which [`ServiceConfig::parse`] reads: a role's come from the config service,
//! and the config service's own from the file it is started with. A setting the file does not name is absent: the crate
//! invents no default, and a role refuses a setting it requires through [`ServiceConfigError::missing`]. A table
//! whose presence means something requires its settings itself: a `[system]` table, which makes a storage instance the
//! system group's host, is refused without `role_address` or `data_dir`, so [`SystemTable`] holds both. Every table and
//! key is taken or refused, so a misspelling stops the start.
//!
//! The crate checks a value's kind and that it fits its field. A setting's bound belongs to the constructor of the domain
//! type it configures, the one place each bound is stated. A role reads its settings once, at start: the crate is
//! control plane alone (Ultra rule 3), and allocates as it reads.

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
pub use crate::types::metadata_table::MetadataTable;
pub use crate::types::observability_table::ObservabilityTable;
pub use crate::types::runtime_table::RuntimeTable;
pub use crate::types::sequencer_table::SequencerTable;
pub use crate::types::service_config::ServiceConfig;
pub use crate::types::storage_table::StorageTable;
pub use crate::types::system_table::SystemTable;
pub use crate::utils::partition_address::partition_address;
