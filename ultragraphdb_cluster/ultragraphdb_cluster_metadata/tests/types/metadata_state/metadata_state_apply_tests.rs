/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataState};
use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId};
use ultragraphdb_core_ids::{Epoch, Namespace, TypePrefix};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Namespace `n`.
fn ns(n: u64) -> Namespace {
    Namespace::new(n)
}

/// Offers `bytes` as the version of `namespace` after `predecessor`.
fn offer(namespace: u64, predecessor: u64, bytes: &[u8]) -> MetadataCommand {
    MetadataCommand::OfferCatalog {
        namespace: ns(namespace),
        predecessor,
        bytes: bytes.to_vec(),
    }
}

/// The close of `epoch` announcing `versions` and carrying `evictions`.
fn close(epoch: u64, versions: &[(u64, u64)], evictions: &[u128]) -> TestResult<MetadataCommand> {
    Ok(MetadataCommand::LearnClose {
        epoch: Epoch::new(epoch)?,
        versions: versions.iter().map(|(n, v)| (ns(*n), *v)).collect(),
        evictions: evictions.to_vec(),
    })
}

/// A verifier no test authenticates with.
fn verifier() -> ScramVerifier {
    ScramVerifier::new(vec![1; 16], 4_096, [2; 32], [3; 32])
}

/// The principal `id` named `name`.
fn create(id: u64, name: &str, administrator: bool) -> MetadataCommand {
    MetadataCommand::CreatePrincipal {
        id,
        name: name.to_string(),
        verifier: verifier(),
        administrator,
    }
}

/// The refusal of applying `command` to `state`, which is checked to be unchanged.
fn refused(state: &mut MetadataState, command: &MetadataCommand) -> TestResult<ClusterErrorEnum> {
    let before = state.clone();
    let refusal = state.apply(command).err().context("a refusal")?;
    assert_eq!(state, &before, "a refused command changes nothing");
    Ok(refusal.0)
}

/// Whether `error` refuses a decision the state contradicts.
fn contradicts(error: &ClusterErrorEnum) -> bool {
    matches!(error, ClusterErrorEnum::Contradicts { .. })
}

#[test]
fn test_versions_chain_from_1_per_namespace() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&offer(1, 0, b"a1"))?;
    state.apply(&offer(1, 1, b"a2"))?;
    state.apply(&offer(2, 0, b"b1"))?;
    assert_eq!(state.current_version(ns(1)), 2);
    assert_eq!(state.current_version(ns(2)), 1);
    assert_eq!(
        state.pending_versions(),
        vec![(ns(1), 1), (ns(1), 2), (ns(2), 1)]
    );
    assert_eq!(state.namespaces(), vec![ns(1), ns(2)]);
    Ok(())
}

#[test]
fn test_a_version_not_on_the_latest_is_refused_naming_both() -> TestResult {
    let mut state = MetadataState::new();
    assert_eq!(
        refused(&mut state, &offer(1, 1, b"x"))?,
        ClusterError::stale_catalog(1, 1, 0).0
    );
    state.apply(&offer(1, 0, b"a1"))?;
    state.apply(&offer(1, 1, b"a2"))?;
    assert_eq!(
        refused(&mut state, &offer(1, 1, b"x"))?,
        ClusterError::stale_catalog(1, 1, 2).0
    );
    assert_eq!(
        refused(&mut state, &offer(1, 3, b"x"))?,
        ClusterError::stale_catalog(1, 3, 2).0
    );
    Ok(())
}

#[test]
fn test_a_version_past_its_bytes_limit_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    let big = vec![0; 16_777_217];
    assert_eq!(
        refused(&mut state, &offer(1, 0, &big))?,
        ClusterError::too_many("bytes of a catalog version", 16_777_217, 16_777_216).0
    );
    state.apply(&offer(1, 0, &vec![0; 16_777_216]))?;
    Ok(())
}

#[test]
fn test_at_most_256_versions_are_pending() -> TestResult {
    let mut state = MetadataState::new();
    for namespace in 0..256 {
        state.apply(&offer(namespace, 0, b"v"))?;
    }
    assert_eq!(
        refused(&mut state, &offer(256, 0, b"v"))?,
        ClusterError::full("pending catalog versions", 256).0
    );
    // A close announcing them all frees the pending set.
    let all: Vec<(u64, u64)> = (0..256).map(|n| (n, 1)).collect();
    state.apply(&close(1, &all, &[])?)?;
    state.apply(&offer(256, 0, b"v"))?;
    Ok(())
}

