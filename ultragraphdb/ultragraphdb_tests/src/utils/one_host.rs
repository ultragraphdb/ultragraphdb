/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where each binary runs on one host in these tests: its name, its declared ports, and the settings that start it. The role
//! secret's file and the data directory are each scenario's own.

/// The frontend binary's name.
pub const FRONTEND_BINARY: &str = "ultragraphdb_frontend";

/// The address the frontend's `/health` and `/ready` listen at.
pub const FRONTEND_OBSERVABILITY: &str = "127.0.0.1:18081";

/// The cluster a role of this host joins, which every frame's header names.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// The address the system group listens at, in the storage process hosting it, which every role dials.
pub const SYSTEM_ROLE_ADDRESS: &str = "127.0.0.1:17081";

/// The address clients connect to.
pub const CLIENT_ADDRESS: &str = "127.0.0.1:17080";

/// The compute binary's name.
pub const COMPUTE_BINARY: &str = "ultragraphdb_compute";

/// The address compute's `/health` and `/ready` listen at.
pub const COMPUTE_OBSERVABILITY: &str = "127.0.0.1:18082";

/// The address the frontend dials compute at.
pub const COMPUTE_ROLE_ADDRESS: &str = "127.0.0.1:17082";

/// The storage binary's name.
pub const STORAGE_BINARY: &str = "ultragraphdb_storage";

/// The address storage's `/health` and `/ready` listen at.
pub const STORAGE_OBSERVABILITY: &str = "127.0.0.1:18083";

/// The address storage's partitions listen at, the first partition's.
pub const STORAGE_ROLE_ADDRESS: &str = "127.0.0.1:17083";

fn named(settings: &[(&str, String)]) -> Vec<(String, String)> {
    settings
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

/// The settings of a frontend dialling the system group, each named `table.key`.
#[must_use]
pub fn frontend_on_one_host() -> Vec<(String, String)> {
    named(&[
        (
            "observability.address",
            format!("tcp://{FRONTEND_OBSERVABILITY}"),
        ),
        ("cluster.id", CLUSTER.to_string()),
        (
            "cluster.system_address",
            format!("tcp://{SYSTEM_ROLE_ADDRESS}"),
        ),
        ("endpoint.address", format!("tcp://{CLIENT_ADDRESS}")),
        ("endpoint.plaintext", "true".to_string()),
    ])
}

/// The settings of a compute process joining the system group, each named `table.key`.
#[must_use]
pub fn compute_on_one_host() -> Vec<(String, String)> {
    named(&[
        (
            "observability.address",
            format!("tcp://{COMPUTE_OBSERVABILITY}"),
        ),
        ("cluster.id", CLUSTER.to_string()),
        (
            "cluster.system_address",
            format!("tcp://{SYSTEM_ROLE_ADDRESS}"),
        ),
        (
            "compute.role_address",
            format!("tcp://{COMPUTE_ROLE_ADDRESS}"),
        ),
    ])
}

/// The settings of a storage process of one partition hosting the system group, which forms the placement over that
/// partition, each named `table.key`: `[system]` but its `data_dir`, which is each scenario's own as `storage.data_dir` is.
#[must_use]
pub fn storage_on_one_host() -> Vec<(String, String)> {
    named(&[
        (
            "observability.address",
            format!("tcp://{STORAGE_OBSERVABILITY}"),
        ),
        ("cluster.id", CLUSTER.to_string()),
        (
            "cluster.system_address",
            format!("tcp://{SYSTEM_ROLE_ADDRESS}"),
        ),
        ("cluster.partition_count", "1".to_string()),
        (
            "storage.role_address",
            format!("tcp://{STORAGE_ROLE_ADDRESS}"),
        ),
        ("storage.partition_count", "1".to_string()),
        (
            "system.role_address",
            format!("tcp://{SYSTEM_ROLE_ADDRESS}"),
        ),
        ("sequencer.epoch_ms_min", "1".to_string()),
        ("sequencer.epoch_ms_max", "20".to_string()),
        // The `[metadata]` table a host holds, as the config service requires; 4,096 is the bound `PROTOCOL.md` §11.9
        // gives when it is not named, so the group runs as it would without it.
        ("metadata.sessions_max", "4096".to_string()),
    ])
}

/// `settings`, a storage process's, without what makes it the system group's host: the `[system]`, `[metadata]` and
/// `[sequencer]` tables and the cluster's partition count. Its partitions dial the group at `cluster.system_address`.
#[must_use]
pub fn not_hosting(settings: Vec<(String, String)>) -> Vec<(String, String)> {
    settings
        .into_iter()
        .filter(|(name, _)| {
            let table = name.split_once('.').map_or("", |(table, _)| table);
            !["system", "metadata", "sequencer"].contains(&table)
                && name != "cluster.partition_count"
        })
        .collect()
}

/// `settings`, each named `table.key`, as the TOML a settings file holds: a value of digits an integer, `true` or `false` a
/// boolean, and anything else a string.
#[must_use]
pub fn render_settings(settings: &[(String, String)]) -> String {
    let mut tables: std::collections::BTreeMap<&str, String> = std::collections::BTreeMap::new();
    for (name, value) in settings {
        let (table, key) = name.split_once('.').unwrap_or(("", name));
        let bare = value == "true"
            || value == "false"
            || (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()));
        let written = if bare {
            format!("{key} = {value}\n")
        } else {
            format!("{key} = \"{value}\"\n")
        };
        tables.entry(table).or_default().push_str(&written);
    }
    tables
        .iter()
        .map(|(table, keys)| format!("[{table}]\n{keys}"))
        .collect()
}
