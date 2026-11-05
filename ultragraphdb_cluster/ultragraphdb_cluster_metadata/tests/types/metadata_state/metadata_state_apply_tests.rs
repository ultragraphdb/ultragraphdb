/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataSnapshot, MetadataState};
use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId};
use ultragraphdb_core_ids::{Epoch, Tenant, TypePrefix};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;

/// Tenant `n`.
fn tenant(n: u64) -> Tenant {
    Tenant::new(n)
}

/// Offers `bytes` as the version of tenant `number` after `predecessor`.
fn offer(number: u64, predecessor: u64, bytes: &[u8]) -> MetadataCommand {
    MetadataCommand::OfferCatalog {
        tenant: tenant(number),
        predecessor,
        bytes: bytes.to_vec(),
    }
}

/// The close of `epoch` announcing `versions` and carrying `evictions`.
fn close(epoch: u64, versions: &[(u64, u64)], evictions: &[u128]) -> TestResult<MetadataCommand> {
    Ok(MetadataCommand::LearnClose {
        placement: 0,
        epoch: Epoch::new(epoch)?,
        versions: versions.iter().map(|(n, v)| (tenant(*n), *v)).collect(),
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

/// The refusal of a decision the state contradicts, naming the rule `what` it breaks.
fn contradicts(what: &'static str) -> ClusterErrorEnum {
    ClusterError::contradicts(what).0
}

#[test]
fn test_versions_chain_from_1_per_tenant() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&offer(1, 0, b"a1"))?;
    state.apply(&offer(1, 1, b"a2"))?;
    state.apply(&offer(2, 0, b"b1"))?;
    assert_eq!(state.current_version(tenant(1)), 2);
    assert_eq!(state.current_version(tenant(2)), 1);
    assert_eq!(
        state.pending_versions(),
        vec![(tenant(1), 1), (tenant(1), 2), (tenant(2), 1)]
    );
    assert_eq!(state.tenants(), vec![tenant(1), tenant(2)]);
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
    for tenant in 0..256 {
        state.apply(&offer(tenant, 0, b"v"))?;
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
fn test_a_tenant_retains_at_most_1024_versions() -> TestResult {
    let mut state = MetadataState::new();
    for version in 1..=1_024 {
        state.apply(&offer(1, version - 1, b"v"))?;
        state.apply(&close(version, &[(1, version)], &[])?)?;
    }
    assert_eq!(
        refused(&mut state, &offer(1, 1_024, b"v"))?,
        ClusterError::full("retained catalog versions of a tenant", 1_024).0
    );
    Ok(())
}

#[test]
fn test_a_close_assigns_the_epoch_after_it() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&offer(1, 0, b"a1"))?;
    state.apply(&close(38, &[(1, 1)], &[])?)?;
    assert_eq!(state.last_learned(), Epoch::new(38)?);
    let record = state.versions(tenant(1));
    let record = record.first().context("version 1")?;
    assert_eq!(record.effective(), Some(Epoch::new(39)?));
    assert!(state.pending_versions().is_empty());
    Ok(())
}

#[test]
fn test_closes_are_learned_in_order() -> TestResult {
    let mut state = MetadataState::new();
    // The first close learned may be any epoch from 1: no close is of epoch 0.
    assert_eq!(
        refused(&mut state, &close(0, &[], &[])?)?,
        ClusterError::epoch_ahead(0, 1).0
    );
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
        tenant: tenant(1),
        predicate: "score".to_string(),
        entry: b"first".to_vec(),
    };
    state.apply(&first)?;
    let again = MetadataCommand::RecordDefault {
        tenant: tenant(1),
        predicate: "score".to_string(),
        entry: b"second".to_vec(),
    };
    state.apply(&again)?;
    assert_eq!(state.default_entry(tenant(1), "score"), Some(&b"first"[..]));
    assert_eq!(state.default_entry(tenant(2), "score"), None);
    assert_eq!(state.default_entry(tenant(1), "scores"), None);
    assert_eq!(state.current_version(tenant(1)), 1);
    assert_eq!(state.pending_versions(), vec![(tenant(1), 1)]);
    Ok(())
}