#[test]
fn test_a_namespace_retains_at_most_1024_versions() -> TestResult {
    let mut state = MetadataState::new();
    for version in 1..=1_024 {
        state.apply(&offer(1, version - 1, b"v"))?;
        state.apply(&close(version, &[(1, version)], &[])?)?;
    }
    assert_eq!(
        refused(&mut state, &offer(1, 1_024, b"v"))?,
        ClusterError::full("retained catalog versions of a namespace", 1_024).0
    );
    Ok(())
}

#[test]
fn test_a_close_assigns_the_epoch_after_it() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&offer(1, 0, b"a1"))?;
    state.apply(&close(38, &[(1, 1)], &[])?)?;
    assert_eq!(state.last_learned(), Epoch::new(38)?);
    let record = state.versions(ns(1));
    let record = record.first().context("version 1")?;
    assert_eq!(record.effective(), Some(Epoch::new(39)?));
    assert!(state.pending_versions().is_empty());
    Ok(())
}

#[test]
fn test_closes_are_learned_in_order() -> TestResult {
    let mut state = MetadataState::new();
    // The first close learned may be any epoch: the sequencer may have closed epochs before the group's log began.
    state.apply(&close(5, &[], &[])?)?;
    assert_eq!(
        refused(&mut state, &close(7, &[], &[])?)?,
        ClusterError::epoch_ahead(7, 6).0
    );
    assert_eq!(
        refused(&mut state, &close(5, &[], &[])?)?,
        ClusterError::epoch_ahead(5, 6).0
    );
    state.apply(&close(6, &[], &[])?)?;
    assert_eq!(state.last_learned(), Epoch::new(6)?);
    Ok(())
}

#[test]
fn test_a_close_of_the_last_epoch_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    let last = Epoch::MAX.get();
    assert_eq!(
        refused(&mut state, &close(last, &[], &[])?)?,
        ClusterError::epoch_limit(last).0
    );
    state.apply(&close(last - 1, &[], &[])?)?;
    Ok(())
}

#[test]
fn test_a_close_announcing_a_version_not_pending_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&offer(1, 0, b"a1"))?;
    assert_eq!(
        refused(&mut state, &close(1, &[(1, 2)], &[])?)?,
        ClusterError::not_pending(1, 2).0
    );
    assert_eq!(
        refused(&mut state, &close(1, &[(2, 1)], &[])?)?,
        ClusterError::not_pending(2, 1).0
    );
    state.apply(&close(1, &[(1, 1)], &[])?)?;
    // Announced already: no longer pending.
    assert_eq!(
        refused(&mut state, &close(2, &[(1, 1)], &[])?)?,
        ClusterError::not_pending(1, 1).0
    );
    Ok(())
}

#[test]
fn test_a_default_entry_is_kept_and_creates_no_version() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&offer(1, 0, b"a1"))?;
    let first = MetadataCommand::RecordDefault {
        namespace: ns(1),
        predicate: "score".to_string(),
        entry: b"first".to_vec(),
    };
    state.apply(&first)?;
    let again = MetadataCommand::RecordDefault {
        namespace: ns(1),
        predicate: "score".to_string(),
        entry: b"second".to_vec(),
    };
    state.apply(&again)?;
    assert_eq!(state.default_entry(ns(1), "score"), Some(&b"first"[..]));
    assert_eq!(state.default_entry(ns(2), "score"), None);
    assert_eq!(state.default_entry(ns(1), "scores"), None);
    assert_eq!(state.current_version(ns(1)), 1);
    assert_eq!(state.pending_versions(), vec![(ns(1), 1)]);
    Ok(())
}

#[test]
fn test_a_default_entry_past_its_limits_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    let long = MetadataCommand::RecordDefault {
        namespace: ns(1),
        predicate: "p".repeat(4_097),
        entry: Vec::new(),
    };
    assert_eq!(
        refused(&mut state, &long)?,
        ClusterError::too_many("bytes of a predicate's name", 4_097, 4_096).0
    );
    let big = MetadataCommand::RecordDefault {
        namespace: ns(1),
        predicate: "p".to_string(),
        entry: vec![0; 65_537],
    };
    assert_eq!(
        refused(&mut state, &big)?,
        ClusterError::too_many("bytes of a default entry", 65_537, 65_536).0
    );
    Ok(())
}

