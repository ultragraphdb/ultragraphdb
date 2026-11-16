/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The `[system]` table and `cluster.system_address` (design D2): laws L2.1, L2.2 and L2.6.

use hegel::TestCase;
use ultragraphdb_common_service_config::{
    ServiceConfig, ServiceConfigError, ServiceConfigErrorEnum,
};
use ultragraphdb_tool_toml::TomlErrorEnum;

use crate::utils::drawn::{KEY_CHARACTERS, address, path, within, word};

/// The two settings of a `[system]` table, as the specification names them.
const SETTINGS: [&str; 2] = ["role_address", "data_dir"];

/// A storage instance's file holding `cluster.system_address = system_address` and a `[system]` table of `lines`.
fn hosting(system_address: &str, lines: &[(&str, &str)]) -> String {
    let system: String = lines
        .iter()
        .map(|(key, value)| format!("{key} = \"{value}\"\n"))
        .collect();
    format!(
        "[cluster]\nsystem_address = \"{system_address}\"\n[storage]\nrole_address = \"tcp://storage-0:7200\"\n[system]\n{system}"
    )
}

/// The refusal of a start lacking `system.<key>`.
fn missing(key: &str) -> ServiceConfigError {
    ServiceConfigError(ServiceConfigErrorEnum::Missing {
        setting: format!("system.{key}"),
    })
}

/// L2.1: a file holding a `[system]` table with both settings and `cluster.system_address`, each drawn as a TCP or a
/// Unix address or a path, reads back as drawn.
#[hegel::test]
fn test_a_drawn_system_table_reads_back_as_drawn(tc: TestCase) {
    let role_address = address(&tc);
    let data_dir = address(&tc);
    let system_address = address(&tc);
    let text = hosting(
        &system_address,
        &[("role_address", &role_address), ("data_dir", &data_dir)],
    );
    let read = ServiceConfig::parse(text.as_bytes()).ok();
    let system = read.as_ref().and_then(ServiceConfig::system);
    assert_eq!(
        system.map(|table| table.role_address()),
        Some(role_address.as_str())
    );
    assert_eq!(
        system.map(|table| table.data_dir()),
        Some(data_dir.as_str())
    );
    assert_eq!(
        read.as_ref()
            .and_then(|config| config.cluster().system_address()),
        Some(system_address.as_str())
    );
}

/// L2.2: the file of L2.1 with `role_address`, `data_dir` or both removed from `[system]` is refused naming each absent
/// setting and no other: the listed problems are exactly those, and a start stops at one of them.
#[hegel::test]
fn test_a_system_table_lacking_a_setting_is_refused_naming_each_absent(tc: TestCase) {
    let removed = within(&tc, 1, 3);
    let kept: Vec<(&str, String)> = SETTINGS
        .iter()
        .enumerate()
        .filter(|(index, _)| removed & (1 << index) == 0)
        .map(|(_, key)| (*key, path(&tc)))
        .collect();
    let lines: Vec<(&str, &str)> = kept
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .collect();
    let text = hosting(&address(&tc), &lines);
    let absent: Vec<ServiceConfigError> = SETTINGS
        .iter()
        .enumerate()
        .filter(|(index, _)| removed & (1 << index) != 0)
        .map(|(_, key)| missing(key))
        .collect();
    let problems = ServiceConfig::problems(text.as_bytes());
    assert_eq!(problems.len(), absent.len(), "{problems:?}");
    assert!(
        absent.iter().all(|one| problems.contains(one)),
        "{problems:?}"
    );
    let refusal = ServiceConfig::parse(text.as_bytes()).err();
    assert!(
        refusal.as_ref().is_some_and(|error| absent.contains(error)),
        "{refusal:?}"
    );
}

/// L2.6: a `[system]` table holding one more key, its name drawn from `a`–`z`, `0`–`9` and `_` and neither of the two,
/// is refused as an unknown setting naming the key, and for nothing else.
#[hegel::test]
fn test_a_system_table_with_one_more_key_is_refused_naming_it(tc: TestCase) {
    let extra = word(&tc, KEY_CHARACTERS, 16);
    tc.assume(!SETTINGS.contains(&extra.as_str()));
    let value = path(&tc);
    let text = hosting(
        &address(&tc),
        &[
            ("role_address", "tcp://storage-0:7100"),
            ("data_dir", "/data/system"),
            (&extra, &value),
        ],
    );
    let names_it = |error: &ServiceConfigError| {
        matches!(&error.0, ServiceConfigErrorEnum::Toml(toml)
            if matches!(&toml.0, TomlErrorEnum::UnreadKey { table, key } if table == "system" && *key == extra))
    };
    let refusal = ServiceConfig::parse(text.as_bytes()).err();
    assert!(refusal.as_ref().is_some_and(names_it), "{refusal:?}");
    let problems = ServiceConfig::problems(text.as_bytes());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems.iter().all(names_it), "{problems:?}");
}