#[test]
fn test_a_default_entry_past_its_limits_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    let long = MetadataCommand::RecordDefault {
        tenant: tenant(1),
        predicate: "p".repeat(4_097),
        entry: Vec::new(),
    };
    assert_eq!(
        refused(&mut state, &long)?,
        ClusterError::too_many("bytes of a predicate's name", 4_097, 4_096).0
    );
    let big = MetadataCommand::RecordDefault {
        tenant: tenant(1),
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
    assert_eq!(
        refused(&mut state, &lease(1_002, 2_000))?,
        contradicts("a block starts at its type's high-water mark")
    );
    assert_eq!(
        refused(&mut state, &lease(1, 1_001))?,
        contradicts("a block starts at its type's high-water mark")
    );
    assert_eq!(
        refused(&mut state, &lease(1_001, 1_001))?,
        contradicts("a block holds a sequence")
    );
    assert_eq!(
        refused(&mut state, &lease(1_001, 1_000))?,
        contradicts("a block holds a sequence")
    );
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
    // Pending until a close announces it: the close of 2 gives it epoch 3.
    assert_eq!(state.pending_placement(), 1);
    state.apply(&placing(2, 1)?)?;
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
    let change = MetadataCommand::ChangePlacement { assignments };
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
    assert_eq!(
        refused(&mut state, &create(3, "bob", false))?,
        contradicts("a principal takes the next identity")
    );
    assert_eq!(
        refused(&mut state, &create(1, "bob", false))?,
        contradicts("a principal takes the next identity")
    );
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
        evicts: Vec::new(),
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
    assert_eq!(
        refused(&mut state, &create(1, "bob", false))?,
        contradicts("a principal takes the next identity")
    );
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
    assert_eq!(
        refused(&mut state, &register(30, 1, &[20]))?,
        contradicts("a registration evicts the oldest sessions")
    );
    assert_eq!(
        refused(&mut state, &register(30, 1, &[10, 99]))?,
        contradicts("a registration evicts the oldest sessions")
    );
    // More evictions than sessions held.
    assert_eq!(
        refused(&mut state, &register(30, 1, &[10, 20, 99]))?,
        contradicts("a registration evicts the oldest sessions")
    );
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
    // Versions 1, 2 and 3 of tenant 1 take effect at 11, 21 and 31; version 1 of tenant 2 at 21.
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
    assert_eq!(state.versions(tenant(1)).len(), 3);
    // Both logs begin at 25 or later: version 2 is in effect there, so version 1 is released.
    state.apply(&report(2, 26)?)?;
    let kept: Vec<u64> = state
        .versions(tenant(1))
        .iter()
        .map(|v| v.version())
        .collect();
    assert_eq!(kept, vec![2, 3]);
    assert_eq!(state.versions(tenant(2)).len(), 1);
    assert_eq!(
        state
            .catalog_at(tenant(1), Epoch::new(15)?)
            .err()
            .context("released")?
            .0,
        ClusterError::catalog_released(1, 15).0
    );
    assert_eq!(
        state
            .catalog_at(tenant(1), Epoch::new(25)?)?
            .map(|v| v.version()),
        Some(2)
    );
    // An epoch before tenant 2's first version reads its initial catalog: nothing of it was released.
    assert_eq!(state.catalog_at(tenant(2), Epoch::new(15)?)?, None);
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

#[test]
fn test_at_most_65536_principals_are_held() -> TestResult {
    let mut state = MetadataState::new();
    let held = u64::try_from(MetadataState::PRINCIPALS_MAX)?;
    for id in 1..=held {
        state.apply(&create(id, &format!("p{id}"), id == 1))?;
    }
    assert_eq!(state.principals(), MetadataState::PRINCIPALS_MAX);
    assert_eq!(
        refused(&mut state, &create(held + 1, "one-more", false))?,
        ClusterError::full("principals", 65_536).0
    );
    // A name held is refused as existing before the table is found full.
    assert_eq!(
        refused(&mut state, &create(held + 1, "p2", false))?,
        ClusterError::principal_exists("p2").0
    );
    // Removing one makes room again.
    state.apply(&MetadataCommand::RemovePrincipal {
        name: "p2".to_string(),
        evicts: Vec::new(),
    })?;
    state.apply(&create(held + 1, "one-more", false))?;
    Ok(())
}

#[test]
fn test_a_registration_evicting_more_than_a_close_carries_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&create(1, "ann", false))?;
    let register = |session, evicts: Vec<u128>| MetadataCommand::RegisterSession {
        session,
        principal: 1,
        evicts,
    };
    for session in 0..4_097_u128 {
        state.apply(&register(session, Vec::new()))?;
    }
    assert_eq!(
        refused(&mut state, &register(5_000, (0..4_097).collect()))?,
        ClusterError::too_many("session evictions", 4_097, 4_096).0
    );
    state.apply(&register(5_000, (0..4_096).collect()))?;
    assert_eq!(state.sessions(), 2);
    Ok(())
}