#[test]
fn test_a_lease_raises_its_types_high_water_mark() -> TestResult {
    let mut state = MetadataState::new();
    let partition = PartitionId::new(1)?;
    let lease = |start, end| MetadataCommand::LeaseBlock {
        prefix: TypePrefix::RESOURCE,
        partition,
        start,
        end,
    };
    state.apply(&lease(1, 1_001))?;
    assert_eq!(state.next_sequence(TypePrefix::RESOURCE), 1_001);
    assert_eq!(state.next_sequence(TypePrefix::STATEMENT), 1);
    // A block not at the high-water mark, or empty, contradicts the state: no group decided it against this one.
    assert!(contradicts(&refused(&mut state, &lease(1_002, 2_000))?));
    assert!(contradicts(&refused(&mut state, &lease(1, 1_001))?));
    assert!(contradicts(&refused(&mut state, &lease(1_001, 1_001))?));
    state.apply(&lease(1_001, 1_002))?;
    assert_eq!(state.next_sequence(TypePrefix::RESOURCE), 1_002);
    Ok(())
}

#[test]
fn test_the_placement_map_is_initialised_once() -> TestResult {
    let mut state = MetadataState::new();
    let init = MetadataCommand::InitPlacement {
        partitions: 2,
        leader: NodeId::new(4),
    };
    let set = MetadataCommand::SetLeader {
        partition: PartitionId::new(1)?,
        node: NodeId::new(5),
    };
    assert_eq!(refused(&mut state, &set)?, ClusterError::no_placement().0);
    let change = MetadataCommand::ChangePlacement {
        from: Epoch::new(3)?,
        assignments: vec![(BucketRange::new(0, 16_777_216)?, PartitionId::new(1)?)],
    };
    assert_eq!(
        refused(&mut state, &change)?,
        ClusterError::no_placement().0
    );
    assert_eq!(
        refused(
            &mut state,
            &MetadataCommand::InitPlacement {
                partitions: 0,
                leader: NodeId::new(4)
            }
        )?,
        ClusterError::partition_count(0, 1_024).0
    );
    state.apply(&init)?;
    assert_eq!(
        refused(&mut state, &init)?,
        ClusterError::full("placement maps", 1).0
    );
    state.apply(&set)?;
    let map = state.placement().context("the map")?;
    assert_eq!(
        map.preferred_leader(PartitionId::new(1)?),
        Some(NodeId::new(5))
    );
    assert_eq!(
        map.preferred_leader(PartitionId::new(2)?),
        Some(NodeId::new(4))
    );
    state.apply(&change)?;
    let map_2 = state.placement().context("the map")?;
    assert_eq!(map_2.latest_change(), Epoch::new(3)?);
    Ok(())
}

#[test]
fn test_a_change_past_the_most_ranges_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&MetadataCommand::InitPlacement {
        partitions: 1,
        leader: NodeId::new(1),
    })?;
    let assignments: Vec<_> = (0..262_145_u32)
        .map(|i| Ok((BucketRange::new(i, i + 1)?, PartitionId::new(1)?)))
        .collect::<Result<_, ClusterError>>()?;
    let change = MetadataCommand::ChangePlacement {
        from: Epoch::new(1)?,
        assignments,
    };
    assert_eq!(
        refused(&mut state, &change)?,
        ClusterError::too_many("ranges of a placement change", 262_145, 262_144).0
    );
    Ok(())
}

#[test]
fn test_principals_are_numbered_and_named_once() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&create(1, "ann", true))?;
    assert_eq!(
        refused(&mut state, &create(2, "ann", false))?,
        ClusterError::principal_exists("ann").0
    );
    assert!(contradicts(&refused(&mut state, &create(3, "bob", false))?));
    assert!(contradicts(&refused(&mut state, &create(1, "bob", false))?));
    state.apply(&create(2, "bob", false))?;
    assert_eq!(state.principals(), 2);
    assert_eq!(state.principal("bob").map(|p| p.id()), Some(2));
    Ok(())
}

#[test]
fn test_a_principals_name_holds_1_to_255_bytes() -> TestResult {
    let mut state = MetadataState::new();
    assert_eq!(
        refused(&mut state, &create(1, "", false))?,
        ClusterError::principal_name(0).0
    );
    assert_eq!(
        refused(&mut state, &create(1, &"n".repeat(256), false))?,
        ClusterError::principal_name(256).0
    );
    state.apply(&create(1, &"n".repeat(255), false))?;
    Ok(())
}

#[test]
fn test_a_verifiers_salt_holds_at_most_255_bytes() -> TestResult {
    let mut state = MetadataState::new();
    let salted = |id, name: &str, salt: usize| MetadataCommand::CreatePrincipal {
        id,
        name: name.to_string(),
        verifier: ScramVerifier::new(vec![1; salt], 4_096, [2; 32], [3; 32]),
        administrator: false,
    };
    assert_eq!(
        refused(&mut state, &salted(1, "ann", 256))?,
        ClusterError::too_many("bytes of a verifier's salt", 256, 255).0
    );
    state.apply(&salted(1, "ann", 255))?;
    Ok(())
}

