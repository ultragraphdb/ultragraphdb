/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_config_check::{Defect, TargetFiles, check_config_service, check_target};
use ultragraphdb_tool_toml::{TomlDocument, TomlKind};

const SERVICE: &str = "[config]\naddress = \"tcp://127.0.0.1:7070\"\nplaintext = true\ndata_dir = \"/data\"\nread_key_file = \"/keys/read\"\nadmin_key_file = \"/keys/admin\"\ntargets_file = \"/etc/targets.toml\"\n";

fn checked(text: &str) -> String {
    check_config_service("config.toml", text.as_bytes())
        .iter()
        .map(Defect::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The config service's own file passes when it names every setting `[config]` requires.
#[test]
fn test_a_complete_config_service_file_passes() {
    assert_eq!(checked(SERVICE), "");
}

/// Each setting `[config]` requires, absent, is named; a refused file is refused for what it holds.
#[test]
fn test_a_config_service_file_names_what_it_lacks() {
    for setting in [
        "address",
        "data_dir",
        "read_key_file",
        "admin_key_file",
        "targets_file",
    ] {
        let lacking: String = SERVICE
            .lines()
            .filter(|line| !line.starts_with(&format!("{setting} =")))
            .map(|line| format!("{line}\n"))
            .collect();
        let listing = checked(&lacking);
        let expected = format!("config.toml: setting config.{setting}: it is absent;");
        assert!(listing.contains(&expected), "{expected} in:\n{listing}");
    }
    let unknown = checked(&format!("{SERVICE}bogus = 1\n"));
    assert!(unknown.contains("config.bogus"), "{unknown}");
}

/// The config service's file names how its listener is secured, and an address a listener can take.
#[test]
fn test_a_config_service_file_names_its_security_and_a_listening_address() {
    let open: String = SERVICE
        .lines()
        .filter(|line| !line.starts_with("plaintext ="))
        .map(|line| format!("{line}\n"))
        .collect();
    let unsecured = checked(&open);
    assert!(
        unsecured.contains("config.toml: setting config.plaintext: "),
        "{unsecured}"
    );
    let address = "ftp://127.0.0.1:7070";
    let elsewhere = checked(&SERVICE.replace("tcp://127.0.0.1:7070", address));
    assert!(
        elsewhere.contains(&format!(
            "config.toml:2: setting config.address: it holds {address}; "
        )),
        "{elsewhere}"
    );
}

/// Every defect of a target holding one storage file of `text`, one per line.
fn checked_storage(name: &str, key: &str, text: &str) -> String {
    let mut target = TargetFiles::new(name);
    target.insert(key, "storage.toml", text.as_bytes().to_vec());
    check_target(&target, None)
        .iter()
        .map(Defect::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A target's name stands in a path, and each key names a role: either refused is named before what the files hold.
#[test]
fn test_a_target_name_and_its_keys_are_checked() {
    for (name, key, expected) in [
        (
            "../ci",
            "storage",
            "the target's name ../ci cannot stand in a path;",
        ),
        (
            "ci",
            "bogus",
            "storage.toml: target ci, bogus names no role;",
        ),
    ] {
        let listing = checked_storage(name, key, "");
        assert!(listing.contains(expected), "{listing}");
    }
}

/// Each refusal of a file names its setting and line where it has one: a value its field cannot hold, an unknown table,
/// a key outside every table, a value of another kind, and a document past the parser's bounds.
#[test]
fn test_each_refusal_of_a_file_names_what_it_can() {
    let below = -1;
    let cases = [
        (
            format!("[storage]\npartition_count = {below}\n"),
            format!(
                "storage.toml:2: target ci, instance storage, setting storage.partition_count: it holds {below}; it must lie from 0 to {}",
                u32::MAX
            ),
        ),
        (
            "[bogus]\n".to_string(),
            "instance storage, the table [bogus] does not exist;".to_string(),
        ),
        (
            "bogus = 1\n".to_string(),
            "storage.toml:1: target ci, instance storage, setting bogus: no such setting exists;"
                .to_string(),
        ),
        (
            "[storage]\npartition_count = \"one\"\n".to_string(),
            format!(
                "storage.toml:2: target ci, instance storage, setting storage.partition_count: it holds a {:?} value; it takes a {:?} value",
                TomlKind::String,
                TomlKind::Integer
            ),
        ),
    ];
    for (text, expected) in cases {
        let listing = checked_storage("ci", "storage", &text);
        assert!(listing.contains(&expected), "{expected} in:\n{listing}");
    }
    let past_bound = checked_storage(
        "ci",
        "storage",
        &format!("# {}\n", "x".repeat(TomlDocument::LINE_BYTES_MAX)),
    );
    assert!(
        past_bound.starts_with("storage.toml: target ci, instance storage, "),
        "{past_bound}"
    );
    assert!(
        past_bound.contains("the settings are standard TOML"),
        "{past_bound}"
    );
}