/// The state after a snapshot of it at position 9 is encoded and decoded.
fn snapshot_round_trip(state: &MetadataState) -> TestResult<MetadataState> {
    let mut bytes = Vec::new();
    MetadataSnapshot::new(Position::new(9), state.clone()).encode(&mut bytes);
    Ok(MetadataSnapshot::decode(&bytes)?.into_state())
}

#[test]
fn test_a_close_skipping_a_pending_version_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&offer(1, 0, b"a1"))?;
    state.apply(&offer(1, 1, b"a2"))?;
    state.apply(&offer(2, 0, b"b1"))?;
    // Version 2 announced while version 1 stays pending: refused, the state unchanged.
    assert_eq!(
        refused(&mut state, &close(1, &[(1, 2)], &[])?)?,
        contradicts("a close announces a tenant's versions in order")
    );
    assert_eq!(
        refused(&mut state, &close(1, &[(2, 1), (1, 2)], &[])?)?,
        contradicts("a close announces a tenant's versions in order")
    );
    assert_eq!(
        state.pending_versions(),
        vec![(tenant(1), 1), (tenant(1), 2), (tenant(2), 1)]
    );
    // Another tenant's pending version does not hold this one back.
    state.apply(&close(1, &[(2, 1)], &[])?)?;
    assert_eq!(snapshot_round_trip(&state)?, state);
    Ok(())
}

#[test]
fn test_versions_announced_in_order_take_effect_and_round_trip() -> TestResult {
    // v1 in one close, v2 in the next.
    let mut apart = MetadataState::new();
    apart.apply(&offer(1, 0, b"a1"))?;
    apart.apply(&offer(1, 1, b"a2"))?;
    apart.apply(&close(1, &[(1, 1)], &[])?)?;
    assert_eq!(snapshot_round_trip(&apart)?, apart);
    apart.apply(&close(2, &[(1, 2)], &[])?)?;
    let effective = |state: &MetadataState| -> Vec<Option<Epoch>> {
        state
            .versions(tenant(1))
            .iter()
            .map(|v| v.effective())
            .collect()
    };
    assert_eq!(
        effective(&apart),
        vec![Some(Epoch::new(2)?), Some(Epoch::new(3)?)]
    );
    assert_eq!(snapshot_round_trip(&apart)?, apart);
    // v1 and v2 in one close, in either order within it.
    for versions in [[(1, 1), (1, 2)], [(1, 2), (1, 1)]] {
        let mut together = MetadataState::new();
        together.apply(&offer(1, 0, b"a1"))?;
        together.apply(&offer(1, 1, b"a2"))?;
        together.apply(&close(1, &versions, &[])?)?;
        assert_eq!(
            effective(&together),
            vec![Some(Epoch::new(2)?), Some(Epoch::new(2)?)]
        );
        assert_eq!(together.pending_versions(), vec![]);
        assert_eq!(snapshot_round_trip(&together)?, together);
    }
    Ok(())
}

