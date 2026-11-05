/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataConfig, MetadataState};
use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_log::LogErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_log::{OrderedLog, Position};
use ultragraphdb_transactions_log::FaultMode;

use std::num::NonZeroU32;

use ultragraphdb_cluster_metadata::MetadataGroup;
use ultragraphdb_transactions_log::{SimStorage, Wal};

use crate::utils::harness::{
    DIR, block_on, fresh, fresh_with, open, pid, sync, tenant, wal_config,
};

#[test]
fn test_a_fresh_group_reads_the_empty_state_at_position_0() -> TestResult {
    let (_storage, group) = fresh()?;
    assert_eq!(group.state(), &MetadataState::new());
    assert_eq!(group.applied(), Position::ZERO);
    assert_eq!(group.replayed(), 0);
    assert_eq!(group.config(), &MetadataConfig::new());
    Ok(())
}

#[test]
fn test_a_command_is_read_only_once_durable() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let (version, position) = group.offer_catalog(tenant(), 0, b"v1")?;
    assert_eq!((version, position.get()), (1, 1));
    // Appended, not durable: the state reads answer from does not hold it.
    assert_eq!(group.state().current_version(tenant()), 0);
    assert_eq!(group.applied(), Position::ZERO);
    let durable = block_on(group.sync())??;
    assert_eq!(durable.get(), 1);
    assert_eq!(group.state().current_version(tenant()), 1);
    assert_eq!(group.applied().get(), 1);
    // A sync with nothing appended changes nothing.
    assert_eq!(block_on(group.sync())??.get(), 1);
    assert_eq!(group.applied().get(), 1);
    Ok(())
}

#[test]
fn test_a_command_is_decided_against_every_command_appended_before_it() -> TestResult {
    let (_storage, mut group) = fresh()?;
    let (first, _) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 10)?;
    let (second, _) = group.lease_block(TypePrefix::RESOURCE, pid(2)?, 5)?;
    assert_eq!((first.start().get(), first.end()), (1, 11));
    assert_eq!((second.start().get(), second.end()), (11, 16));
    let (_, _) = group.offer_catalog(tenant(), 0, b"v1")?;
    let (version, position) = group.offer_catalog(tenant(), 1, b"v2")?;
    assert_eq!((version, position.get()), (2, 4));
    sync(&mut group)?;
    assert_eq!(group.state().next_sequence(TypePrefix::RESOURCE), 16);
    assert_eq!(group.state().current_version(tenant()), 2);
    Ok(())
}

#[test]
fn test_every_command_is_one_round_of_the_groups_own_log() -> TestResult {
    let (storage, mut group) = fresh()?;
    let _offer = group.offer_catalog(tenant(), 0, b"v1")?;
    let _lease = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 3)?;
    sync(&mut group)?;
    let mut log = open(&storage)?;
    let rounds = block_on(log.read(Position::new(1), 1 << 20))??;
    let commands: Vec<MetadataCommand> = rounds
        .iter()
        .map(|round| MetadataCommand::decode(round.body()))
        .collect::<Result<_, _>>()?;
    assert_eq!(
        commands,
        vec![
            MetadataCommand::OfferCatalog {
                tenant: tenant(),
                predecessor: 0,
                bytes: b"v1".to_vec()
            },
            MetadataCommand::LeaseBlock {
                prefix: TypePrefix::RESOURCE,
                partition: pid(1)?,
                start: 1,
                end: 4
            },
        ]
    );
    assert!(rounds.iter().all(|round| round.transactions() == 1));
    Ok(())
}

#[test]
fn test_a_round_the_log_refuses_changes_nothing() -> TestResult {
    // A log of two 256 KiB segments: a version of 500 KiB, four chunks, does not fit after a first one of 200 KiB.
    let storage = SimStorage::new(13);
    let config = wal_config().with_capacity(NonZeroU32::MIN.saturating_add(1));
    let (log, _) = block_on(Wal::open(storage.clone(), DIR, config))??;
    let mut group = block_on(MetadataGroup::recover(log, MetadataConfig::new(), None))??;
    let _: (u64, Position) = group.offer_catalog(tenant(), 0, &vec![1; 200 * 1_024])?;
    sync(&mut group)?;
    let refusal = group
        .offer_catalog(tenant(), 1, &vec![2; 500 * 1_024])
        .err()
        .context("a refusal")?;
    assert!(
        matches!(refusal.0, ClusterErrorEnum::Log(ref e) if matches!(e.0, LogErrorEnum::Busy { .. })),
        "{refusal:?}"
    );
    // The refused version is not the latest: an offer on version 1 is not stale, whatever room the log has left.
    match group.offer_catalog(tenant(), 1, b"small") {
        Ok((version, _)) => assert_eq!(version, 2),
        Err(again) => assert!(matches!(again.0, ClusterErrorEnum::Log(_)), "{again:?}"),
    }
    assert_eq!(group.state().current_version(tenant()), 1);
    Ok(())
}

