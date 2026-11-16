/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The settings in use beside the new ones, and the moved tables in a storage instance's file (design D2): laws L2.3
//! and L2.4.

use hegel::TestCase;
use ultragraphdb_common_service_config::ServiceConfig;

use crate::utils::drawn::{address, coin, path, u32_setting, u64_setting};

/// Settings in use before this group, as drawn: each `(table, key, value)`, the value as the file writes it.
struct InUse {
    frontend_address: String,
    id: u64,
    partition_count: u32,
    frontend_role_address: String,
    frontend_data_dir: String,
    storage_role_address: String,
    storage_data_dir: String,
    sessions_max: u32,
    epoch_ms_min: u64,
    epoch_ms_max: u64,
}

impl InUse {
    fn draw(tc: &TestCase) -> Self {
        Self {
            frontend_address: address(tc),
            id: u64_setting(tc),
            partition_count: u32_setting(tc),
            frontend_role_address: address(tc),
            frontend_data_dir: path(tc),
            storage_role_address: address(tc),
            storage_data_dir: path(tc),
            sessions_max: u32_setting(tc),
            epoch_ms_min: u64_setting(tc),
            epoch_ms_max: u64_setting(tc),
        }
    }

    /// The file holding these settings, `cluster` lines added to `[cluster]`, and `tail` after the last table.
    fn text(&self, cluster: &str, tail: &str) -> String {
        format!(
            "[cluster]\nfrontend_address = \"{}\"\nid = {}\npartition_count = {}\n{cluster}\
             [frontend]\nrole_address = \"{}\"\ndata_dir = \"{}\"\n\
             [storage]\nrole_address = \"{}\"\ndata_dir = \"{}\"\n\
             [metadata]\nsessions_max = {}\n\
             [sequencer]\nepoch_ms_min = {}\nepoch_ms_max = {}\n{tail}",
            self.frontend_address,
            self.id,
            self.partition_count,
            self.frontend_role_address,
            self.frontend_data_dir,
            self.storage_role_address,
            self.storage_data_dir,
            self.sessions_max,
            self.epoch_ms_min,
            self.epoch_ms_max,
        )
    }

    /// Whether `config` holds each of these settings as drawn.
    fn read_in(&self, config: &ServiceConfig) -> bool {
        config.cluster().frontend_address() == Some(self.frontend_address.as_str())
            && config.cluster().id() == Some(self.id)
            && config.cluster().partition_count() == Some(self.partition_count)
            && config.frontend().role_address() == Some(self.frontend_role_address.as_str())
            && config.frontend().data_dir() == Some(self.frontend_data_dir.as_str())
            && config.storage().role_address() == Some(self.storage_role_address.as_str())
            && config.storage().data_dir() == Some(self.storage_data_dir.as_str())
            && config.metadata().sessions_max() == Some(self.sessions_max)
            && config.sequencer().epoch_ms_min() == Some(self.epoch_ms_min)
            && config.sequencer().epoch_ms_max() == Some(self.epoch_ms_max)
    }
}

/// L2.3: a file of the settings in use reads every one of them as drawn, with or without the `[system]` table and
/// `cluster.system_address` added, and the new read as present exactly when added.
#[hegel::test]
fn test_the_settings_in_use_read_alike_with_and_without_the_new(tc: TestCase) {
    let in_use = InUse::draw(&tc);
    let system_address = coin(&tc).then(|| address(&tc));
    let system = coin(&tc).then(|| (address(&tc), path(&tc)));
    let cluster = system_address
        .as_ref()
        .map(|address| format!("system_address = \"{address}\"\n"))
        .unwrap_or_default();
    let tail = system
        .as_ref()
        .map(|(role_address, data_dir)| {
            format!("[system]\nrole_address = \"{role_address}\"\ndata_dir = \"{data_dir}\"\n")
        })
        .unwrap_or_default();
    let without = ServiceConfig::parse(in_use.text("", "").as_bytes()).ok();
    let with = ServiceConfig::parse(in_use.text(&cluster, &tail).as_bytes()).ok();
    assert!(
        without
            .as_ref()
            .is_some_and(|config| in_use.read_in(config))
    );
    assert!(with.as_ref().is_some_and(|config| in_use.read_in(config)));
    assert_eq!(
        with.as_ref()
            .and_then(|config| config.cluster().system_address()),
        system_address.as_deref()
    );
    assert_eq!(
        with.as_ref().and_then(ServiceConfig::system).map(|table| (
            table.role_address().to_string(),
            table.data_dir().to_string()
        )),
        system
    );
}

