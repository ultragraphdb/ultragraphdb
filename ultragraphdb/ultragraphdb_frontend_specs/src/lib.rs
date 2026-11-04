/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where the frontend binary runs on one host, declared once for whatever starts it: a test harness, a profile.

/// The binary's name.
pub const BINARY: &str = "ultragraphdb_frontend";

/// The address its `/health` and `/ready` listen at on one host, which no other role's declares.
pub const OBSERVABILITY: &str = "127.0.0.1:18081";

/// The cluster a role of this host joins, which every frame's header names.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// The address the roles join at on one host.
pub const ROLE_ADDRESS: &str = "127.0.0.1:17081";

/// The address clients connect to on one host.
pub const CLIENT_ADDRESS: &str = "127.0.0.1:17080";

/// The settings that start it on one host, each named `table.key`, and one partition to form the
/// placement over. The role secret's file and the data directory are the starter's: a deployment's own, which no specification names.
#[must_use]
pub fn settings() -> Vec<(String, String)> {
    [
        ("observability.address", format!("tcp://{OBSERVABILITY}")),
        ("cluster.id", CLUSTER.to_string()),
        ("cluster.partition_count", "1".to_string()),
        ("frontend.role_address", format!("tcp://{ROLE_ADDRESS}")),
        ("endpoint.address", format!("tcp://{CLIENT_ADDRESS}")),
        ("endpoint.plaintext", "true".to_string()),
        ("sequencer.epoch_ms_min", "1".to_string()),
        ("sequencer.epoch_ms_max", "20".to_string()),
    ]
    .map(|(name, value)| (name.to_string(), value))
    .to_vec()
}

/// `settings`, each named `table.key`, as the TOML a settings file holds: a value of digits an integer, `true` or `false` a
/// boolean, and anything else a string.
#[must_use]
pub fn render(settings: &[(String, String)]) -> String {
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