#[test]
fn test_the_last_administrator_is_not_removed() -> TestResult {
    let mut state = MetadataState::new();
    let remove = |name: &str| MetadataCommand::RemovePrincipal {
        name: name.to_string(),
    };
    assert_eq!(
        refused(&mut state, &remove("ann"))?,
        ClusterError::unknown_principal("ann").0
    );
    state.apply(&create(1, "root", true))?;
    state.apply(&create(2, "bob", false))?;
    assert_eq!(
        refused(&mut state, &remove("root"))?,
        ClusterError::last_administrator("root").0
    );
    state.apply(&remove("bob"))?;
    state.apply(&create(3, "admin", true))?;
    // With a second administrator, the first may go; the identities are never reused.
    state.apply(&remove("root"))?;
    assert_eq!(
        refused(&mut state, &remove("admin"))?,
        ClusterError::last_administrator("admin").0
    );
    assert!(contradicts(&refused(&mut state, &create(1, "bob", false))?));
    state.apply(&create(4, "bob", false))?;
    Ok(())
}

#[test]
fn test_a_session_registers_once_for_a_principal_that_exists() -> TestResult {
    let mut state = MetadataState::new();
    let register = |session, principal, evicts: &[u128]| MetadataCommand::RegisterSession {
        session,
        principal,
        evicts: evicts.to_vec(),
    };
    assert_eq!(
        refused(&mut state, &register(10, 1, &[]))?,
        ClusterError::unknown_principal("#1").0
    );
    state.apply(&create(1, "ann", false))?;
    state.apply(&register(10, 1, &[]))?;
    assert_eq!(
        refused(&mut state, &register(10, 1, &[]))?,
        ClusterError::session_exists(10).0
    );
    state.apply(&register(20, 1, &[]))?;
    // Evictions name the oldest sessions, in registration order.
    assert!(contradicts(&refused(&mut state, &register(30, 1, &[20]))?));
    assert!(contradicts(&refused(
        &mut state,
        &register(30, 1, &[10, 99])
    )?));
    state.apply(&register(30, 1, &[10]))?;
    assert_eq!(state.sessions(), 2);
    assert_eq!(state.pending_evictions(), vec![10]);
    assert_eq!(
        state.session_principal(10).err().context("evicted")?.0,
        ClusterError::session_evicted(10).0
    );
    assert_eq!(state.session_principal(20)?, 1);
    Ok(())
}

#[test]
fn test_a_close_carries_the_pending_evictions_away() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&create(1, "ann", false))?;
    for session in 1..=3 {
        state.apply(&MetadataCommand::RegisterSession {
            session,
            principal: 1,
            evicts: Vec::new(),
        })?;
    }
    state.apply(&MetadataCommand::RegisterSession {
        session: 4,
        principal: 1,
        evicts: vec![1, 2],
    })?;
    assert_eq!(state.pending_evictions(), vec![1, 2]);
    // A close carries some of them; the rest wait for the next. An eviction carried twice changes nothing.
    state.apply(&close(1, &[], &[1])?)?;
    assert_eq!(state.pending_evictions(), vec![2]);
    state.apply(&close(2, &[], &[1, 2])?)?;
    assert!(state.pending_evictions().is_empty());
    Ok(())
}

#[test]
fn test_at_most_4096_evictions_are_offered_to_a_close() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&create(1, "ann", false))?;
    for session in 0..5_000_u128 {
        state.apply(&MetadataCommand::RegisterSession {
            session,
            principal: 1,
            evicts: Vec::new(),
        })?;
    }
    let evicts: Vec<u128> = (0..4_096).collect();
    state.apply(&MetadataCommand::RegisterSession {
        session: 9_999,
        principal: 1,
        evicts,
    })?;
    state.apply(&MetadataCommand::RegisterSession {
        session: 9_998,
        principal: 1,
        evicts: vec![4_096],
    })?;
    let offered = state.pending_evictions();
    assert_eq!(offered.len(), 4_096);
    assert_eq!(offered.first(), Some(&0));
    assert_eq!(offered.last(), Some(&4_095));
    Ok(())
}