/// A change of the whole bucket space to `partition`.
fn change_to(partition: u32) -> TestResult<MetadataCommand> {
    Ok(MetadataCommand::ChangePlacement {
        assignments: vec![(
            BucketRange::new(0, 16_777_216)?,
            PartitionId::new(partition)?,
        )],
    })
}

/// The close of `epoch` announcing placement version `placement` and nothing else.
fn placing(epoch: u64, placement: u64) -> TestResult<MetadataCommand> {
    Ok(MetadataCommand::LearnClose {
        epoch: Epoch::new(epoch)?,
        versions: Vec::new(),
        placement,
        evictions: Vec::new(),
    })
}

/// A state with the map split evenly over partitions 1 and 2, its close of 5 learned.
fn placed_at_5() -> TestResult<MetadataState> {
    let mut state = MetadataState::new();
    state.apply(&MetadataCommand::InitPlacement {
        partitions: 2,
        leader: NodeId::new(4),
    })?;
    state.apply(&close(5, &[], &[])?)?;
    Ok(state)
}

/// The partitions the map gives the whole bucket space at `epoch`.
fn partitions_at(state: &MetadataState, epoch: u64) -> TestResult<Vec<u32>> {
    let map = state.placement().context("the map")?;
    Ok(map
        .ranges_at(Epoch::new(epoch)?)
        .iter()
        .map(|(_, partition)| partition.get())
        .collect())
}

#[test]
fn test_a_placement_change_is_pending_until_a_close_announces_it() -> TestResult {
    let mut state = placed_at_5()?;
    let even = partitions_at(&state, 6)?;
    assert!(even.contains(&1) && even.contains(&2), "{even:?}");
    assert_eq!(state.pending_placement(), 0);
    state.apply(&change_to(1)?)?;
    // Committed, numbered 1 above the initial map, and in effect at no epoch yet.
    assert_eq!(state.pending_placement(), 1);
    assert_eq!(
        state.placement().context("the map")?.latest_change(),
        Epoch::ZERO
    );
    assert_eq!(snapshot_round_trip(&state)?, state);
    // A close announcing none leaves it pending.
    state.apply(&placing(6, 0)?)?;
    assert_eq!(state.pending_placement(), 1);
    // The close of 7 announces it: it applies from 8, and the placement of 7, opened already, keeps the even map.
    state.apply(&placing(7, 1)?)?;
    assert_eq!(state.pending_placement(), 0);
    assert_eq!(
        state.placement().context("the map")?.latest_change(),
        Epoch::new(8)?
    );
    assert_eq!(partitions_at(&state, 7)?, even);
    assert_eq!(partitions_at(&state, 8)?, vec![1]);
    assert_eq!(snapshot_round_trip(&state)?, state);
    Ok(())
}

#[test]
fn test_each_close_announces_one_placement_version_in_order() -> TestResult {
    let mut state = placed_at_5()?;
    state.apply(&change_to(1)?)?;
    state.apply(&change_to(2)?)?;
    state.apply(&change_to(1)?)?;
    assert_eq!(state.pending_placement(), 1);
    assert_eq!(snapshot_round_trip(&state)?, state);
    // Any version but the oldest pending is refused, changing nothing: a later one, an unknown one, the initial map's
    // number is none at all.
    for announced in [2, 3, 4, u64::MAX] {
        assert_eq!(
            refused(&mut state, &placing(6, announced)?)?,
            contradicts("a close announces the oldest pending placement version")
        );
    }
    for (closed, announced, applies) in [(6, 1, 7), (7, 2, 8), (8, 3, 9)] {
        state.apply(&placing(closed, announced)?)?;
        assert_eq!(
            state.placement().context("the map")?.latest_change(),
            Epoch::new(applies)?
        );
    }
    assert_eq!(partitions_at(&state, 7)?, vec![1]);
    assert_eq!(partitions_at(&state, 8)?, vec![2]);
    assert_eq!(partitions_at(&state, 9)?, vec![1]);
    // None is pending now, so a close announcing one is refused.
    assert_eq!(state.pending_placement(), 0);
    assert_eq!(
        refused(&mut state, &placing(9, 4)?)?,
        contradicts("a close announces the oldest pending placement version")
    );
    assert_eq!(snapshot_round_trip(&state)?, state);
    Ok(())
}

