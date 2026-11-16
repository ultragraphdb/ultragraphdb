/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! One host's settings: where each role listens and dials the system group, and the TOML they are written as. That the settings pass the config
//! check whole is `harness_tests`'s.

use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, COMPUTE_OBSERVABILITY, COMPUTE_ROLE_ADDRESS, FRONTEND_OBSERVABILITY,
    STORAGE_OBSERVABILITY, STORAGE_ROLE_ADDRESS, SYSTEM_ROLE_ADDRESS, compute_on_one_host,
    frontend_on_one_host, not_hosting, render_settings, storage_on_one_host,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The setting `name` of `settings`.
fn setting(settings: &[(String, String)], name: &str) -> TestResult<String> {
    settings
        .iter()
        .find(|(held, _)| held == name)
        .map(|(_, value)| value.clone())
        .context(&format!("the setting {name}"))
}

#[test]
fn test_the_frontend_listens_at_its_declared_addresses() -> TestResult {
    let settings = frontend_on_one_host();
    let tcp = |address: &str| format!("tcp://{address}");
    assert_eq!(
        setting(&settings, "observability.address")?,
        tcp(FRONTEND_OBSERVABILITY)
    );
    assert_eq!(setting(&settings, "endpoint.address")?, tcp(CLIENT_ADDRESS));
    assert!(
        settings
            .iter()
            .all(|(name, _)| !name.starts_with("frontend.")),
        "the frontend holds no role address and no data: {settings:?}"
    );
    assert_eq!(setting(&settings, "cluster.id")?, CLUSTER.to_string());
    Ok(())
}

/// Storage hosts the system group at the address every role dials: its `[system]` table's role address.
#[test]
fn test_storage_hosts_the_system_group_every_role_dials() -> TestResult {
    let tcp = |address: &str| format!("tcp://{address}");
    let storage = storage_on_one_host();
    assert_eq!(
        setting(&storage, "system.role_address")?,
        tcp(SYSTEM_ROLE_ADDRESS)
    );
    for settings in [frontend_on_one_host(), compute_on_one_host(), storage] {
        assert_eq!(
            setting(&settings, "cluster.system_address")?,
            tcp(SYSTEM_ROLE_ADDRESS)
        );
    }
    let other = not_hosting(storage_on_one_host());
    assert!(
        other.iter().all(|(name, _)| !name.starts_with("system.")
            && !name.starts_with("sequencer.")
            && name != "cluster.partition_count"),
        "{other:?}"
    );
    assert_eq!(
        setting(&other, "cluster.system_address")?,
        tcp(SYSTEM_ROLE_ADDRESS)
    );
    Ok(())
}

#[test]
fn test_compute_and_storage_listen_at_their_declared_addresses() -> TestResult {
    let tcp = |address: &str| format!("tcp://{address}");
    for (settings, role_address, observability, own) in [
        (
            compute_on_one_host(),
            "compute.role_address",
            COMPUTE_OBSERVABILITY,
            COMPUTE_ROLE_ADDRESS,
        ),
        (
            storage_on_one_host(),
            "storage.role_address",
            STORAGE_OBSERVABILITY,
            STORAGE_ROLE_ADDRESS,
        ),
    ] {
        assert_eq!(setting(&settings, "cluster.id")?, CLUSTER.to_string());
        assert_eq!(setting(&settings, role_address)?, tcp(own));
        assert_eq!(
            setting(&settings, "observability.address")?,
            tcp(observability)
        );
    }
    Ok(())
}

#[test]
fn test_settings_render_as_tables_of_typed_values() {
    let settings: Vec<(String, String)> = [
        ("b.count", "12"),
        ("a.on", "true"),
        ("a.address", "tcp://h:1"),
        ("b.off", "false"),
        ("b.empty", ""),
        ("b.mixed", "1a"),
    ]
    .map(|(name, value)| (name.to_string(), value.to_string()))
    .to_vec();
    let expected = "[a]\non = true\naddress = \"tcp://h:1\"\n[b]\ncount = 12\noff = false\nempty = \"\"\nmixed = \"1a\"\n";
    assert_eq!(render_settings(&settings), expected);
}