#[test]
fn test_reports_keep_each_partitions_latest() -> TestResult {
    let mut state = MetadataState::new();
    let report = |partition, epoch| -> TestResult<MetadataCommand> {
        Ok(MetadataCommand::ReportUntrimmed {
            partition: PartitionId::new(partition)?,
            epoch: Epoch::new(epoch)?,
        })
    };
    assert_eq!(state.untrimmed(PartitionId::new(1)?), None);
    state.apply(&report(1, 10)?)?;
    state.apply(&report(1, 4)?)?;
    assert_eq!(state.untrimmed(PartitionId::new(1)?), Some(Epoch::new(10)?));
    state.apply(&report(1, 12)?)?;
    assert_eq!(state.untrimmed(PartitionId::new(1)?), Some(Epoch::new(12)?));
    Ok(())
}

#[test]
fn test_versions_no_untrimmed_log_names_are_released() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&MetadataCommand::InitPlacement {
        partitions: 2,
        leader: NodeId::new(1),
    })?;
    // Versions 1, 2 and 3 of namespace 1 take effect at 11, 21 and 31; version 1 of namespace 2 at 21.
    state.apply(&offer(1, 0, b"a1"))?;
    state.apply(&close(10, &[(1, 1)], &[])?)?;
    for epoch in 11..20 {
        state.apply(&close(epoch, &[], &[])?)?;
    }
    state.apply(&offer(1, 1, b"a2"))?;
    state.apply(&offer(2, 0, b"b1"))?;
    state.apply(&close(20, &[(1, 2), (2, 1)], &[])?)?;
    for epoch in 21..30 {
        state.apply(&close(epoch, &[], &[])?)?;
    }
    state.apply(&offer(1, 2, b"a3"))?;
    state.apply(&close(30, &[(1, 3)], &[])?)?;
    let report = |partition, epoch| -> TestResult<MetadataCommand> {
        Ok(MetadataCommand::ReportUntrimmed {
            partition: PartitionId::new(partition)?,
            epoch: Epoch::new(epoch)?,
        })
    };
    // Partition 2 has not reported: nothing is released.
    state.apply(&report(1, 25)?)?;
    assert_eq!(state.versions(ns(1)).len(), 3);
    // Both logs begin at 25 or later: version 2 is in effect there, so version 1 is released.
    state.apply(&report(2, 26)?)?;
    let kept: Vec<u64> = state.versions(ns(1)).iter().map(|v| v.version()).collect();
    assert_eq!(kept, vec![2, 3]);
    assert_eq!(state.versions(ns(2)).len(), 1);
    assert_eq!(
        state
            .catalog_at(ns(1), Epoch::new(15)?)
            .err()
            .context("released")?
            .0,
        ClusterError::catalog_released(1, 15).0
    );
    assert_eq!(
        state
            .catalog_at(ns(1), Epoch::new(25)?)?
            .map(|v| v.version()),
        Some(2)
    );
    // An epoch before namespace 2's first version reads its initial catalog: nothing of it was released.
    assert_eq!(state.catalog_at(ns(2), Epoch::new(15)?)?, None);
    Ok(())
}

#[test]
fn test_at_most_65536_evictions_await_a_close() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&create(1, "ann", false))?;
    let register = |session, evicts: Vec<u128>| MetadataCommand::RegisterSession {
        session,
        principal: 1,
        evicts,
    };
    let sessions: u128 = 16 * 4_096 + 1;
    for session in 0..sessions {
        state.apply(&register(session, Vec::new()))?;
    }
    // Sixteen registrations, each evicting the 4,096 oldest, fill the 65,536 evictions awaiting a close.
    for batch in 0..16_u128 {
        let evicts: Vec<u128> = (batch * 4_096..(batch + 1) * 4_096).collect();
        state.apply(&register(sessions + batch, evicts))?;
    }
    assert_eq!(state.pending_evictions().len(), 4_096);
    assert_eq!(
        refused(&mut state, &register(2 * sessions, vec![65_536]))?,
        ClusterError::full("session evictions awaiting a close", 65_536).0
    );
    // A registration evicting nothing still registers.
    state.apply(&register(2 * sessions, Vec::new()))?;
    // A close carrying some makes room again.
    let carried: Vec<u128> = (0..4_096).collect();
    state.apply(&close(1, &[], &carried)?)?;
    state.apply(&register(2 * sessions + 1, vec![65_536]))?;
    Ok(())
}

#[test]
fn test_a_chunk_or_a_commit_is_no_command_of_the_state() -> TestResult {
    let mut state = MetadataState::new();
    let chunk = MetadataCommand::Chunk {
        offset: 0,
        bytes: vec![1],
    };
    let commit = MetadataCommand::Commit {
        length: 1,
        checksum: 2,
    };
    for framing in [chunk, commit] {
        assert_eq!(
            refused(&mut state, &framing)?,
            ClusterError::contradicts("a whole command, not a chunk of one or its commit").0
        );
    }
    Ok(())
}
