/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::{Path, PathBuf};

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::{MetadataConfig, MetadataGroup};
use ultragraphdb_cluster_sequencer::{ClockReading, SequencerConfig};
use ultragraphdb_common_frontend::{Owner, OwnerStart, OwnerThread, start_owner};
use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{ErrorCode, Join, RoleKind};
use ultragraphdb_store_sim::LogCapture;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

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

/// The owner's two logs under `directory`.
async fn logs(directory: &Path) -> TestResult<(Wal<CompioStorage>, Wal<CompioStorage>)> {
    std::fs::create_dir_all(directory)?;
    let (metadata, _) = Wal::open(
        CompioStorage,
        directory.join("meta"),
        WalConfig::new(CLUSTER, false),
    )
    .await?;
    let (sequencer, _) = Wal::open(
        CompioStorage,
        directory.join("seq"),
        WalConfig::new(CLUSTER, false),
    )
    .await?;
    Ok((metadata, sequencer))
}

/// The settings of an owner placing `partitions`.
fn starting(partitions: u32) -> TestResult<OwnerStart> {
    Ok(OwnerStart::new(
        MetadataConfig::new(),
        SequencerConfig::new(1, 5)?,
        partitions,
    ))
}

/// An owner recovered over the logs under `directory` and run no further: dropping it is a process stopped there, with nothing
/// synced past its recovery.
async fn recovered(
    directory: &Path,
    start: &OwnerStart,
) -> TestResult<Owner<Wal<CompioStorage>, Wal<CompioStorage>>> {
    let (metadata, sequencer) = logs(directory).await?;
    Ok(Owner::recover(metadata, sequencer, start, None, ClockReading::new(1, 1)).await?)
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
        assert_eq!(large.detail(), MetadataConfig::new().block_size_max());
        assert!(handle.members().await.context("the members")?.is_empty());
        let compute = Join::compute("tcp://c:1");
        assert_eq!(compute.kind(), RoleKind::Compute);
        assert!(handle.join(compute).await.is_ok());
        assert_eq!(handle.members().await.context("the members")?.len(), 1);
        running.stopped().await?;
        Ok(())
    })
}

/// An owner's debug form names it, its membership and how many closes it has waiting, and nothing it holds besides.
#[test]
fn test_an_owners_debug_form_names_its_membership_and_waits() -> TestResult {
    let data = directory("debug")?;
    Runtime::new()?.block_on(async {
        let owner = recovered(&data, &starting(1)?).await?;
        let shown = format!("{owner:?}");
        assert!(shown.starts_with("Owner {"), "{shown}");
        assert!(shown.contains("membership: "), "{shown}");
        assert!(shown.contains("closes_waited: 0"), "{shown}");
        assert!(shown.ends_with(", .. }"), "{shown}");
        Ok(())
    })
}

/// The secret an unknown name's verifier derives from is drawn at random on a cluster's first start and kept by a restart.
#[test]
fn test_the_unknown_secret_is_drawn_once_per_cluster() -> TestResult {
    let (one, other) = (directory("secret-one")?, directory("secret-other")?);
    Runtime::new()?.block_on(async {
        let first = owner(&one, MetadataConfig::new(), 1).await?;
        let drawn = first.unknown_secret();
        first.stopped().await?;
        let restarted = owner(&one, MetadataConfig::new(), 1).await?;
        assert_eq!(restarted.unknown_secret(), drawn, "kept by a restart");
        restarted.stopped().await?;
        let elsewhere = owner(&other, MetadataConfig::new(), 1).await?;
        assert_ne!(elsewhere.unknown_secret(), drawn, "drawn per cluster");
        elsewhere.stopped().await?;
        Ok(())
    })
}

/// A secret file named on a group holding no principal creates the bootstrap administrator, durable before the owner serves; a
/// group holding a principal reads no file.
#[test]
fn test_a_bootstrap_file_creates_the_administrator_once() -> TestResult {
    let data = directory("bootstrap")?;
    std::fs::create_dir_all(&data)?;
    let file = data.join("bootstrap-secret");
    std::fs::write(&file, "the administrator's secret")?;
    let named = || -> TestResult<OwnerStart> {
        Ok(starting(1)?.with_bootstrap_file(file.display().to_string()))
    };
    Runtime::new()?.block_on(async {
        drop(recovered(&data, &starting(1)?).await?);
        // A process stopped right after the restart that created it: nothing but recovery synced the group.
        drop(recovered(&data, &named()?).await?);
        std::fs::remove_file(&file)?;
        // The file is gone: only a group holding the administrator starts without reading it.
        drop(recovered(&data, &named()?).await?);
        let running = owner(&data, MetadataConfig::new(), 1).await?;
        let held = running.handle().verifier("admin".to_string()).await;
        assert!(held.is_some(), "the administrator");
        running.stopped().await?;
        Ok(())
    })
}

/// A group holding tenant 0 and no secret yet, as a process stopped between the two would leave it, draws the secret on its
/// next start and keeps it durable before the owner serves.
#[test]
fn test_a_secret_drawn_at_a_later_start_is_durable() -> TestResult {
    let data = directory("late-secret")?;
    Runtime::new()?.block_on(async {
        let (log, sequencer) = logs(&data).await?;
        drop(sequencer);
        let mut group = MetadataGroup::recover(log, MetadataConfig::new(), None).await?;
        let _created = group.create_tenant(Tenant::new(0))?;
        let _durable = group.sync().await?;
        drop(group);
        let drawn = recovered(&data, &starting(1)?).await?.unknown_secret();
        assert!(drawn.is_some(), "drawn");
        let kept = recovered(&data, &starting(1)?).await?.unknown_secret();
        assert_eq!(kept, drawn, "durable before the first start returned");
        Ok(())
    })
}

/// A restart over a group placed over another partition count than the settings name keeps its placement and says so; one over
/// the same count says nothing of it.
#[test]
fn test_a_placement_over_another_count_is_named_in_the_log() -> TestResult {
    let data = directory("count-warned")?;
    let warned = |capture: &LogCapture| {
        capture
            .events()
            .iter()
            .any(|event| event.to_string().contains("another partition count"))
    };
    let same = LogCapture::new();
    same.capture(|| -> TestResult {
        Runtime::new()?.block_on(async {
            drop(recovered(&data, &starting(2)?).await?);
            drop(recovered(&data, &starting(2)?).await?);
            Ok(())
        })
    })?;
    assert!(!warned(&same), "the same count");
    let other = LogCapture::new();
    other.capture(|| -> TestResult {
        Runtime::new()?.block_on(async {
            drop(recovered(&data, &starting(8)?).await?);
            Ok(())
        })
    })?;
    assert!(warned(&other), "another count");
    Ok(())
}