/// The `[metadata]` and `[sequencer]` settings drawn, each present or absent.
struct Moved {
    block_size_max: Option<u64>,
    sessions_max: Option<u32>,
    snapshot_rounds: Option<u32>,
    bootstrap_secret_file: Option<String>,
    catalog_check_ms: Option<u64>,
    epoch_ms_min: Option<u64>,
    epoch_ms_max: Option<u64>,
}

impl Moved {
    fn draw(tc: &TestCase) -> Self {
        Self {
            block_size_max: coin(tc).then(|| u64_setting(tc)),
            sessions_max: coin(tc).then(|| u32_setting(tc)),
            snapshot_rounds: coin(tc).then(|| u32_setting(tc)),
            bootstrap_secret_file: coin(tc).then(|| path(tc)),
            catalog_check_ms: coin(tc).then(|| u64_setting(tc)),
            epoch_ms_min: coin(tc).then(|| u64_setting(tc)),
            epoch_ms_max: coin(tc).then(|| u64_setting(tc)),
        }
    }

    /// The two tables, each setting drawn present written out.
    fn text(&self) -> String {
        let line = |key: &str, value: Option<String>| {
            value
                .map(|value| format!("{key} = {value}\n"))
                .unwrap_or_default()
        };
        [
            "[metadata]\n".to_string(),
            line("block_size_max", self.block_size_max.map(|v| v.to_string())),
            line("sessions_max", self.sessions_max.map(|v| v.to_string())),
            line(
                "snapshot_rounds",
                self.snapshot_rounds.map(|v| v.to_string()),
            ),
            line(
                "bootstrap_secret_file",
                self.bootstrap_secret_file
                    .as_ref()
                    .map(|v| format!("\"{v}\"")),
            ),
            line(
                "catalog_check_ms",
                self.catalog_check_ms.map(|v| v.to_string()),
            ),
            "[sequencer]\n".to_string(),
            line("epoch_ms_min", self.epoch_ms_min.map(|v| v.to_string())),
            line("epoch_ms_max", self.epoch_ms_max.map(|v| v.to_string())),
        ]
        .concat()
    }

    /// Whether `config` holds each setting as drawn, an absent one as absent.
    fn read_in(&self, config: &ServiceConfig) -> bool {
        let metadata = config.metadata();
        let sequencer = config.sequencer();
        metadata.block_size_max() == self.block_size_max
            && metadata.sessions_max() == self.sessions_max
            && metadata.snapshot_rounds() == self.snapshot_rounds
            && metadata.bootstrap_secret_file() == self.bootstrap_secret_file.as_deref()
            && metadata.catalog_check_ms() == self.catalog_check_ms
            && sequencer.epoch_ms_min() == self.epoch_ms_min
            && sequencer.epoch_ms_max() == self.epoch_ms_max
    }
}

/// L2.4: `[metadata]` and `[sequencer]` with drawn values in a storage instance's file, hosting or not, read back as
/// drawn, and equal to what the same text reads as in a frontend's file.
#[hegel::test]
fn test_the_moved_tables_read_alike_in_a_storage_and_a_frontend_file(tc: TestCase) {
    let moved = Moved::draw(&tc);
    let hosting = if coin(&tc) {
        format!(
            "[system]\nrole_address = \"{}\"\ndata_dir = \"{}\"\n",
            address(&tc),
            path(&tc)
        )
    } else {
        String::new()
    };
    let storage = format!(
        "[storage]\nrole_address = \"{}\"\ndata_dir = \"{}\"\n{hosting}{}",
        address(&tc),
        path(&tc),
        moved.text()
    );
    let frontend = format!(
        "[frontend]\nrole_address = \"{}\"\ndata_dir = \"{}\"\n[endpoint]\nplaintext = true\n{}",
        address(&tc),
        path(&tc),
        moved.text()
    );
    let in_storage = ServiceConfig::parse(storage.as_bytes()).ok();
    let in_frontend = ServiceConfig::parse(frontend.as_bytes()).ok();
    assert!(
        in_storage
            .as_ref()
            .is_some_and(|config| moved.read_in(config))
    );
    assert_eq!(
        in_storage.as_ref().map(|config| config.system().is_some()),
        Some(!hosting.is_empty())
    );
    assert_eq!(
        in_storage
            .as_ref()
            .map(|config| (config.metadata().clone(), config.sequencer().clone())),
        in_frontend
            .as_ref()
            .map(|config| (config.metadata().clone(), config.sequencer().clone()))
    );
}
