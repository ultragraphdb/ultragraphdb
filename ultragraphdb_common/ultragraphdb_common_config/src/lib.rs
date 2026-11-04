/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The config service: every deployment target's settings as immutable numbered revisions, a state machine over an
//! [`OrderedLog`](ultragraphdb_traits_log::OrderedLog), and the HTTP API serving them.
//!
//! The log is the store, so the replication stage 8 builds beneath that trait carries the service unchanged. One task owns
//! the store, and the API reaches it by message, so a request's parsing and a sync's wait share nothing. A revision's body
//! is the request's body, validated and then logged as it came; each instance's merged settings are rendered once, when a
//! revision is stored or replayed, so a role's fetch is a lookup.

mod errors;
mod types;
mod utils;

pub use crate::errors::config_error::{ConfigError, ConfigErrorEnum};
pub use crate::types::config_api::ConfigApi;
pub use crate::types::config_handle::ConfigHandle;
pub use crate::types::config_record::ConfigRecord;
pub use crate::types::config_store::CONFIG_LOG_CLUSTER;
pub use crate::types::config_store::ConfigStore;
pub use crate::types::target_summary::TargetSummary;
pub use crate::utils::run_config_service::run_config_service;
pub use crate::utils::store_owner::store_owner;
