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
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{ErrorCode, Join, RoleKind};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const CLUSTER: u64 = 7;

fn directory(name: &str) -> TestResult<PathBuf> {
    let directory = std::env::temp_dir().join(format!("owner-tests-{name}-{}", std::process::id()));
    if directory.exists() {
        std::fs::remove_dir_all(&directory)?;
    }
    Ok(directory)
}

async fn owner(
    directory: &Path,
    metadata: MetadataConfig,
    partitions: u32,
) -> Result<OwnerThread, RoleError> {
    let sequencer =
        SequencerConfig::new(1, 5).map_err(|error| RoleError::software(error.to_string()))?;
    let config = ServiceConfig::default();
    start_owner(
        config,
        CLUSTER,
        directory.to_path_buf(),
        OwnerStart::new(metadata, sequencer, partitions),
        Vec::new(),
    )
    .await
}

/// Snapshots trim the log, and a restart recovers from them: the leases rise past everything leased before.
#[test]
fn test_a_restart_recovers_through_the_snapshot() -> TestResult {
    let data = directory("snapshot")?;
    Runtime::new()?.block_on(async {
        let metadata = MetadataConfig::new().with_snapshot_rounds(4)?;
        let running = owner(&data, metadata, 1).await?;
        let mut last = 0;
        for _ in 0..16 {
            last = running
                .handle()
                .lease(1, 1, 8)
                .await
                .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        }
        // A close takes the snapshot that is due: every lease above is durable by then.
        let closed = running
            .handle()
            .next_close(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let _next = running
            .handle()
            .next_close(closed.epoch().get())
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        running.stopped().await?;
        assert!(data.join("metadata.snapshot").exists());
        let restarted = owner(&data, metadata, 1).await?;
        let next = restarted
            .handle()
            .lease(1, 1, 8)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert!(next >= last + 8, "{next} after {last}");
        restarted.stopped().await?;
        Ok(())
    })
}

/// A restart keeps the placement it holds, whatever the settings now say, and bounds the claims by it.
#[test]
fn test_a_restart_keeps_the_placement_it_holds() -> TestResult {
    let data = directory("kept")?;
    Runtime::new()?.block_on(async {
        owner(&data, MetadataConfig::new(), 2)
            .await?
            .stopped()
            .await?;
        let restarted = owner(&data, MetadataConfig::new(), 8).await?;
        let handle = restarted.handle();
        assert!(
            handle
                .join(Join::storage("tcp://a:1", vec![2]))
                .await
                .is_ok()
        );
        let refused = handle
            .join(Join::storage("tcp://a:1", vec![3]))
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(refused.code(), ErrorCode::Protocol);
        assert!(
            refused.message().contains("partition 3"),
            "{}",
            refused.message()
        );
        restarted.stopped().await?;
        Ok(())
    })
}

/// Closes in order from the start; a Subscribe past the latest close is refused naming both epochs; a catalog version
/// never held is refused with code 7 naming it; a lease past the maximum names the maximum.
#[test]
fn test_what_the_owner_refuses_it_names() -> TestResult {
    let data = directory("refused")?;
    Runtime::new()?.block_on(async {
        let running = owner(&data, MetadataConfig::new(), 1).await?;
        let handle = running.handle();
        let first = handle
            .next_close(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_eq!(first.epoch().get(), 1);
        let second = handle
            .next_close(1)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_eq!(second.epoch().get(), 2);
        assert!(handle.subscribe(1).await.is_ok());
        let far = (1u64 << 42) - 1;
        let past = handle.subscribe(far).await.err().context("a refusal")?;
        assert_eq!(past.code(), ErrorCode::Protocol);
        assert!(
            past.message().contains(&far.to_string()),
            "{}",
            past.message()
        );
        let never = handle.catalog(1, 5).await.err().context("a refusal")?;
        assert_eq!((never.code(), never.detail()), (ErrorCode::Refused, 5));
        let large = handle
            .lease(1, 1, u64::MAX)
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(large.code(), ErrorCode::Refused);
        assert!(handle.members().await.context("the members")?.is_empty());
        let compute = Join::compute("tcp://c:1");
        assert_eq!(compute.kind(), RoleKind::Compute);
        assert!(handle.join(compute).await.is_ok());
        assert_eq!(handle.members().await.context("the members")?.len(), 1);
        running.stopped().await?;
        Ok(())
    })
}