#[test]
fn test_a_refused_placement_announcement_learns_nothing_of_the_close() -> TestResult {
    let mut state = placed_at_5()?;
    state.apply(&offer(1, 0, b"a1"))?;
    state.apply(&change_to(1)?)?;
    state.apply(&change_to(2)?)?;
    // The close announcing the catalog version and placement version 2 is refused whole: the catalog version stays
    // pending and the close of 6 is not learned.
    let refusal = refused(
        &mut state,
        &MetadataCommand::LearnClose {
            epoch: Epoch::new(6)?,
            versions: vec![(tenant(1), 1)],
            placement: 2,
            evictions: Vec::new(),
        },
    )?;
    assert_eq!(
        refusal,
        contradicts("a close announces the oldest pending placement version")
    );
    assert_eq!(state.last_learned(), Epoch::new(5)?);
    assert_eq!(state.pending_versions(), vec![(tenant(1), 1)]);
    Ok(())
}

#[test]
fn test_a_placement_change_is_checked_when_committed() -> TestResult {
    let mut unplaced = MetadataState::new();
    assert_eq!(
        refused(&mut unplaced, &change_to(1)?)?,
        ClusterError::no_placement().0
    );
    let mut state = placed_at_5()?;
    let gap = MetadataCommand::ChangePlacement {
        assignments: vec![(BucketRange::new(1, 16_777_216)?, PartitionId::new(1)?)],
    };
    assert_eq!(refused(&mut state, &gap)?, ClusterError::unassigned(0).0);
    let overlap = MetadataCommand::ChangePlacement {
        assignments: vec![
            (BucketRange::new(0, 200)?, PartitionId::new(1)?),
            (BucketRange::new(100, 16_777_216)?, PartitionId::new(2)?),
        ],
    };
    assert_eq!(
        refused(&mut state, &overlap)?,
        ClusterError::assigned_twice(100).0
    );
    assert_eq!(state.pending_placement(), 0);
    Ok(())
}

#[test]
fn test_at_most_16_placement_versions_are_pending() -> TestResult {
    let mut state = placed_at_5()?;
    for version in 1..=16 {
        state.apply(&change_to(1)?)?;
        assert_eq!(state.pending_placement(), 1, "after version {version}");
    }
    assert_eq!(
        refused(&mut state, &change_to(2)?)?,
        ClusterError::full("pending placement versions", 16).0
    );
    assert_eq!(snapshot_round_trip(&state)?, state);
    // One close takes one: room for one more.
    state.apply(&placing(6, 1)?)?;
    state.apply(&change_to(2)?)?;
    assert_eq!(state.pending_placement(), 2);
    Ok(())
}

#[test]
fn test_pending_placement_versions_keep_room_in_the_map() -> TestResult {
    // The map holds 256 layouts: the first and 255 changes. Announce 245, then 10 more fit pending, and no 11th.
    let mut state = placed_at_5()?;
    for (i, closed) in (6..6 + 245).enumerate() {
        state.apply(&change_to(1 + (i % 2) as u32)?)?;
        state.apply(&placing(closed, i as u64 + 1)?)?;
    }
    assert_eq!(state.placement().context("the map")?.layouts_free(), 10);
    for _ in 0..10 {
        state.apply(&change_to(1)?)?;
    }
    assert_eq!(
        refused(&mut state, &change_to(2)?)?,
        ClusterError::full("placement layouts", 256).0
    );
    // Every one pending is announced without the map refusing it.
    for (i, closed) in (251..261).enumerate() {
        state.apply(&placing(closed, 246 + i as u64)?)?;
    }
    assert_eq!(state.placement().context("the map")?.layouts_free(), 0);
    assert_eq!(state.pending_placement(), 0);
    Ok(())
}

