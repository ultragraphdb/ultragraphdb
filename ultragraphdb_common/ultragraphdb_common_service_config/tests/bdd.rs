/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The service configuration's scenarios, bound to `features/service_config/` and `features/role-binaries/`.

use cucumber::{World, then, when};
use ultragraphdb_common_service_config::{ServiceConfig, ServiceConfigError};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

#[derive(Debug, Default, World)]
struct Steps {
    read: Option<Result<ServiceConfig, ServiceConfigError>>,
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
    w.read = Some(ServiceConfig::parse(text.as_bytes()));
}

#[then("the configuration's storage table holds 4 partitions and its cluster table that file")]
fn the_fields_hold(w: &mut Steps) -> TestResult {
    let config = config(w)?;
    assert_eq!(config.storage().partition_count(), Some(4));
    assert_eq!(config.cluster().role_secret_file(), Some("/run/secret"));
    Ok(())
}

#[when("a file holds the header `[storge]` and no key")]
fn an_empty_table(w: &mut Steps) {
    w.read = Some(ServiceConfig::parse(b"[storge]\n"));
}

#[then("the configuration is refused with an error naming `storge`")]
fn names_storge(w: &mut Steps) -> TestResult {
    let message = refusal(w)?;
    assert!(message.contains("storge"), "{message}");
    Ok(())
}

/// A hosting storage instance's settings, before the lines a step adds.
const HOSTING: &str =
    "[storage]\nrole_address = \"tcp://storage-0:7200\"\ndata_dir = \"/data/storage\"\n[system]\n";

#[when(
    "a storage instance's settings hold a `[system]` table with `role_address` and `data_dir`, a `[metadata]` and a `[sequencer]` table"
)]
fn the_hosting_tables(w: &mut Steps) {
    let text = format!(
        "{HOSTING}role_address = \"tcp://storage-0:7100\"\ndata_dir = \"/data/system\"\n\
         [metadata]\nsessions_max = 1024\nbootstrap_secret_file = \"/run/bootstrap\"\n\
         [sequencer]\nepoch_ms_min = 2\nepoch_ms_max = 10\n"
    );
    w.read = Some(ServiceConfig::parse(text.as_bytes()));
}

#[then("each of their settings reads as written")]
fn read_as_written(w: &mut Steps) -> TestResult {
    let config = config(w)?;
    let system = config.system().context("the [system] table")?;
    assert_eq!(system.role_address(), "tcp://storage-0:7100");
    assert_eq!(system.data_dir(), "/data/system");
    assert_eq!(config.metadata().sessions_max(), Some(1024));
    assert_eq!(
        config.metadata().bootstrap_secret_file(),
        Some("/run/bootstrap")
    );
    assert_eq!(config.sequencer().epoch_ms_min(), Some(2));
    assert_eq!(config.sequencer().epoch_ms_max(), Some(10));
    Ok(())
}

#[when("a storage instance's `[system]` table holds the key `role_adress`")]
fn a_misspelt_system_key(w: &mut Steps) {
    let text = format!(
        "{HOSTING}role_address = \"tcp://storage-0:7100\"\ndata_dir = \"/data/system\"\nrole_adress = \"tcp://storage-0:7101\"\n"
    );
    w.read = Some(ServiceConfig::parse(text.as_bytes()));
}

#[then("the settings are refused with an error naming `role_adress`")]
fn names_role_adress(w: &mut Steps) -> TestResult {
    let message = refusal(w)?;
    assert!(message.contains("role_adress"), "{message}");
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
