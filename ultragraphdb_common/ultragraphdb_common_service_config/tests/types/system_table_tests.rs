/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The `[system]` table (design D2; system-group, "One storage instance hosts the system group"): the corner cases C2.1
//! to C2.7 of `openspec/changes/stage-5-8-system-group/notes/corner-cases.md`. Every expected value is the text the
//! test writes, or a setting's name as the specification spells it.

use ultragraphdb_common_service_config::{
    ServiceConfig, ServiceConfigError, ServiceConfigErrorEnum,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_toml::TomlErrorEnum;

/// A storage instance's settings, without a `[system]` table.
const STORAGE: &str = "[storage]\npartition_first = 1\npartition_count = 2\nrole_address = \"tcp://storage-0:7200\"\ndata_dir = \"/data/storage\"\n";

/// The settings `STORAGE` with `system`, the lines of a `[system]` table, after them.
fn hosting(system: &str) -> String {
    format!("{STORAGE}[system]\n{system}")
}

/// The refusal `parse` gives `text`.
fn refused(text: &str) -> TestResult<ServiceConfigErrorEnum> {
    match ServiceConfig::parse(text.as_bytes()) {
        Ok(config) => Err(TestError::new(format!("accepted as {config:?}"))),
        Err(error) => Ok(error.0),
    }
}

/// The refusal of a start lacking `setting`, as the specification names it, `table.key`.
fn missing(setting: &str) -> ServiceConfigErrorEnum {
    ServiceConfigErrorEnum::Missing {
        setting: setting.to_string(),
    }
}

/// C2.1: a storage file with no `[system]` table reads, and the instance hosts no group.
#[test]
fn test_a_storage_file_without_a_system_table_hosts_no_group() -> TestResult {
    let config = ServiceConfig::parse(STORAGE.as_bytes())?;
    assert_eq!(config.system(), None);
    assert_eq!(
        config.storage().role_address(),
        Some("tcp://storage-0:7200")
    );
    Ok(())
}

/// C2.2: a `[system]` table with both settings reads as written.
#[test]
fn test_a_system_table_with_both_settings_reads_as_written() -> TestResult {
    let text = hosting("role_address = \"tcp://storage-0:7100\"\ndata_dir = \"/data/system\"\n");
    let config = ServiceConfig::parse(text.as_bytes())?;
    let system = config.system().context("the [system] table")?;
    assert_eq!(system.role_address(), "tcp://storage-0:7100");
    assert_eq!(system.data_dir(), "/data/system");
    assert_eq!(config.storage().data_dir(), Some("/data/storage"));
    assert!(ServiceConfig::problems(text.as_bytes()).is_empty());
    Ok(())
}

/// C2.3: a `[system]` table naming `role_address` and no `data_dir` is refused naming `system.data_dir`, and listed with
/// that refusal alone (system-group, "A system table without its directory stops the start", its settings half).
#[test]
fn test_a_system_table_without_its_directory_is_refused_naming_it() -> TestResult {
    let text = hosting("role_address = \"tcp://storage-0:7100\"\n");
    assert_eq!(refused(&text)?, missing("system.data_dir"));
    assert_eq!(
        ServiceConfig::problems(text.as_bytes()),
        vec![ServiceConfigError(missing("system.data_dir"))]
    );
    let message = ServiceConfig::parse(text.as_bytes())
        .err()
        .context("a refusal")?
        .to_string();
    assert!(message.contains("system.data_dir"), "{message}");
    Ok(())
}

/// C2.4: a `[system]` table naming `data_dir` and no `role_address` is refused naming `system.role_address`.
#[test]
fn test_a_system_table_without_its_address_is_refused_naming_it() -> TestResult {
    let text = hosting("data_dir = \"/data/system\"\n");
    assert_eq!(refused(&text)?, missing("system.role_address"));
    assert_eq!(
        ServiceConfig::problems(text.as_bytes()),
        vec![ServiceConfigError(missing("system.role_address"))]
    );
    Ok(())
}

/// C2.5: an empty `[system]` table is refused naming both settings: the list of problems holds each, and nothing else,
/// and a start stops at one of the two.
#[test]
fn test_an_empty_system_table_is_refused_naming_both() -> TestResult {
    let text = hosting("");
    let problems = ServiceConfig::problems(text.as_bytes());
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems.contains(&ServiceConfigError(missing("system.role_address"))));
    assert!(problems.contains(&ServiceConfigError(missing("system.data_dir"))));
    let first = refused(&text)?;
    assert!(
        first == missing("system.role_address") || first == missing("system.data_dir"),
        "{first:?}"
    );
    // The header alone, with nothing else in the file.
    assert_eq!(ServiceConfig::problems(b"[system]\n").len(), 2);
    Ok(())
}

