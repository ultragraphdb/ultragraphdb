/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! One host's settings: where each role listens and dials the system group, and the TOML they are written as. That the settings pass the config
//! check whole is `harness_tests`'s.

use ultragraphdb_tests::{
    CLUSTER, client_address, compute_observability, compute_on_one_host, compute_role_address,
    config_address, double_address, frontend_observability, frontend_on_one_host, not_hosting,
    render_settings, storage_observability, storage_on_one_host, storage_role_address,
    system_role_address,
};
use ultragraphdb_tool_bdd::{Context, PORT_BLOCK, TestResult, port};

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
    let settings = frontend_on_one_host()?;
    let tcp = |address: &str| format!("tcp://{address}");
    assert_eq!(
        setting(&settings, "observability.address")?,
        tcp(&frontend_observability()?)
    );
    assert_eq!(
        setting(&settings, "endpoint.address")?,
        tcp(&client_address()?)
    );
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
    let system = tcp(&system_role_address()?);
    let storage = storage_on_one_host()?;
    assert_eq!(setting(&storage, "system.role_address")?, system);
    for settings in [frontend_on_one_host()?, compute_on_one_host()?, storage] {
        assert_eq!(setting(&settings, "cluster.system_address")?, system);
    }
    let other = not_hosting(storage_on_one_host()?);
    assert!(
        other.iter().all(|(name, _)| !name.starts_with("system.")
            && !name.starts_with("sequencer.")
            && name != "cluster.partition_count"),
        "{other:?}"
    );
    assert_eq!(setting(&other, "cluster.system_address")?, system);
    Ok(())
}

#[test]
fn test_compute_and_storage_listen_at_their_declared_addresses() -> TestResult {
    let tcp = |address: &str| format!("tcp://{address}");
    for (settings, role_address, observability, own) in [
        (
            compute_on_one_host()?,
            "compute.role_address",
            compute_observability()?,
            compute_role_address()?,
        ),
        (
            storage_on_one_host()?,
            "storage.role_address",
            storage_observability()?,
            storage_role_address()?,
        ),
    ] {
        assert_eq!(setting(&settings, "cluster.id")?, CLUSTER.to_string());
        assert_eq!(setting(&settings, role_address)?, tcp(&own));
        assert_eq!(
            setting(&settings, "observability.address")?,
            tcp(&observability)
        );
    }
    Ok(())
}

/// Every declared address lies in the target's block, on the loopback, and no two share a port; the first storage
/// process's four partitions take the ports after its role address, which none of the others holds.
#[test]
fn test_the_declared_addresses_lie_apart_in_the_targets_block() -> TestResult {
    let declared = [
        client_address()?,
        system_role_address()?,
        compute_role_address()?,
        storage_role_address()?,
        config_address()?,
        double_address()?,
        frontend_observability()?,
        compute_observability()?,
        storage_observability()?,
    ];
    let (first, past) = (port(0)?, port(PORT_BLOCK - 1)?);
    let mut ports = Vec::new();
    for address in &declared {
        let held: u16 = address
            .strip_prefix("127.0.0.1:")
            .context(&format!("{address} on the loopback"))?
            .parse()?;
        assert!((first..=past).contains(&held), "{address}");
        ports.push(held);
    }
    let storage = ports.get(3).copied().context("storage's port")?;
    ports.extend((1..4).map(|partition| storage + partition));
    let count = ports.len();
    ports.sort_unstable();
    ports.dedup();
    assert_eq!(ports.len(), count, "{declared:?}");
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
