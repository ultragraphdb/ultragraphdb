/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What only the merged settings, or the set of them, can show: the checks of the config-service spec's table, each
//! reached through `check_target`.

use ultragraphdb_cluster_placement::PlacementMap;
use ultragraphdb_common_config_check::{Defect, TargetFiles, check_target};
use ultragraphdb_tool_bdd::TestResult;

const FRONTEND: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nsystem_address = \"tcp://10.0.0.2:7100\"\n[observability]\naddress = \"tcp://127.0.0.1:18081\"\n";
const STORAGE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nsystem_address = \"tcp://10.0.0.2:7100\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[storage]\npartition_count = 1\ndata_dir = \"/data/storage\"\n";
/// The instance hosting the system group: its `[system]` table, the two moved tables and the partition count it forms the
/// placement over. Its `[storage]` table comes last, so a key appended to it lands there.
const STORAGE_1: &str = "[cluster]\npartition_count = 2\n[metadata]\n[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n[system]\nrole_address = \"tcp://10.0.0.2:7100\"\ndata_dir = \"/data/system\"\n[storage]\npartition_first = 1\nrole_address = \"tcp://10.0.0.2:7000\"\n";
const STORAGE_2: &str = "[storage]\npartition_first = 2\nrole_address = \"tcp://10.0.0.4:7000\"\n";
const COMPUTE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nsystem_address = \"tcp://10.0.0.2:7100\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[compute]\nrole_address = \"tcp://10.0.0.3:7082\"\n";

/// The host's `system.role_address`, which every `cluster.system_address` of the fixtures names.
const SYSTEM_ROLE_ADDRESS: &str = "tcp://10.0.0.2:7100";
/// The host's `system.data_dir`.
const SYSTEM_DATA_DIR: &str = "/data/system";
/// Every storage instance's `storage.data_dir`.
const STORAGE_DATA_DIR: &str = "/data/storage";

/// The SHA-256 digest of no bytes, as FIPS 180-4's test vectors give it.
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// A target of `files`, each labelled by its key.
fn target_of(files: &[(&str, &str)]) -> TargetFiles {
    let mut target = TargetFiles::new("ci");
    for (key, text) in files {
        target.insert(*key, format!("ci_{key}.toml"), text.as_bytes().to_vec());
    }
    target
}

/// A valid target: one frontend, two storage instances of one partition each, one compute.
fn valid() -> TargetFiles {
    target_of(&[
        ("frontend", FRONTEND),
        ("storage", STORAGE),
        ("storage_1", STORAGE_1),
        ("storage_2", STORAGE_2),
        ("compute_1", COMPUTE),
    ])
}

/// `target` with `key`'s file holding `text` instead.
fn with(mut target: TargetFiles, key: &str, text: &str) -> TargetFiles {
    target.insert(key, format!("ci_{key}.toml"), text.as_bytes().to_vec());
    target
}

