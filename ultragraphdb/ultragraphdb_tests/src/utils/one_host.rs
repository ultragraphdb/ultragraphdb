/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where each binary runs on one host in these tests: its name, its declared ports, and the settings that start it. The role
//! secret's file and the data directory are each scenario's own.

use ultragraphdb_tool_bdd::{TestResult, loopback};

/// The cluster a role of this host joins, which every frame's header names.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// The compute binary's name.
pub const COMPUTE_BINARY: &str = "ultragraphdb_compute";

/// The storage binary's name.
pub const STORAGE_BINARY: &str = "ultragraphdb_storage";

/// The offsets above the target's port base (`ultragraphdb_tool_bdd::port`) of the addresses below. The first storage
/// process's partitions follow its role address, up to four of them; a test's own addresses take the offsets these leave
/// free: 2, 4 to 9, 12, 15 to 19, and from 20 on.
pub(crate) const CLIENT_OFFSET: u16 = 0;
/// The system group's role address, as [`CLIENT_OFFSET`] says.
pub(crate) const SYSTEM_ROLE_OFFSET: u16 = 1;
/// Storage's role address, its first partition's, as [`CLIENT_OFFSET`] says.
pub(crate) const STORAGE_ROLE_OFFSET: u16 = 3;
/// The config service's address, as [`CLIENT_OFFSET`] says.
pub(crate) const CONFIG_OFFSET: u16 = 10;
/// The config double's address, as [`CLIENT_OFFSET`] says.
pub(crate) const DOUBLE_OFFSET: u16 = 11;
/// Compute's observability, as [`CLIENT_OFFSET`] says.
pub(crate) const COMPUTE_OBSERVABILITY_OFFSET: u16 = 13;
/// Storage's observability, as [`CLIENT_OFFSET`] says.
pub(crate) const STORAGE_OBSERVABILITY_OFFSET: u16 = 14;

/// The address the system group listens at, in the storage process hosting it, which every role dials.
///
/// # Errors
///
/// When the target sets no port base, as `ultragraphdb_tool_bdd::port` says.
pub fn system_role_address() -> TestResult<String> {
    loopback(SYSTEM_ROLE_OFFSET)
}

/// The address clients connect to.
///
/// # Errors
///
/// As [`system_role_address`].
pub fn client_address() -> TestResult<String> {
    loopback(CLIENT_OFFSET)
}

/// The address compute's `/health` and `/ready` listen at.
///
/// # Errors
///
/// As [`system_role_address`].
pub fn compute_observability() -> TestResult<String> {
    loopback(COMPUTE_OBSERVABILITY_OFFSET)
}

/// The address storage's `/health` and `/ready` listen at.
///
/// # Errors
///
/// As [`system_role_address`].
pub fn storage_observability() -> TestResult<String> {
    loopback(STORAGE_OBSERVABILITY_OFFSET)
}

/// The address storage's partitions listen at, the first partition's.
///
/// # Errors
///
/// As [`system_role_address`].
pub fn storage_role_address() -> TestResult<String> {
    loopback(STORAGE_ROLE_OFFSET)
}

fn named(settings: &[(&str, String)]) -> Vec<(String, String)> {
    settings
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect()
}

/// The settings of a compute process dialling the system group and serving clients at [`client_address`] in clear text,
/// each named `table.key`.
///
/// # Errors
///
/// As [`system_role_address`].
pub fn compute_on_one_host() -> TestResult<Vec<(String, String)>> {
    Ok(named(&[
        (
            "observability.address",
            format!("tcp://{}", compute_observability()?),
        ),
        ("cluster.id", CLUSTER.to_string()),
        (
            "cluster.system_address",
            format!("tcp://{}", system_role_address()?),
        ),
        ("endpoint.address", format!("tcp://{}", client_address()?)),
        ("endpoint.plaintext", "true".to_string()),
    ]))
}

/// The settings of a storage process of one partition hosting the system group, which forms the placement over that
/// partition, each named `table.key`: `[system]` but its `data_dir`, which is each scenario's own as `storage.data_dir` is.
///
/// # Errors
///
/// As [`system_role_address`].
pub fn storage_on_one_host() -> TestResult<Vec<(String, String)>> {
    Ok(named(&[
        (
            "observability.address",
            format!("tcp://{}", storage_observability()?),
        ),
        ("cluster.id", CLUSTER.to_string()),
        (
            "cluster.system_address",
            format!("tcp://{}", system_role_address()?),
        ),
        ("cluster.partition_count", "1".to_string()),
        (
            "storage.role_address",
            format!("tcp://{}", storage_role_address()?),
        ),
        ("storage.partition_count", "1".to_string()),
        (
            "system.role_address",
            format!("tcp://{}", system_role_address()?),
        ),
        ("sequencer.epoch_ms_min", "1".to_string()),
        ("sequencer.epoch_ms_max", "20".to_string()),
        // The `[metadata]` table a host holds, as the config service requires; 4,096 is the bound `PROTOCOL.md` §11.9
        // gives when it is not named, so the group runs as it would without it.
        ("metadata.sessions_max", "4096".to_string()),
    ]))
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
