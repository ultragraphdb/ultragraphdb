/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What only the merged settings, or the set of them, can show: the checks of the config-service spec's table, each
//! reached through `check_target`.

use ultragraphdb_common_config_check::{Defect, TargetFiles, check_target};

const FRONTEND: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\npartition_count = 2\n[observability]\naddress = \"tcp://127.0.0.1:18081\"\n[frontend]\nrole_address = \"tcp://10.0.0.1:7081\"\ndata_dir = \"/data\"\n[sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n";
const STORAGE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[storage]\npartition_count = 1\ndata_dir = \"/data\"\n";
const STORAGE_1: &str = "[storage]\npartition_first = 1\nrole_address = \"tcp://10.0.0.2:7000\"\n";
const STORAGE_2: &str = "[storage]\npartition_first = 2\nrole_address = \"tcp://10.0.0.4:7000\"\n";
const COMPUTE: &str = "[cluster]\nid = 7\nrole_secret_file = \"/run/secret\"\nfrontend_address = \"tcp://frontend:7081\"\n[observability]\naddress = \"tcp://0.0.0.0:8080\"\n[compute]\nrole_address = \"tcp://10.0.0.3:7082\"\n";

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

/// A frontend's sequencer and metadata settings are bounded by their domain types; a storage instance's are not its own.
#[test]
fn test_a_frontend_bound_is_checked_by_its_domain_type() {
    let reversed = FRONTEND.replace("epoch_ms_min = 2", "epoch_ms_min = 11");
    says(
        &checked(&with(valid(), "frontend", &reversed), None),
        &["instance frontend, setting sequencer.epoch_ms_max: 11 to 10 ms is refused"],
    );
    let sessions = format!("{FRONTEND}[metadata]\nsessions_max = 0\n");
    says(
        &checked(&with(valid(), "frontend", &sessions), None),
        &["instance frontend, setting metadata.sessions_max: it holds 0;"],
    );
    let storage = format!("{STORAGE}[sequencer]\nepoch_ms_min = 11\nepoch_ms_max = 10\n");
    says_not(
        &checked(&with(valid(), "storage", &storage), None),
        &["sequencer.epoch_ms_max"],
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
            "frontend",
            FRONTEND.replace("tcp://10.0.0.1:7081", "ftp://10.0.0.1:7081"),
            "instance frontend, setting frontend.role_address: it holds ftp://10.0.0.1:7081;",
        ),
        (
            "storage_1",
            STORAGE_1.replace("tcp://", "ftp://"),
            "instance storage_1, setting storage.role_address: it holds ftp://10.0.0.2:7000;",
        ),
        (
            "compute_1",
            COMPUTE.replace("tcp://10.0.0.3", "ftp://10.0.0.3"),
            "instance compute_1, setting compute.role_address: it holds ftp://10.0.0.3:7082;",
        ),
        (
            "compute_1",
            COMPUTE.replace("tcp://frontend", "ftp://frontend"),
            "instance compute_1, setting cluster.frontend_address: it holds ftp://frontend:7081;",
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

/// An instance reaches the frontend's role address by its port; only its host may differ.
#[test]
fn test_a_frontend_address_reaches_the_frontend_by_its_port() {
    let elsewhere = STORAGE.replace("tcp://frontend:7081", "tcp://frontend:7999");
    says(
        &checked(&with(valid(), "storage", &elsewhere), None),
        &[
            "instance storage_1, setting cluster.frontend_address: it holds tcp://frontend:7999, and the frontend listens at tcp://10.0.0.1:7081",
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

/// A frontend's own listeners take part: its role address cannot be a wildcard, nor share an address with its endpoint.
#[test]
fn test_a_frontend_listener_takes_part_in_the_address_checks() {
    let wildcard = FRONTEND.replace("tcp://10.0.0.1:7081", "tcp://0.0.0.0:7081");
    says(
        &checked(&with(valid(), "frontend", &wildcard), None),
        &[
            "instance frontend, setting frontend.role_address: it holds tcp://0.0.0.0:7081, which no peer can dial",
        ],
    );
    let shared =
        format!("{FRONTEND}[endpoint]\naddress = \"tcp://10.0.0.1:7081\"\nplaintext = true\n");
    says(
        &checked(&with(valid(), "frontend", &shared), None),
        &[
            "instance frontend, setting endpoint.address: tcp://10.0.0.1:7081 is also frontend's frontend.role_address",
        ],
    );
}

/// Two instances on one host hold their own data directories; instances on other hosts, or holding none, may repeat one.
#[test]
fn test_one_host_holds_a_data_directory_once() {
    let beside = STORAGE_1.replace("10.0.0.2", "10.0.0.1");
    says(
        &checked(&with(valid(), "storage_1", &beside), None),
        &[
            "instance storage_1, setting storage.data_dir: /data is also frontend's, on the same host",
        ],
    );
    let own = format!("{beside}data_dir = \"/data/storage_1\"\n");
    assert_eq!(checked(&with(valid(), "storage_1", &own), None), "");
    let compute = COMPUTE.replace("10.0.0.3", "10.0.0.1");
    assert_eq!(checked(&with(valid(), "compute_1", &compute), None), "");
}

/// A storage instance's `close_request_ms` is at least twice the frontend's `epoch_ms_max`, which is 10 here.
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
    let more = FRONTEND.replace("partition_count = 2", "partition_count = 3");
    says(
        &checked(&with(valid(), "frontend", &more), Some(&active)),
        &[
            "instance frontend, setting cluster.partition_count: it differs from the active revision",
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
    let moved = FRONTEND.replace("\"/data\"", "\"/data2\"");
    says(
        &checked(&with(valid(), "frontend", &moved), Some(&active)),
        &["instance frontend, setting frontend.data_dir: it differs from the active revision"],
    );
    let stored = format!("{STORAGE_1}data_dir = \"/data2\"\n");
    says(
        &checked(&with(valid(), "storage_1", &stored), Some(&active)),
        &["instance storage_1, setting storage.data_dir: it differs from the active revision"],
    );
}