/// A registration of `session` for `principal`, evicting nothing.
fn register_for(session: u128, principal: u64) -> MetadataCommand {
    MetadataCommand::RegisterSession {
        session,
        principal,
        evicts: Vec::new(),
    }
}

/// A removal of `name` evicting `evicts`.
fn removal(name: &str, evicts: &[u128]) -> MetadataCommand {
    MetadataCommand::RemovePrincipal {
        name: name.to_string(),
        evicts: evicts.to_vec(),
    }
}

#[test]
fn test_a_removal_evicts_every_session_of_its_principal() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&create(1, "root", true))?;
    state.apply(&create(2, "ann", false))?;
    state.apply(&create(3, "bob", false))?;
    state.apply(&create(4, "cid", false))?;
    // ann holds 10, 30 and 40; bob holds 20; cid holds none; root holds 50.
    for (session, principal) in [(10, 2), (20, 3), (30, 2), (40, 2), (50, 1)] {
        state.apply(&register_for(session, principal))?;
    }
    // The evictions named must be the principal's sessions, every one, in registration order.
    let rule = contradicts("a removal evicts the principal's sessions");
    assert_eq!(refused(&mut state, &removal("ann", &[10, 30]))?, rule);
    assert_eq!(refused(&mut state, &removal("ann", &[30, 10, 40]))?, rule);
    assert_eq!(
        refused(&mut state, &removal("ann", &[10, 20, 30, 40]))?,
        rule
    );
    assert_eq!(refused(&mut state, &removal("cid", &[20]))?, rule);
    // None.
    state.apply(&removal("cid", &[]))?;
    assert_eq!(state.pending_evictions(), Vec::<u128>::new());
    // One.
    state.apply(&removal("bob", &[20]))?;
    assert_eq!(state.pending_evictions(), vec![20]);
    // Several; the other principals' sessions stay.
    state.apply(&removal("ann", &[10, 30, 40]))?;
    assert_eq!(state.pending_evictions(), vec![20, 10, 30, 40]);
    assert_eq!(state.sessions(), 1);
    assert_eq!(state.session_principal(50)?, 1);
    for session in [10, 20, 30, 40] {
        assert_eq!(
            state.session_principal(session).err().context("evicted")?,
            ClusterError::session_evicted(session)
        );
    }
    assert_eq!(snapshot_round_trip(&state)?, state);
    // The next close carries them away.
    state.apply(&close(1, &[], &[20, 10, 30, 40])?)?;
    assert_eq!(state.pending_evictions(), Vec::<u128>::new());
    Ok(())
}

#[test]
fn test_a_refused_removal_leaves_the_sessions_registered() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&create(1, "root", true))?;
    state.apply(&register_for(10, 1))?;
    assert_eq!(
        refused(&mut state, &removal("root", &[10]))?,
        ClusterError::last_administrator("root").0
    );
    assert_eq!(state.session_principal(10)?, 1);
    assert_eq!(state.pending_evictions(), Vec::<u128>::new());
    Ok(())
}

#[test]
fn test_a_removal_past_the_evictions_awaiting_a_close_is_refused() -> TestResult {
    let mut state = MetadataState::new();
    state.apply(&create(1, "root", true))?;
    state.apply(&create(2, "ann", false))?;
    let sessions: u128 = 16 * 4_096;
    for session in 0..=sessions {
        state.apply(&register_for(session, 1))?;
    }
    // Sixteen registrations of root's, each evicting the 4,096 oldest, fill the evictions awaiting a close.
    for batch in 0..16_u128 {
        let evicts: Vec<u128> = (batch * 4_096..(batch + 1) * 4_096).collect();
        state.apply(&MetadataCommand::RegisterSession {
            session: sessions + 1 + batch,
            principal: 1,
            evicts,
        })?;
    }
    state.apply(&register_for(1_000_000, 2))?;
    assert_eq!(
        refused(&mut state, &removal("ann", &[1_000_000]))?,
        ClusterError::full("session evictions awaiting a close", 65_536).0
    );
    Ok(())
}