#[test]
fn test_a_refused_command_appends_nothing() -> TestResult {
    let (_storage, mut group) = fresh_with(MetadataConfig::new().with_block_size_max(5)?)?;
    assert_eq!(
        group
            .lease_block(TypePrefix::RESOURCE, pid(1)?, 6)
            .err()
            .context("a refusal")?,
        ClusterError::block_size(6, 5)
    );
    assert_eq!(
        group
            .offer_catalog(tenant(), 3, b"v")
            .err()
            .context("a refusal")?,
        ClusterError::stale_catalog(tenant().get(), 3, 0)
    );
    let (_, position) = group.lease_block(TypePrefix::RESOURCE, pid(1)?, 5)?;
    assert_eq!(position.get(), 1);
    Ok(())
}

#[test]
fn test_a_failed_sync_is_the_logs_error_and_applies_nothing() -> TestResult {
    let (storage, mut group) = fresh()?;
    let _offer = group.offer_catalog(tenant(), 0, b"v1")?;
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    let refusal = block_on(group.sync())?.err().context("a refusal")?;
    let ClusterErrorEnum::Log(_) = refusal.0 else {
        return Err(TestError::new(format!("{refusal:?}")));
    };
    assert_eq!(group.state().current_version(tenant()), 0);
    assert_eq!(group.applied(), Position::ZERO);
    let refused = group
        .lease_block(TypePrefix::RESOURCE, pid(1)?, 1)
        .err()
        .context("a refusal")?;
    assert!(
        matches!(refused.0, ClusterErrorEnum::Log(ref e) if e.0 == LogErrorEnum::Poisoned),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_a_group_runs_with_its_own_settings() -> TestResult {
    let config = MetadataConfig::new()
        .with_sessions_max(3)?
        .with_block_size_max(7)?
        .with_snapshot_rounds(9)?;
    let (_storage, group) = fresh_with(config)?;
    assert_eq!(group.config(), &config);
    Ok(())
}

#[test]
fn test_a_version_larger_than_one_round_commits_across_rounds_and_reads_back_whole() -> TestResult {
    // 300 KiB of bytes: three chunks of 128 KiB at most and their commit, four rounds.
    let (storage, mut group) = fresh()?;
    let large: Vec<u8> = (0..300 * 1_024).map(|i: u32| (i % 251) as u8).collect();
    let (version, position) = group.offer_catalog(tenant(), 0, &large)?;
    assert_eq!((version, position.get()), (1, 4));
    // Until the commit is durable the version is no one's: the state read answers from does not hold it.
    assert_eq!(group.state().current_version(tenant()), 0);
    sync(&mut group)?;
    let held = group.state().versions(tenant());
    assert_eq!(held.first().map(|v| v.bytes()), Some(large.as_slice()));
    assert_eq!(group.applied().get(), 4);
    let mut log = open(&storage)?;
    let rounds = block_on(log.read(Position::new(1), 1 << 20))??;
    let kinds: Vec<u8> = rounds
        .iter()
        .map(|r| r.body().first().copied().unwrap_or(0))
        .collect();
    assert_eq!(kinds, vec![12, 12, 12, 13]);
    drop(log);
    let recovered =
        crate::utils::harness::crash_and_recover(&storage, MetadataConfig::new(), None)?;
    assert_eq!(recovered.state(), group.state());
    assert_eq!(recovered.replayed(), 1);
    assert_eq!(recovered.applied().get(), 4);
    Ok(())
}

#[test]
fn test_a_command_one_byte_past_a_chunk_takes_two_chunks() -> TestResult {
    // An offer's encoding is its 21 header bytes and its version: 131,051 bytes of version fill one round exactly.
    let (_storage, mut group) = fresh()?;
    let (_, one) = group.offer_catalog(tenant(), 0, &vec![3; 131_051])?;
    assert_eq!(one.get(), 1);
    let (_, chunked) = group.offer_catalog(tenant(), 1, &vec![3; 131_052])?;
    assert_eq!(chunked.get(), 4);
    sync(&mut group)?;
    assert_eq!(group.state().current_version(tenant()), 2);
    Ok(())
}
