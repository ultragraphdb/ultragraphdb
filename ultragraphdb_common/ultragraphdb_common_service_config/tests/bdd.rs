/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The service configuration's scenarios, bound to `features/service_config/`.

use std::path::Path;

use cucumber::{World, then, when};
use ultragraphdb_common_service_config::{ServiceConfig, ServiceConfigError};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const MISSING_FILE: &str = "/nonexistent/ultragraphdb/settings.toml";

#[derive(Debug, Default, World)]
struct Steps {
    read: Option<Result<ServiceConfig, ServiceConfigError>>,
}

fn variable(name: &str, value: &str) -> (String, String) {
    (name.to_string(), value.to_string())
}

fn config(w: &mut Steps) -> TestResult<ServiceConfig> {
    Ok(w.read.take().context("a reading")??)
}

/// The message of the reading's refusal.
fn refusal(w: &mut Steps) -> TestResult<String> {
    match w.read.take().context("a reading")? {
        Ok(config) => Err(TestError::new(format!("accepted as {config:?}"))),
        Err(error) => Ok(error.to_string()),
    }
}

#[when(
    "a file holds `[storage] partition_count = 4` and `[cluster] role_secret_file = \"/run/secret\"`"
)]
fn a_file_holds(w: &mut Steps) {
    let text = "[storage]\npartition_count = 4\n[cluster]\nrole_secret_file = \"/run/secret\"\n";
    w.read = Some(ServiceConfig::parse(Some(text.as_bytes()), []));
}

#[then("the configuration's storage table holds 4 partitions and its cluster table that file")]
fn the_fields_hold(w: &mut Steps) -> TestResult {
    let config = config(w)?;
    assert_eq!(config.storage().partition_count(), Some(4));
    assert_eq!(config.cluster().role_secret_file(), Some("/run/secret"));
    Ok(())
}

#[when("no file is named and `ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=2` is set")]
fn no_file(w: &mut Steps) {
    let set = [variable("ULTRAGRAPHDB_STORAGE_PARTITION_COUNT", "2")];
    w.read = Some(ServiceConfig::load(None, set));
}

#[then("the storage table holds 2 partitions and every other setting is absent")]
fn only_the_variable(w: &mut Steps) -> TestResult {
    let config = config(w)?;
    assert_eq!(config.storage().partition_count(), Some(2));
    let only = ServiceConfig::parse(Some(b"[storage]\npartition_count = 2\n"), [])?;
    assert_eq!(config, only);
    Ok(())
}

#[when("a file is named that does not exist")]
fn a_missing_file(w: &mut Steps) {
    w.read = Some(ServiceConfig::load(Some(Path::new(MISSING_FILE)), []));
}

#[then("the configuration is refused with an error naming the file")]
fn names_the_file(w: &mut Steps) -> TestResult {
    let message = refusal(w)?;
    assert!(message.contains(MISSING_FILE), "{message}");
    Ok(())
}

#[when("a file holds the header `[storge]` and no key")]
fn an_empty_table(w: &mut Steps) {
    w.read = Some(ServiceConfig::parse(Some(b"[storge]\n"), []));
}

#[then("the configuration is refused with an error naming `storge`")]
fn names_storge(w: &mut Steps) -> TestResult {
    let message = refusal(w)?;
    assert!(message.contains("storge"), "{message}");
    Ok(())
}

#[when("`ULTRAGRAPHDB_STORAGE_PARTITON_COUNT=2` is set")]
fn a_misspelled_variable(w: &mut Steps) {
    let set = [variable("ULTRAGRAPHDB_STORAGE_PARTITON_COUNT", "2")];
    w.read = Some(ServiceConfig::parse(None, set));
}

#[then("the configuration is refused with an error naming the variable")]
fn names_the_variable(w: &mut Steps) -> TestResult {
    let message = refusal(w)?;
    assert!(
        message.contains("ULTRAGRAPHDB_STORAGE_PARTITON_COUNT"),
        "{message}"
    );
    Ok(())
}

#[when("`ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=many` is set")]
fn a_wrong_kind(w: &mut Steps) {
    let set = [variable("ULTRAGRAPHDB_STORAGE_PARTITION_COUNT", "many")];
    w.read = Some(ServiceConfig::parse(None, set));
}

#[then(
    "the configuration is refused with an error naming the variable and the integer it expected"
)]
fn names_the_kind(w: &mut Steps) -> TestResult {
    let message = refusal(w)?;
    assert!(
        message.contains("ULTRAGRAPHDB_STORAGE_PARTITION_COUNT"),
        "{message}"
    );
    assert!(message.contains("integer"), "{message}");
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
