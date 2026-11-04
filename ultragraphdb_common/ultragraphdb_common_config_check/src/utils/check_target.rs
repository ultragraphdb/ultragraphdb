/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service_config::{
    ServiceConfig, ServiceConfigError, ServiceConfigErrorEnum,
};
use ultragraphdb_tool_toml::{TomlDocument, TomlError, TomlErrorEnum};

use crate::types::defect::Defect;
use crate::types::target_files::TargetFiles;
use crate::utils::cross_checks::cross_checks;
use crate::utils::merge::role_of;

/// The longest name a target takes.
const TARGET_NAME_MAX: usize = 64;

/// Whether `name` can name a target, which stands in a path and a folder's name.
pub(crate) fn well_named(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= TARGET_NAME_MAX
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// Every defect of `target`, whole: each file by itself, each instance's merged settings, and what only the set of them can
/// show. With `active`, the target's active revision, what must not change against it too.
#[must_use]
pub fn check_target(target: &TargetFiles, active: Option<&TargetFiles>) -> Vec<Defect> {
    let mut defects = check_files(target);
    defects.extend(cross_checks(target, active));
    defects
}

/// Every defect each of `target`'s files shows by itself.
pub(crate) fn check_files(target: &TargetFiles) -> Vec<Defect> {
    let name = target.name();
    let named = (!well_named(name)).then(|| {
        Defect::new(
            format!("the target's name {name} cannot stand in a path"),
            format!("a target is named by 1 to {TARGET_NAME_MAX} of A-Z, a-z, 0-9, _ and -"),
        )
    });
    named.into_iter().chain(file_checks(target)).collect()
}

/// Each file's defects.
fn file_checks(target: &TargetFiles) -> Vec<Defect> {
    target
        .files()
        .flat_map(|(key, label, bytes)| {
            if role_of(key).is_some() {
                file_defects(target.name(), key, label, bytes)
            } else {
                vec![
                    Defect::new(
                        format!("{key} names no role"),
                        "a key is frontend, compute or storage, alone or followed by _ and a label of a-z, 0-9 and _",
                    )
                    .in_file(label)
                    .of_target(target.name()),
                ]
            }
        })
        .collect()
}

/// Every refusal of one file by itself, its line found when the parser knows it.
fn file_defects(target: &str, key: &str, label: &str, bytes: &[u8]) -> Vec<Defect> {
    settings_defects(label, bytes)
        .into_iter()
        .map(|defect| defect.of_target(target).of_instance(key))
        .collect()
}

/// Every refusal of the settings `bytes` of the file `label`.
fn settings_defects(label: &str, bytes: &[u8]) -> Vec<Defect> {
    let document = TomlDocument::parse(bytes).ok();
    ServiceConfig::problems(bytes)
        .iter()
        .map(|problem| {
            let (setting, found, required, line) = describe(problem, document.as_ref());
            let defect = Defect::new(found, required).in_file(label).on_line(line);
            match setting {
                Some(setting) => defect.of_setting(setting),
                None => defect,
            }
        })
        .collect()
}

/// Every defect of the config service's own settings file, `label`: what any settings file can show, and each setting
/// of `[config]` it requires.
#[must_use]
pub fn check_config_service(label: &str, bytes: &[u8]) -> Vec<Defect> {
    let mut defects = settings_defects(label, bytes);
    if !defects.is_empty() {
        return defects;
    }
    let Ok(config) = ServiceConfig::parse(bytes) else {
        return defects;
    };
    let table = config.config();
    let required = [
        ("config.address", table.address()),
        ("config.data_dir", table.data_dir()),
        ("config.read_key_file", table.read_key_file()),
        ("config.admin_key_file", table.admin_key_file()),
        ("config.targets_file", table.targets_file()),
    ];
    let document = TomlDocument::parse(bytes).ok();
    for (setting, value) in required {
        if value.is_none() {
            defects.push(
                Defect::new("it is absent", "the config service requires it")
                    .in_file(label)
                    .of_setting(setting),
            );
        }
    }
    if let Err(error) = table.security() {
        defects.push(
            Defect::new(
                error.to_string(),
                "name tls_cert_file and tls_key_file, or set plaintext = true",
            )
            .in_file(label)
            .of_setting("config.plaintext"),
        );
    }
    if let Some(address) = table.address()
        && let Err(error) = ultragraphdb_protocol_transport::Address::parse(address)
    {
        defects.push(
            Defect::new(format!("it holds {address}"), error.to_string())
                .in_file(label)
                .on_line(
                    document
                        .as_ref()
                        .and_then(|held| held.line_of("config", "address")),
                )
                .of_setting("config.address"),
        );
    }
    defects
}

/// A refusal's setting, what was found, what is required, and its line.
fn describe(
    problem: &ServiceConfigError,
    document: Option<&TomlDocument>,
) -> (Option<String>, String, String, Option<usize>) {
    let line_of = |table: &str, key: &str| document.and_then(|held| held.line_of(table, key));
    if let ServiceConfigErrorEnum::Toml(error) = &problem.0 {
        return describe_toml(error, document);
    }
    if let ServiceConfigErrorEnum::OutOfRange {
        setting,
        value,
        max,
    } = &problem.0
    {
        let line = setting
            .split_once('.')
            .and_then(|(table, key)| line_of(table, key));
        return (
            Some(setting.clone()),
            format!("it holds {value}"),
            format!("it must lie from 0 to {max}"),
            line,
        );
    }
    if let ServiceConfigErrorEnum::UnknownTable { table } = &problem.0 {
        return (
            None,
            format!("the table [{table}] does not exist"),
            "remove it, or correct its name".to_string(),
            None,
        );
    }
    (
        None,
        problem.to_string(),
        "correct the setting".to_string(),
        None,
    )
}

/// A parser's refusal: its setting, what was found, what is required, and its line.
fn describe_toml(
    error: &TomlError,
    document: Option<&TomlDocument>,
) -> (Option<String>, String, String, Option<usize>) {
    let line_of = |table: &str, key: &str| document.and_then(|held| held.line_of(table, key));
    let dotted = |table: &str, key: &str| {
        if table.is_empty() {
            key.to_string()
        } else {
            format!("{table}.{key}")
        }
    };
    match &error.0 {
        TomlErrorEnum::UnreadKey { table, key } => (
            Some(dotted(table, key)),
            "no such setting exists".to_string(),
            "remove it, or correct its name".to_string(),
            line_of(table, key),
        ),
        TomlErrorEnum::WrongKind {
            table,
            key,
            expected,
            found,
        } => (
            Some(dotted(table, key)),
            format!("it holds a {found:?} value"),
            format!("it takes a {expected:?} value"),
            line_of(table, key),
        ),
        TomlErrorEnum::Unsupported { line, .. } | TomlErrorEnum::Invalid { line, .. } => (
            None,
            error.to_string(),
            "the settings are standard TOML: tables, keys, strings, integers and booleans"
                .to_string(),
            Some(*line),
        ),
        TomlErrorEnum::Limit { .. } | TomlErrorEnum::Missing { .. } => (
            None,
            error.to_string(),
            "the settings are standard TOML: tables, keys, strings, integers and booleans"
                .to_string(),
            None,
        ),
    }
}
