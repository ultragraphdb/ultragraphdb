/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_config_check::{Defect, check_config_service};

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