/// C2.6: a `[system]` table holding a key the table does not take is refused as an unknown setting naming the key; a
/// misspelling in place of `role_address` is listed beside the missing setting.
#[test]
fn test_an_unknown_key_in_the_system_table_is_refused_naming_it() -> TestResult {
    let beside = hosting(
        "role_address = \"tcp://storage-0:7100\"\ndata_dir = \"/data/system\"\nrole_adress = \"tcp://storage-0:7101\"\n",
    );
    let ServiceConfigErrorEnum::Toml(toml) = refused(&beside)? else {
        return Err(TestError::new("refused other than as an unread key"));
    };
    assert!(
        matches!(&toml.0, TomlErrorEnum::UnreadKey { table, key } if table == "system" && key == "role_adress"),
        "{toml:?}"
    );
    let instead = hosting("role_adress = \"tcp://storage-0:7100\"\ndata_dir = \"/data/system\"\n");
    let text = ServiceConfig::problems(instead.as_bytes())
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert_eq!(text.len(), 2, "{text:?}");
    assert!(
        text.iter().any(|line| line.contains("role_adress")),
        "{text:?}"
    );
    assert!(
        text.iter().any(|line| line.contains("system.role_address")),
        "{text:?}"
    );
    Ok(())
}

/// C2.7: `system.role_address` reads over either carrier, and a text that is no address reads as written, as every
/// address setting does: this crate checks a value's kind, and the instance listening there, or the config service's
/// Addresses check, parses it.
#[test]
fn test_a_system_role_address_reads_on_either_carrier_as_written() -> TestResult {
    for address in [
        "tcp://storage-0:7100",
        "unix:///run/ultragraph/system.sock",
        "storage-0",
    ] {
        let text = hosting(&format!(
            "role_address = \"{address}\"\ndata_dir = \"/data/system\"\n"
        ));
        let config = ServiceConfig::parse(text.as_bytes())?;
        let system = config.system().context("the [system] table")?;
        assert_eq!(system.role_address(), address);
    }
    Ok(())
}

/// A setting of the wrong kind in `[system]` is refused naming it, and is no missing setting: though the setting gave
/// no value, the list of problems holds no `Missing` beside its refusal (phase 3: a value refused while listing is
/// absent too). The list also holds it as unread, as it does any value of the wrong kind in any table.
#[test]
fn test_a_system_setting_of_the_wrong_kind_is_refused() -> TestResult {
    let text = hosting("role_address = 7100\ndata_dir = \"/data/system\"\n");
    let wrong_kind = |error: &ServiceConfigErrorEnum| {
        matches!(error, ServiceConfigErrorEnum::Toml(toml)
            if matches!(&toml.0, TomlErrorEnum::WrongKind { table, key, .. } if table == "system" && key == "role_address"))
    };
    let error = refused(&text)?;
    assert!(wrong_kind(&error), "{error:?}");
    let problems = ServiceConfig::problems(text.as_bytes());
    assert!(
        problems.iter().any(|problem| wrong_kind(&problem.0)),
        "{problems:?}"
    );
    assert!(
        !problems
            .iter()
            .any(|problem| matches!(problem.0, ServiceConfigErrorEnum::Missing { .. })),
        "{problems:?}"
    );
    Ok(())
}
