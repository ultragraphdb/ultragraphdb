/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_frontend::{OwnerStart, OwnerThread, start_owner};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_error_role::{RoleError, RoleErrorEnum};
use ultragraphdb_tool_bdd::{TestError, TestResult};

const CLUSTER: u64 = 7;
const METADATA_SNAPSHOT: &str = "metadata.snapshot";

/// A fresh directory of this test process's own.
fn directory(name: &str) -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let directory = root.join(format!("owner-thread-{name}-{}", std::process::id()));
    if directory.exists() {
        std::fs::remove_dir_all(&directory)?;
    }
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

fn starting() -> TestResult<OwnerStart> {
    Ok(OwnerStart::new(
        MetadataConfig::new(),
        SequencerConfig::new(1, 5)?,
        1,
    ))
}

/// The owner of `start` started over `data`.
async fn started(data: &Path, start: OwnerStart) -> Result<OwnerThread, RoleError> {
    start_owner(
        ServiceConfig::default(),
        CLUSTER,
        data.to_path_buf(),
        start,
        Vec::new(),
    )
    .await
}

/// The error a start refused with.
async fn refused_start(data: &Path, start: OwnerStart) -> TestResult<RoleError> {
    match started(data, start).await {
        Ok(_running) => Err(TestError::new("the owner started")),
        Err(error) => Ok(error),
    }
}

/// A data directory that cannot be created stops the start with an I/O failure naming it.
#[test]
fn test_a_data_directory_that_cannot_be_made_stops_the_start() -> TestResult {
    let parent = directory("not-a-directory")?;
    let file = parent.join("a-file");
    std::fs::write(&file, "a file where the data directory goes")?;
    let data = file.join("data");
    Runtime::new()?.block_on(async {
        let error = refused_start(&data, starting()?).await?;
        assert!(matches!(error.0, RoleErrorEnum::Io { .. }), "{error}");
        assert!(
            error.to_string().contains(&data.display().to_string()),
            "{error}"
        );
        Ok(())
    })
}

/// A snapshot that cannot be read stops the start with an I/O failure naming it; one that does not decode, as a bug's.
#[test]
fn test_a_snapshot_that_does_not_load_stops_the_start() -> TestResult {
    let unreadable = directory("unreadable-snapshot")?;
    std::fs::create_dir_all(unreadable.join(METADATA_SNAPSHOT))?;
    let undecodable = directory("undecodable-snapshot")?;
    std::fs::write(undecodable.join(METADATA_SNAPSHOT), b"no snapshot")?;
    Runtime::new()?.block_on(async {
        let error = refused_start(&unreadable, starting()?).await?;
        assert!(matches!(error.0, RoleErrorEnum::Io { .. }), "{error}");
        assert!(error.to_string().contains(METADATA_SNAPSHOT), "{error}");
        let bug = refused_start(&undecodable, starting()?).await?;
        assert!(matches!(bug.0, RoleErrorEnum::Software { .. }), "{bug}");
        Ok(())
    })
}

/// A bootstrap secret file that cannot be read stops the start with a configuration error naming the setting and the file.
#[test]
fn test_an_unreadable_bootstrap_file_stops_the_start() -> TestResult {
    let data = directory("bootstrap-unreadable")?;
    let file = data.join("absent-secret");
    Runtime::new()?.block_on(async {
        let start = starting()?.with_bootstrap_file(file.display().to_string());
        let error = refused_start(&data, start).await?;
        assert!(
            matches!(error.0, RoleErrorEnum::Configuration { .. }),
            "{error}"
        );
        assert!(
            error.to_string().contains("metadata.bootstrap_secret_file"),
            "{error}"
        );
        assert!(
            error.to_string().contains(&file.display().to_string()),
            "{error}"
        );
        Ok(())
    })
}

/// A tenant the settings name is created at the start, once: a restart finds it held.
#[test]
fn test_a_tenant_the_settings_name_is_created_once() -> TestResult {
    let data = directory("tenants")?;
    let tenant = 3;
    Runtime::new()?.block_on(async {
        for _ in 0..2 {
            let running = started(&data, starting()?.with_tenants(vec![tenant])).await?;
            let facts = running
                .handle()
                .tenant_facts(tenant)
                .await
                .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
            assert!(facts.known());
            running.stopped().await?;
        }
        Ok(())
    })
}

/// A role keeping the handle and the stop apart sees the owner stop once every handle is gone.
#[test]
fn test_the_parts_of_a_running_owner_stop_it_apart() -> TestResult {
    let data = directory("parts")?;
    Runtime::new()?.block_on(async {
        let (handle, stopped) = started(&data, starting()?).await?.into_parts();
        assert!(handle.members().await.is_some());
        drop(handle);
        stopped
            .await
            .map_err(|_| TestError::new("the owner ended without a word"))??;
        Ok(())
    })
}