/// Every defect of `target` against `active`, one per line.
fn checked(target: &TargetFiles, active: Option<&TargetFiles>) -> String {
    check_target(target, active)
        .iter()
        .map(Defect::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn says(listing: &str, words: &[&str]) {
    for word in words {
        assert!(listing.contains(word), "{word} in:\n{listing}");
    }
}

fn says_not(listing: &str, words: &[&str]) {
    for word in words {
        assert!(!listing.contains(word), "{word} not in:\n{listing}");
    }
}

/// The valid target passes, by itself and against itself as the active revision.
#[test]
fn test_the_valid_target_passes() {
    assert_eq!(checked(&valid(), None), "");
    assert_eq!(checked(&valid(), Some(&valid())), "");
}

/// A target has exactly one frontend and at least one storage instance.
#[test]
fn test_a_target_holds_one_frontend_and_a_storage_instance() {
    let two = target_of(&[
        ("frontend_1", FRONTEND),
        ("frontend_2", FRONTEND),
        ("storage", STORAGE),
        ("storage_1", STORAGE_1),
        ("storage_2", STORAGE_2),
    ]);
    says(
        &checked(&two, None),
        &["target ci, the target has 2 frontends"],
    );
    let none = target_of(&[("storage", STORAGE), ("storage_1", STORAGE_1)]);
    says(
        &checked(&none, None),
        &["target ci, the target has 0 frontends"],
    );
    let no_storage = target_of(&[("frontend", FRONTEND), ("compute_1", COMPUTE)]);
    says(
        &checked(&no_storage, None),
        &["target ci, the target has no storage instance"],
    );
}

/// The host's sequencer and metadata settings are bounded by their domain types.
#[test]
fn test_a_host_bound_is_checked_by_its_domain_type() {
    let reversed = STORAGE_1.replace("epoch_ms_min = 2", "epoch_ms_min = 11");
    says(
        &checked(&with(valid(), "storage_1", &reversed), None),
        &["instance storage_1, setting sequencer.epoch_ms_max: 11 to 10 ms is refused"],
    );
    let sessions = STORAGE_1.replace("[metadata]\n", "[metadata]\nsessions_max = 0\n");
    says(
        &checked(&with(valid(), "storage_1", &sessions), None),
        &["instance storage_1, setting metadata.sessions_max: it holds 0;"],
    );
}

/// A Scan's stall of 30,000 ms, the floor of a client's read timeout, is taken; only one above it is refused.
#[test]
fn test_a_scan_stall_at_its_bound_passes() {
    let at = format!("{STORAGE_1}scan_stall_ms = 30000\n");
    assert_eq!(checked(&with(valid(), "storage_1", &at), None), "");
}

/// Each address a role is reached at parses, or is refused naming its setting and value.
#[test]
fn test_a_role_address_that_does_not_parse_is_refused() {
    let cases = [
        (
            "storage_1",
            STORAGE_1.replace(SYSTEM_ROLE_ADDRESS, "ftp://10.0.0.2:7100"),
            "instance storage_1, setting system.role_address: it holds ftp://10.0.0.2:7100;",
        ),
        (
            "storage_1",
            STORAGE_1.replace("tcp://10.0.0.2:7000", "ftp://10.0.0.2:7000"),
            "instance storage_1, setting storage.role_address: it holds ftp://10.0.0.2:7000;",
        ),
        (
            "compute_1",
            COMPUTE.replace("tcp://10.0.0.3", "ftp://10.0.0.3"),
            "instance compute_1, setting compute.role_address: it holds ftp://10.0.0.3:7082;",
        ),
        (
            "compute_1",
            COMPUTE.replace(SYSTEM_ROLE_ADDRESS, "ftp://10.0.0.2:7100"),
            "instance compute_1, setting cluster.system_address: it holds ftp://10.0.0.2:7100;",
        ),
    ];
    for (key, text, expected) in cases {
        says(&checked(&with(valid(), key, &text), None), &[expected]);
    }
}

/// A listener names TLS or clear text, and an admin listener the admin key's digest.
#[test]
fn test_a_listener_names_its_security() {
    let endpoint = format!("{FRONTEND}[endpoint]\naddress = \"tcp://10.0.0.1:9080\"\n");
    says(
        &checked(&with(valid(), "frontend", &endpoint), None),
        &["instance frontend, setting endpoint.plaintext:"],
    );
    let admin = format!("{FRONTEND}[admin]\naddress = \"tcp://10.0.0.1:9090\"\n");
    says(
        &checked(&with(valid(), "frontend", &admin), None),
        &[
            "instance frontend, setting admin.plaintext:",
            "instance frontend, setting admin.key_sha256: it is absent;",
        ],
    );
}

/// A secret's path is absolute, and the admin key's digest is 64 hexadecimal digits.
#[test]
fn test_secrets_are_absolute_paths_and_a_digest() {
    let relative = FRONTEND.replace("\"/run/secret\"", "\"run/secret\"");
    says(
        &checked(&with(valid(), "frontend", &relative), None),
        &["instance frontend, setting cluster.role_secret_file: it holds run/secret;"],
    );
    let digest = |text: &str| format!("{COMPUTE}[admin]\nkey_sha256 = \"{text}\"\n");
    assert_eq!(
        checked(&with(valid(), "compute_1", &digest(EMPTY_SHA256)), None),
        ""
    );
    let short = EMPTY_SHA256.get(1..).unwrap_or_default();
    let not_hex = "g".repeat(EMPTY_SHA256.len());
    for wrong in [short, not_hex.as_str()] {
        says(
            &checked(&with(valid(), "compute_1", &digest(wrong)), None),
            &["instance compute_1, setting admin.key_sha256: it is not a SHA-256 digest"],
        );
    }
}

/// Every instance of a target names one cluster.
#[test]
fn test_two_cluster_identities_are_refused() {
    let other = format!("{STORAGE_2}[cluster]\nid = 8\n");
    says(
        &checked(&with(valid(), "storage_2", &other), None),
        &[
            "instance storage_2, setting cluster.id: it holds 8, and",
            "holds 7;",
        ],
    );
}

/// A storage instance whose file is refused holds partitions no one can read, so coverage accuses no other.
#[test]
fn test_a_refused_storage_file_leaves_coverage_unchecked() {
    let refused = format!("{STORAGE_2}bogus = 1\n");
    let listing = checked(&with(valid(), "storage_2", &refused), None);
    says(&listing, &["storage.bogus"]);
    says_not(&listing, &["no storage instance holds partition"]);
}

/// The system group's role address takes part in the address checks: it cannot be a wildcard, nor share an address with a
/// partition's (C7.26).
#[test]
fn test_the_system_role_address_takes_part_in_the_address_checks() -> TestResult {
    let wildcard = STORAGE_1.replace(SYSTEM_ROLE_ADDRESS, "tcp://0.0.0.0:7100");
    says(
        &checked(&with(valid(), "storage_1", &wildcard), None),
        &[
            "instance storage_1, setting system.role_address: it holds tcp://0.0.0.0:7100, which no peer can dial",
        ],
    );
    let partition = "tcp://10.0.0.2:7000";
    let shared = STORAGE_1.replace(SYSTEM_ROLE_ADDRESS, partition);
    let everywhere = |text: &str| text.replace(SYSTEM_ROLE_ADDRESS, partition);
    let target = with(
        with(
            with(
                with(valid(), "storage_1", &shared),
                "frontend",
                &everywhere(FRONTEND),
            ),
            "storage",
            &everywhere(STORAGE),
        ),
        "compute_1",
        &everywhere(COMPUTE),
    );
    says(
        &checked(&target, None),
        &[&format!(
            "instance storage_1, setting system.role_address: {partition} is also storage_1's storage.role_address"
        )],
    );
    Ok(())
}

/// Two instances on one host hold their own data directories, compared as the Data check compares them, a trailing
/// separator ignored; instances on other hosts may repeat one.
#[test]
fn test_one_host_holds_a_data_directory_once() -> TestResult {
    let beside = STORAGE_2.replace("10.0.0.4:7000", "10.0.0.2:7010");
    says(
        &checked(&with(valid(), "storage_2", &beside), None),
        &[&format!(
            "instance storage_2, setting storage.data_dir: {STORAGE_DATA_DIR} is also storage_1's, on the same host"
        )],
    );
    let trailing = format!("{beside}data_dir = \"{STORAGE_DATA_DIR}/\"\n");
    says(
        &checked(&with(valid(), "storage_2", &trailing), None),
        &[
            "instance storage_2, setting storage.data_dir:",
            "is also storage_1's, on the same host",
        ],
    );
    let own = format!("{beside}data_dir = \"/data/storage_2\"\n");
    assert_eq!(checked(&with(valid(), "storage_2", &own), None), "");
    let compute = COMPUTE.replace("10.0.0.3", "10.0.0.2");
    assert_eq!(checked(&with(valid(), "compute_1", &compute), None), "");
    Ok(())
}

/// A storage instance's `close_request_ms` is at least twice the host's `epoch_ms_max`, which is 10 here.
#[test]
fn test_a_close_request_lasts_twice_the_longest_epoch() {
    let short = format!("{STORAGE_1}close_request_ms = 19\n");
    says(
        &checked(&with(valid(), "storage_1", &short), None),
        &[
            "instance storage_1, setting storage.close_request_ms: it holds 19; it must be at least twice sequencer.epoch_ms_max, 20",
        ],
    );
    let enough = format!("{STORAGE_1}close_request_ms = 20\n");
    assert_eq!(checked(&with(valid(), "storage_1", &enough), None), "");
}

/// Against the active revision, the partition count, an instance's partitions and its data directory stay as they are.
#[test]
fn test_the_placement_and_data_stay_fixed() {
    let active = valid();
    let more = STORAGE_1.replace("partition_count = 2", "partition_count = 3");
    says(
        &checked(&with(valid(), "storage_1", &more), Some(&active)),
        &[
            "instance storage_1, setting cluster.partition_count: it differs from the active revision",
        ],
    );
    let swapped = with(
        with(
            valid(),
            "storage_1",
            &STORAGE_1.replace("first = 1", "first = 2"),
        ),
        "storage_2",
        &STORAGE_2.replace("first = 2", "first = 1"),
    );
    says(
        &checked(&swapped, Some(&active)),
        &[
            "instance storage_1, setting storage.partition_first: the instance's partitions differ from the active revision",
        ],
    );
    let stored = format!("{STORAGE_1}data_dir = \"/data2\"\n");
    says(
        &checked(&with(valid(), "storage_1", &stored), Some(&active)),
        &["instance storage_1, setting storage.data_dir: it differs from the active revision"],
    );
}

/// Each of the host's metadata bounds is checked by `MetadataConfig`, which refuses 0 for every one.
#[test]
fn test_each_metadata_bound_is_checked() {
    for setting in ["block_size_max", "snapshot_rounds", "catalog_check_ms"] {
        let zero = STORAGE_1.replace("[metadata]\n", &format!("[metadata]\n{setting} = 0\n"));
        says(
            &checked(&with(valid(), "storage_1", &zero), None),
            &[&format!(
                "instance storage_1, setting metadata.{setting}: it holds 0;"
            )],
        );
    }
}

/// Each of a frontend's settings of failed authentications, its counts before a throttle and a ban and the ban's time, is refused
/// at 0 (design D15).
#[test]
fn test_each_failed_login_setting_is_at_least_1() {
    for setting in [
        "auth_failures_before_throttle",
        "auth_throttled_failures_before_ban",
        "auth_ban_ms",
    ] {
        let zero = format!("{FRONTEND}[endpoint]\n{setting} = 0\n");
        says(
            &checked(&with(valid(), "frontend", &zero), None),
            &[&format!(
                "instance frontend, setting endpoint.{setting}: it holds 0; it must be at least 1"
            )],
        );
    }
}

/// The host's partition count lies from 1 to the placement's most, and a storage instance's partitions within it.
#[test]
fn test_a_partition_count_bounds_the_partitions_held() {
    let none = STORAGE_1.replace("partition_count = 2", "partition_count = 0");
    says(
        &checked(&with(valid(), "storage_1", &none), None),
        &[
            &format!(
                "instance storage_1, setting cluster.partition_count: it holds 0; it must lie from 1 to {}",
                PlacementMap::PARTITIONS_MAX
            ),
            "instance storage_1, setting storage.partition_count: partition 1 lies outside the cluster's 0;",
        ],
    );
}

/// An observability address is an IP address and a port, which a listener binds without a lookup.
#[test]
fn test_an_observability_address_names_an_ip() {
    let named = FRONTEND.replace("tcp://127.0.0.1:18081", "tcp://frontend:18081");
    says(
        &checked(&with(valid(), "frontend", &named), None),
        &[
            "instance frontend, setting observability.address: it holds tcp://frontend:18081; it must be tcp://<ip>:<port>",
        ],
    );
}

/// A storage instance naming no role address is refused for that alone, with no listener derived from it.
#[test]
fn test_a_storage_instance_without_a_role_address_derives_no_listener() {
    let silent: String = STORAGE_2
        .lines()
        .filter(|line| !line.starts_with("role_address"))
        .map(|line| format!("{line}\n"))
        .collect();
    let listing = checked(&with(valid(), "storage_2", &silent), None);
    says(
        &listing,
        &["instance storage_2, setting storage.role_address: it is absent;"],
    );
    assert_eq!(listing.lines().count(), 1, "{listing}");
}

/// Against the active revision, an instance whose file is refused and an instance the revision lacks are compared with
/// nothing.
#[test]
fn test_only_instances_the_active_revision_holds_are_compared() {
    let without_compute = target_of(&[
        ("frontend", FRONTEND),
        ("storage", STORAGE),
        ("storage_1", STORAGE_1),
        ("storage_2", STORAGE_2),
    ]);
    assert_eq!(checked(&valid(), Some(&without_compute)), "");
    let refused = format!("{STORAGE_2}bogus = 1\n");
    let listing = checked(&with(valid(), "storage_2", &refused), Some(&valid()));
    says(&listing, &["storage.bogus"]);
    says_not(&listing, &["active revision"]);
}

/// A storage instance's partition count of billions is walked no further than one past the most a cluster holds: it is
/// refused as placing a partition outside the cluster's, at once, where walking it whole would hold 2^32 partitions.
#[test]
fn test_a_partition_count_of_billions_is_walked_no_further_than_the_cluster_holds() -> TestResult {
    let billions = format!("{STORAGE_1}partition_count = {}\n", u32::MAX);
    let listing = checked(&with(valid(), "storage_1", &billions), None);
    let past = PlacementMap::PARTITIONS_MAX + 1;
    says(
        &listing,
        &[&format!(
            "instance storage_1, setting storage.partition_count: partition {past} lies outside the cluster's 2;"
        )],
    );
    says_not(&listing, &[&format!("partition {} ", past + 1)]);
    Ok(())
}

/// `STORAGE_1` without the hosting: no `[system]` table, no moved table, no cluster partition count.
fn not_hosting() -> String {
    let mut kept = String::new();
    let mut table = "";
    for line in STORAGE_1.lines() {
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            table = name;
        }
        if table == "storage" {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    kept
}

/// `STORAGE_2` as a second host: `STORAGE_1`'s tables of hosting, at `STORAGE_2`'s address.
fn hosting_too() -> String {
    let hosting = STORAGE_1.replace(&not_hosting(), "");
    format!(
        "{}{STORAGE_2}",
        hosting.replace(SYSTEM_ROLE_ADDRESS, "tcp://10.0.0.4:7100")
    )
}

/// C7.1: no storage instance holds `[system]`: refused naming `system` and the target.
#[test]
fn test_a_target_with_no_hosting_instance_is_refused() -> TestResult {
    let listing = checked(&with(valid(), "storage_1", &not_hosting()), None);
    says(
        &listing,
        &["target ci, setting system: no storage instance holds a [system] table"],
    );
    Ok(())
}

/// C7.2: two storage instances hold `[system]`: refused naming `system` and both.
#[test]
fn test_two_hosting_instances_are_refused() -> TestResult {
    let listing = checked(&with(valid(), "storage_2", &hosting_too()), None);
    says(
        &listing,
        &["instance storage_2, setting system: storage_1 and storage_2 both hold a [system] table"],
    );
    Ok(())
}

/// C7.3: the one storage instance of a target hosts, and one of eight does: both accepted.
#[test]
fn test_one_host_among_one_or_eight_storage_instances_is_accepted() -> TestResult {
    let alone = STORAGE_1.replace("partition_count = 2", "partition_count = 1");
    let one = target_of(&[
        ("frontend", FRONTEND),
        ("storage", STORAGE),
        ("storage_1", &alone),
        ("compute_1", COMPUTE),
    ]);
    assert_eq!(checked(&one, None), "");
    let eight_host = STORAGE_1.replace("partition_count = 2", "partition_count = 8");
    let others: Vec<(String, String)> = (2..=8)
        .map(|index| {
            let text = STORAGE_2
                .replace("first = 2", &format!("first = {index}"))
                .replace("10.0.0.4", &format!("10.0.1.{index}"));
            (format!("storage_{index}"), text)
        })
        .collect();
    let mut files: Vec<(&str, &str)> = vec![
        ("frontend", FRONTEND),
        ("storage", STORAGE),
        ("storage_1", &eight_host),
        ("compute_1", COMPUTE),
    ];
    files.extend(
        others
            .iter()
            .map(|(key, text)| (key.as_str(), text.as_str())),
    );
    assert_eq!(checked(&target_of(&files), None), "");
    Ok(())
}

/// C7.4 to C7.6: a host lacking `system.role_address`, `system.data_dir` or both is refused naming each, and the System check
/// does not count it out as a host.
#[test]
fn test_a_host_lacking_a_system_setting_is_refused_naming_it() -> TestResult {
    let role_address = format!("role_address = \"{SYSTEM_ROLE_ADDRESS}\"\n");
    let data_dir = format!("data_dir = \"{SYSTEM_DATA_DIR}\"\n");
    let cases = [
        (vec![role_address.as_str()], vec!["system.role_address"]),
        (vec![data_dir.as_str()], vec!["system.data_dir"]),
        (
            vec![role_address.as_str(), data_dir.as_str()],
            vec!["system.role_address", "system.data_dir"],
        ),
    ];
    for (removed, named) in cases {
        let text = removed.iter().fold(STORAGE_1.to_string(), |text, line| {
            text.replacen(line, "", 1)
        });
        let listing = checked(&with(valid(), "storage_1", &text), None);
        for setting in named {
            says(
                &listing,
                &[&format!(
                    "instance storage_1, setting {setting}: it is absent"
                )],
            );
        }
        says_not(&listing, &["no storage instance holds a [system] table"]);
    }
    Ok(())
}

/// C7.7: a host lacking `[metadata]`, `[sequencer]` or both is refused naming each absent table.
#[test]
fn test_a_host_lacking_a_moved_table_is_refused_naming_it() -> TestResult {
    let metadata = "[metadata]\n";
    let sequencer = "[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n";
    let cases = [
        (vec![metadata], vec!["metadata"]),
        (vec![sequencer], vec!["sequencer"]),
        (vec![metadata, sequencer], vec!["metadata", "sequencer"]),
    ];
    for (removed, named) in cases {
        let text = removed.iter().fold(STORAGE_1.to_string(), |text, table| {
            text.replacen(table, "", 1)
        });
        let listing = checked(&with(valid(), "storage_1", &text), None);
        for table in named {
            says(
                &listing,
                &[&format!(
                    "instance storage_1, setting {table}: the instance hosting the system group holds no [{table}] table"
                )],
            );
        }
    }
    Ok(())
}

/// C7.8: a host holding no partition is refused.
#[test]
fn test_a_host_holding_no_partition_is_refused() -> TestResult {
    let none = format!("{STORAGE_1}partition_count = 0\n");
    says(
        &checked(&with(valid(), "storage_1", &none), None),
        &["instance storage_1, setting system: it hosts the system group and holds no partition"],
    );
    Ok(())
}

/// C7.9: a storage instance not hosting holds `[metadata]`, the frontend `[sequencer]`, a compute instance `[metadata]`: each
/// refused naming the table and the instance.
#[test]
fn test_a_moved_table_outside_the_host_is_refused() -> TestResult {
    let cases = [
        ("storage_2", format!("[metadata]\n{STORAGE_2}"), "metadata"),
        ("frontend", format!("{FRONTEND}[sequencer]\n"), "sequencer"),
        ("compute_1", format!("{COMPUTE}[metadata]\n"), "metadata"),
    ];
    for (instance, text, table) in cases {
        says(
            &checked(&with(valid(), instance, &text), None),
            &[&format!(
                "instance {instance}, setting {table}: it holds a [{table}] table and does not host the system group"
            )],
        );
    }
    Ok(())
}

/// C7.10: a host lacking `system.data_dir` beside a storage instance holding `[metadata]`: both listed.
#[test]
fn test_every_system_defect_is_listed() -> TestResult {
    let lacking = STORAGE_1.replacen(&format!("data_dir = \"{SYSTEM_DATA_DIR}\"\n"), "", 1);
    let target = with(
        with(valid(), "storage_1", &lacking),
        "storage_2",
        &format!("[metadata]\n{STORAGE_2}"),
    );
    says(
        &checked(&target, None),
        &[
            "instance storage_1, setting system.data_dir: it is absent",
            "instance storage_2, setting metadata:",
        ],
    );
    Ok(())
}

/// The listing of the valid target with `instance`'s `cluster.system_address` holding `address`.
fn reaching(instance: &str, address: &str) -> String {
    let text = match instance {
        "frontend" => FRONTEND,
        "compute_1" => COMPUTE,
        _ => STORAGE,
    };
    checked(
        &with(
            valid(),
            instance,
            &text.replace(SYSTEM_ROLE_ADDRESS, address),
        ),
        None,
    )
}

/// C7.11 to C7.16, over TCP: an address equal to the host's, or differing in its host alone, is accepted; one differing in
/// its scheme, its port, or its host and port is refused naming both addresses and both instances.
#[test]
fn test_a_system_address_reaches_the_host_by_its_scheme_and_port() -> TestResult {
    assert_eq!(reaching("compute_1", SYSTEM_ROLE_ADDRESS), "");
    assert_eq!(reaching("compute_1", "tcp://system.svc:7100"), "");
    for address in [
        "unix:///run/ultragraph/system.sock",
        "tcp://10.0.0.2:7101",
        "tcp://system.svc:7101",
    ] {
        says(
            &reaching("compute_1", address),
            &[&format!(
                "instance compute_1, setting cluster.system_address: it holds {address}, and storage_1's system.role_address is {SYSTEM_ROLE_ADDRESS}"
            )],
        );
    }
    Ok(())
}

/// C7.12 and C7.14, over a Unix socket: the same path is accepted; another path, or a TCP address, is refused.
#[test]
fn test_a_system_address_reaches_a_unix_host_by_its_path() -> TestResult {
    let socket = "unix:///run/ultragraph/system.sock";
    let unix_host = |instance_text: &str| instance_text.replace(SYSTEM_ROLE_ADDRESS, socket);
    let target = with(
        with(
            with(
                with(valid(), "storage_1", &unix_host(STORAGE_1)),
                "frontend",
                &unix_host(FRONTEND),
            ),
            "storage",
            &unix_host(STORAGE),
        ),
        "compute_1",
        &unix_host(COMPUTE),
    );
    assert_eq!(checked(&target, None), "");
    for address in ["unix:///run/ultragraph/other.sock", SYSTEM_ROLE_ADDRESS] {
        let varied = with(
            target.clone(),
            "compute_1",
            &COMPUTE.replace(SYSTEM_ROLE_ADDRESS, address),
        );
        says(
            &checked(&varied, None),
            &[&format!(
                "instance compute_1, setting cluster.system_address: it holds {address}, and storage_1's system.role_address is {socket}"
            )],
        );
    }
    Ok(())
}

/// C7.17: the host's own `cluster.system_address` differing in its port is refused.
#[test]
fn test_the_hosts_own_system_address_is_checked() -> TestResult {
    let own = STORAGE_1.replacen(
        "[cluster]\n",
        "[cluster]\nsystem_address = \"tcp://10.0.0.2:7101\"\n",
        1,
    );
    says(
        &checked(&with(valid(), "storage_1", &own), None),
        &["instance storage_1, setting cluster.system_address: it holds tcp://10.0.0.2:7101"],
    );
    Ok(())
}

/// C7.18: an instance naming no `cluster.system_address` is refused by the Required check.
#[test]
fn test_a_missing_system_address_is_required() -> TestResult {
    let without = COMPUTE.replace(&format!("system_address = \"{SYSTEM_ROLE_ADDRESS}\"\n"), "");
    says(
        &checked(&with(valid(), "compute_1", &without), None),
        &[
            "instance compute_1, setting cluster.system_address: it is absent; a compute requires it",
        ],
    );
    Ok(())
}

/// The listing of the valid target with the host's `system.data_dir` holding `system` and its `storage.data_dir` holding
/// `storage`.
fn directories(system: &str, storage: &str) -> String {
    let text = format!(
        "{}data_dir = \"{storage}\"\n",
        STORAGE_1.replace(SYSTEM_DATA_DIR, system)
    );
    checked(&with(valid(), "storage_1", &text), None)
}

/// C7.19 to C7.21: the group's directory equal to, inside or containing the host's own `data_dir` is refused naming both
/// settings and the instance.
#[test]
fn test_the_groups_directory_sharing_a_path_on_its_host_is_refused() -> TestResult {
    let cases = [
        (STORAGE_DATA_DIR, STORAGE_DATA_DIR, "is"),
        ("/data/storage/system", STORAGE_DATA_DIR, "lies inside"),
        (SYSTEM_DATA_DIR, "/data/system/storage", "contains"),
    ];
    for (system, storage, relation) in cases {
        says(
            &directories(system, storage),
            &[&format!(
                "instance storage_1, setting system.data_dir: {system} {relation} storage_1's storage.data_dir, {storage}"
            )],
        );
    }
    Ok(())
}

/// C7.22 to C7.24: siblings, a text that is a prefix of another's without a component's end, and a path holding `.` are
/// accepted; a trailing separator is ignored, and `..` is a component as written.
#[test]
fn test_directories_are_compared_component_by_component() -> TestResult {
    assert_eq!(directories(SYSTEM_DATA_DIR, STORAGE_DATA_DIR), "");
    assert_eq!(directories("/data/sys", SYSTEM_DATA_DIR), "");
    assert_eq!(directories("/data/./storage", STORAGE_DATA_DIR), "");
    says(
        &directories("/data/storage/", STORAGE_DATA_DIR),
        &[
            "instance storage_1, setting system.data_dir: /data/storage/ is storage_1's storage.data_dir",
        ],
    );
    says(
        &directories("/data/storage/..", STORAGE_DATA_DIR),
        &[
            "instance storage_1, setting system.data_dir: /data/storage/.. lies inside storage_1's storage.data_dir",
        ],
    );
    Ok(())
}

/// C7.25: the group's directory inside the `data_dir` of an instance on another host is accepted; on the host's own, refused.
#[test]
fn test_only_a_data_dir_on_the_groups_host_is_compared() -> TestResult {
    let inside = format!(
        "{}data_dir = \"/data/storage_1\"\n",
        STORAGE_1.replace(SYSTEM_DATA_DIR, "/data/storage/system")
    );
    let target = with(valid(), "storage_1", &inside);
    assert_eq!(checked(&target, None), "");
    let beside = STORAGE_2.replace("10.0.0.4:7000", "10.0.0.2:7010");
    says(
        &checked(&with(target, "storage_2", &beside), None),
        &[
            "instance storage_1, setting system.data_dir: /data/storage/system lies inside storage_2's storage.data_dir",
        ],
    );
    Ok(())
}

/// C7.27: against the active revision, the host moved to another instance and its `system.data_dir` moved are refused; its
/// `system.role_address` alone moved, or no active revision, is accepted.
#[test]
fn test_the_host_and_its_directory_stay_fixed() -> TestResult {
    let active = valid();
    let moved = with(
        with(valid(), "storage_1", &not_hosting()),
        "storage_2",
        &hosting_too(),
    );
    says(
        &checked(&moved, Some(&active)),
        &[
            "instance storage_2, setting system: it holds [system], and the active revision's storage_1 holds it",
        ],
    );
    let elsewhere = STORAGE_1.replace(SYSTEM_DATA_DIR, "/data/system2");
    says(
        &checked(&with(valid(), "storage_1", &elsewhere), Some(&active)),
        &["instance storage_1, setting system.data_dir: it differs from the active revision"],
    );
    let named = STORAGE_1.replace(SYSTEM_ROLE_ADDRESS, "tcp://storage-1.ci:7100");
    assert_eq!(
        checked(&with(valid(), "storage_1", &named), Some(&active)),
        ""
    );
    says_not(&checked(&moved, None), &["active revision"]);
    Ok(())
}

/// C7.28: `cluster.frontend_address` in any file, `frontend.role_address` and `frontend.data_dir` are refused naming the
/// setting as unknown.
#[test]
fn test_the_old_settings_are_refused_by_name() -> TestResult {
    let cases = [
        (
            "compute_1",
            COMPUTE.replace(
                "[cluster]\n",
                "[cluster]\nfrontend_address = \"tcp://frontend:7081\"\n",
            ),
            "cluster.frontend_address",
        ),
        (
            "frontend",
            format!("{FRONTEND}[frontend]\nrole_address = \"tcp://10.0.0.1:7081\"\n"),
            "frontend.role_address",
        ),
        (
            "frontend",
            format!("{FRONTEND}[frontend]\ndata_dir = \"/data\"\n"),
            "frontend.data_dir",
        ),
    ];
    for (instance, text, setting) in cases {
        says(
            &checked(&with(valid(), instance, &text), None),
            &[&format!(
                "instance {instance}, setting {setting}: no such setting exists"
            )],
        );
    }
    Ok(())
}

/// A table in a role's file is held by every instance of the role: `[metadata]` there serves the host, which then lacks no
/// moved table, and is refused in the instance not hosting.
#[test]
fn test_a_table_in_the_roles_file_is_held_by_each_instance() -> TestResult {
    let host = STORAGE_1.replacen("[metadata]\n", "", 1);
    let target = with(
        with(valid(), "storage_1", &host),
        "storage",
        &format!("{STORAGE}[metadata]\n"),
    );
    let listing = checked(&target, None);
    says(
        &listing,
        &[
            "instance storage_2, setting metadata: it holds a [metadata] table and does not host the system group",
        ],
    );
    says_not(&listing, &["instance storage_1, setting metadata:"]);
    Ok(())
}

/// The sequencer's bounds are the host's alone: a storage instance not hosting that holds `[sequencer]` is refused by the
/// System check for the table, and its bounds are not checked.
#[test]
fn test_only_the_hosts_sequencer_bounds_are_checked() -> TestResult {
    let reversed = format!("[sequencer]\nepoch_ms_min = 11\nepoch_ms_max = 10\n{STORAGE_2}");
    let listing = checked(&with(valid(), "storage_2", &reversed), None);
    says(
        &listing,
        &[
            "instance storage_2, setting sequencer: it holds a [sequencer] table and does not host the system group",
        ],
    );
    says_not(&listing, &["setting sequencer.epoch_ms_max"]);
    Ok(())
}

/// A frontend dials the system group alone: a `compute.role_address` in its file is no address of its own, so it is not
/// parsed as one.
#[test]
fn test_a_frontends_role_addresses_are_its_system_address_alone() -> TestResult {
    let foreign = format!("{FRONTEND}[compute]\nrole_address = \"ftp://10.0.0.1:7082\"\n");
    says_not(
        &checked(&with(valid(), "frontend", &foreign), None),
        &["instance frontend, setting compute.role_address"],
    );
    Ok(())
}

/// A frontend's client endpoint is a listener: it cannot share an address with another instance's.
#[test]
fn test_the_frontends_endpoint_takes_part_in_the_address_checks() -> TestResult {
    let compute = "tcp://10.0.0.3:7082";
    let endpoint = format!("{FRONTEND}[endpoint]\naddress = \"{compute}\"\nplaintext = true\n");
    says(
        &checked(&with(valid(), "frontend", &endpoint), None),
        &[&format!(
            "instance frontend, setting endpoint.address: {compute} is also compute_1's compute.role_address"
        )],
    );
    Ok(())
}
