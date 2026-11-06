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

/// The snapshot's encoding.
fn encoded(snapshot: &MetadataSnapshot) -> Vec<u8> {
    let mut out = Vec::new();
    snapshot.encode(&mut out);
    out
}

/// The refusal of decoding `bytes`.
fn refusal(bytes: &[u8]) -> TestResult<ClusterErrorEnum> {
    Ok(MetadataSnapshot::decode(bytes)
        .err()
        .context("a refusal")?
        .0)
}

/// The small state of the hand-written encoding: tenant 2's version 1, "ab", effective at 6 after the learned close
/// of 5, and the `Resource` type's high-water mark at 11; three commands, so at position 3.
fn small() -> TestResult<MetadataSnapshot> {
    let mut state = MetadataState::new();
    state.apply(&MetadataCommand::OfferCatalog {
        tenant: Tenant::new(2),
        predecessor: 0,
        bytes: b"ab".to_vec(),
    })?;
    state.apply(&MetadataCommand::LearnClose {
        placement: 0,
        epoch: Epoch::new(5)?,
        versions: vec![(Tenant::new(2), 1)],
        evictions: Vec::new(),
    })?;
    state.apply(&MetadataCommand::LeaseBlock {
        prefix: TypePrefix::RESOURCE,
        partition: PartitionId::new(1)?,
        start: 1,
        end: 11,
    })?;
    Ok(MetadataSnapshot::new(Position::new(3), state))
}

/// The hand-written encoding of [`small`], from the format's documentation.
fn small_bytes() -> Vec<u8> {
    let mut out = vec![1];
    out.extend_from_slice(&3_u64.to_be_bytes()); // position
    out.extend_from_slice(&5_u64.to_be_bytes()); // last learned close
    out.extend_from_slice(&1_u32.to_be_bytes()); // one catalog
    out.extend_from_slice(&2_u64.to_be_bytes()); // tenant 2
    out.extend_from_slice(&1_u64.to_be_bytes()); // latest version
    out.extend_from_slice(&0_u64.to_be_bytes()); // none released
    out.extend_from_slice(&1_u32.to_be_bytes()); // one version retained
    out.extend_from_slice(&1_u64.to_be_bytes()); // version 1
    out.push(1); // effective
    out.extend_from_slice(&6_u64.to_be_bytes()); // at epoch 6
    out.extend_from_slice(&2_u32.to_be_bytes()); // its bytes' length
    out.extend_from_slice(b"ab");
    out.extend_from_slice(&0_u32.to_be_bytes()); // no default entry
    out.extend_from_slice(&1_u32.to_be_bytes()); // one high-water mark
    out.extend_from_slice(&1_u16.to_be_bytes()); // Resource
    out.extend_from_slice(&11_u64.to_be_bytes()); // next sequence 11
    out.push(0); // no placement map
    out.extend_from_slice(&0_u32.to_be_bytes()); // no report
    out.extend_from_slice(&1_u64.to_be_bytes()); // the next principal is 1
    out.extend_from_slice(&0_u32.to_be_bytes()); // no principal
    out.extend_from_slice(&0_u32.to_be_bytes()); // no session
    out.extend_from_slice(&0_u32.to_be_bytes()); // no eviction pending
    out.extend_from_slice(&0_u64.to_be_bytes()); // the initial map's placement version
    out.extend_from_slice(&0_u32.to_be_bytes()); // no placement version pending
    out.extend_from_slice(&0_u32.to_be_bytes()); // no tenant
    out.push(0); // no unknown-name secret
    out
}

/// The catalog commands of [`full`]: tenants 1 and 3, tenant 1 with an effective, a second effective and a pending
/// version, and the partitions' reports.
fn full_catalogs() -> TestResult<Vec<MetadataCommand>> {
    let tenant = Tenant::new;
    let offer = |number, predecessor, bytes: &[u8]| MetadataCommand::OfferCatalog {
        tenant: tenant(number),
        predecessor,
        bytes: bytes.to_vec(),
    };
    let close = |epoch, versions: Vec<(Tenant, u64)>| -> TestResult<MetadataCommand> {
        Ok(MetadataCommand::LearnClose {
            placement: 0,
            epoch: Epoch::new(epoch)?,
            versions,
            evictions: Vec::new(),
        })
    };
    let report = |partition, epoch| -> TestResult<MetadataCommand> {
        Ok(MetadataCommand::ReportUntrimmed {
            partition: PartitionId::new(partition)?,
            epoch: Epoch::new(epoch)?,
        })
    };
    Ok(vec![
        MetadataCommand::InitPlacement {
            partitions: 2,
            leader: NodeId::new(9),
        },
        offer(1, 0, &[1; 3]),
        offer(3, 0, &[3; 3]),
        close(1, vec![(tenant(1), 1), (tenant(3), 1)])?,
        offer(1, 1, b"second"),
        close(2, vec![(tenant(1), 2)])?,
        offer(1, 2, b""),
        report(1, 3)?,
        report(2, 4)?,
    ])
}

/// The other commands of [`full`]: default entries, two high-water marks, a placement change, two principals and three
/// sessions, the third evicting the first.
fn full_rest() -> TestResult<Vec<MetadataCommand>> {
    let mut commands: Vec<MetadataCommand> = [(1, "b"), (1, "a"), (0, "z")]
        .into_iter()
        .map(|(tenant, predicate)| MetadataCommand::RecordDefault {
            tenant: Tenant::new(tenant),
            predicate: predicate.to_string(),
            entry: predicate.as_bytes().to_vec(),
        })
        .collect();
    for (prefix, end) in [(2, 5), (1, 9)] {
        commands.push(MetadataCommand::LeaseBlock {
            prefix: TypePrefix::new(prefix)?,
            partition: PartitionId::new(1)?,
            start: 1,
            end,
        });
    }
    commands.push(MetadataCommand::ChangePlacement {
        assignments: vec![(BucketRange::new(0, 16_777_216)?, PartitionId::new(2)?)],
    });
    for (id, name, administrator) in [(1_u8, "zed", true), (2, "amy", false)] {
        commands.push(MetadataCommand::CreatePrincipal {
            id: u64::from(id),
            name: name.to_string(),
            verifier: ScramVerifier::new(vec![id; 16], 4_096, [id; 32], [7; 32]),
            administrator,
        });
    }
    for (session, evicts) in [(30, vec![]), (10, vec![]), (20, vec![30])] {
        commands.push(MetadataCommand::RegisterSession {
            session,
            principal: 2,
            evicts,
        });
    }
    Ok(commands)
}

/// A state with every table filled: catalogs with effective and pending versions, default entries, two high-water
/// marks, a changed placement map, reports, principals, sessions and pending evictions.
fn full() -> TestResult<MetadataSnapshot> {
    let mut state = MetadataState::new();
    for command in full_catalogs()?.into_iter().chain(full_rest()?) {
        state.apply(&command)?;
    }
    Ok(MetadataSnapshot::new(Position::new(25), state))
}

#[test]
fn test_a_small_state_encodes_as_written_by_hand() -> TestResult {
    let snapshot = small()?;
    assert_eq!(encoded(&snapshot), small_bytes());
    assert_eq!(MetadataSnapshot::decode(&small_bytes())?, snapshot);
    Ok(())
}

#[test]
fn test_an_empty_state_round_trips() -> TestResult {
    let snapshot = MetadataSnapshot::new(Position::ZERO, MetadataState::new());
    let bytes = encoded(&snapshot);
    // Format, position, learned, 4 empty tables, no map, no report, next principal, 3 empty tables, the last placement
    // version and none pending.
    assert_eq!(
        bytes.len(),
        1 + 8 + 8 + 4 * 3 + 1 + 4 + 8 + 4 * 3 + 8 + 4 + 4 + 1
    );
    assert_eq!(MetadataSnapshot::decode(&bytes)?, snapshot);
    Ok(())
}

#[test]
fn test_a_full_state_round_trips() -> TestResult {
    let snapshot = full()?;
    let bytes = encoded(&snapshot);
    let decoded = MetadataSnapshot::decode(&bytes)?;
    assert_eq!(decoded, snapshot);
    assert_eq!(encoded(&decoded), bytes);
    // Spot checks that the tables were filled at all.
    let state = decoded.state();
    assert_eq!(state.default_entry(Tenant::new(0), "z"), Some(&b"z"[..]));
    assert_eq!(state.session_principal(20)?, 2);
    assert_eq!(state.pending_evictions(), vec![30]);
    assert_eq!(state.next_sequence(TypePrefix::STATEMENT), 5);
    Ok(())
}

#[test]
fn test_encoding_appends_to_the_buffer() -> TestResult {
    let mut out = vec![0xAA];
    small()?.encode(&mut out);
    let mut expected = vec![0xAA];
    expected.extend_from_slice(&small_bytes());
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn test_a_format_byte_other_than_1_is_refused() -> TestResult {
    let mut bytes = small_bytes();
    if let Some(byte) = bytes.first_mut() {
        *byte = 2;
    }
    assert_eq!(
        refusal(&bytes)?,
        ClusterError::malformed("the format byte 1", 0).0
    );
    assert_eq!(
        refusal(&[])?,
        ClusterError::malformed("the format byte 1", 0).0
    );
    Ok(())
}

#[test]
fn test_every_truncation_is_refused() -> TestResult {
    for snapshot in [small()?, full()?] {
        let bytes = encoded(&snapshot);
        for cut in 0..bytes.len() {
            let refused = refusal(bytes.get(..cut).context("a prefix")?)?;
            assert!(
                matches!(refused, ClusterErrorEnum::Malformed { offset, .. } if offset <= cut),
                "cut at {cut}: {refused:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_bytes_after_the_end_are_refused() -> TestResult {
    let mut bytes = small_bytes();
    let end = bytes.len();
    bytes.push(0);
    assert_eq!(
        refusal(&bytes)?,
        ClusterError::malformed("the end of the snapshot", end).0
    );
    Ok(())
}

#[test]
fn test_values_out_of_order_or_range_are_refused() -> TestResult {
    // The version's flag (offset 57) set to 2.
    let mut flag = small_bytes();
    if let Some(byte) = flag.get_mut(57) {
        *byte = 2;
    }
    assert_eq!(
        refusal(&flag)?,
        ClusterError::malformed("a flag, 0 or 1", 57).0
    );
    // The latest version (offset 29) named 2 while the last retained is 1.
    let mut latest = small_bytes();
    if let Some(byte) = latest.get_mut(36) {
        *byte = 2;
    }
    assert_eq!(
        refusal(&latest)?,
        ClusterError::malformed("a tenant's latest version, its last retained", 29).0
    );
    // The high-water mark's prefix (offset 80) as 0.
    let mut prefix = small_bytes();
    if let Some(byte) = prefix.get_mut(81) {
        *byte = 0;
    }
    assert_eq!(
        refusal(&prefix)?,
        ClusterError::malformed("a type prefix from 1", 80).0
    );
    // The learned close (offset 9) past 42 bits.
    let mut learned = small_bytes();
    if let Some(byte) = learned.get_mut(9) {
        *byte = 0xFF;
    }
    assert_eq!(
        refusal(&learned)?,
        ClusterError::malformed("an epoch of 42 bits", 9).0
    );
    Ok(())
}

#[test]
fn test_counts_past_their_limits_are_refused_before_their_values() -> TestResult {
    // 1,025 versions retained by one tenant.
    let mut versions = small_bytes();
    versions.truncate(45);
    versions.extend_from_slice(&1_025_u32.to_be_bytes());
    assert_eq!(
        refusal(&versions)?,
        ClusterError::too_many("retained catalog versions of a tenant", 1_025, 1_024).0
    );
    Ok(())
}

/// A snapshot's bytes up to its first table: the format byte, position 9, the learned close 4, then `rest`.
fn snapshot_with(rest: &[&[u8]]) -> Vec<u8> {
    let mut out = vec![1];
    out.extend_from_slice(&9_u64.to_be_bytes());
    out.extend_from_slice(&4_u64.to_be_bytes());
    for part in rest {
        out.extend_from_slice(part);
    }
    out
}

#[test]
fn test_every_tables_count_past_its_limit_is_refused_before_its_values() -> TestResult {
    let zero = 0_u32.to_be_bytes();
    let one = 1_u32.to_be_bytes();
    let tenant = 1_u64.to_be_bytes();
    let cases: [(Vec<u8>, ClusterError); 7] = [
        (
            snapshot_with(&[&1_048_577_u32.to_be_bytes()]),
            ClusterError::too_many("catalogs", 1_048_577, 1_048_576),
        ),
        // One catalog of one pending version, version 1 above the released 0, of 523,000 bytes and one.
        (
            snapshot_with(&[
                &one,
                &tenant,
                &1_u64.to_be_bytes(),
                &0_u64.to_be_bytes(),
                &one,
                &1_u64.to_be_bytes(),
                &[0],
                &523_001_u32.to_be_bytes(),
            ]),
            ClusterError::too_many("bytes of a catalog version", 523_001, 523_000),
        ),
        (
            snapshot_with(&[&zero, &1_048_577_u32.to_be_bytes()]),
            ClusterError::too_many("default-entry records", 1_048_577, 1_048_576),
        ),
        (
            snapshot_with(&[&zero, &one, &tenant, &4_097_u16.to_be_bytes()]),
            ClusterError::too_many("bytes of a predicate's name", 4_097, 4_096),
        ),
        (
            snapshot_with(&[
                &zero,
                &one,
                &tenant,
                &1_u16.to_be_bytes(),
                b"p",
                &65_537_u32.to_be_bytes(),
            ]),
            ClusterError::too_many("bytes of a default entry", 65_537, 65_536),
        ),
        (
            snapshot_with(&[&zero, &zero, &65_537_u32.to_be_bytes()]),
            ClusterError::too_many("high-water marks", 65_537, 65_536),
        ),
        (
            snapshot_with(&[&zero, &zero, &zero, &[1], &2_147_483_649_u32.to_be_bytes()]),
            ClusterError::too_many("bytes of a placement map", 2_147_483_649, 2_147_483_648),
        ),
    ];
    for (bytes, expected) in cases {
        assert_eq!(refusal(&bytes)?, expected.0);
    }
    Ok(())
}

#[test]
fn test_the_access_tables_counts_past_their_limits_are_refused_before_their_values() -> TestResult {
    let zero = 0_u32.to_be_bytes();
    let cases: [(Vec<u8>, ClusterError); 4] = [
        (
            snapshot_with(&[&zero, &zero, &zero, &[0], &1_025_u32.to_be_bytes()]),
            ClusterError::too_many("partitions reporting", 1_025, 1_024),
        ),
        (
            snapshot_with(&[
                &zero,
                &zero,
                &zero,
                &[0],
                &zero,
                &1_u64.to_be_bytes(),
                &65_537_u32.to_be_bytes(),
            ]),
            ClusterError::too_many("principals", 65_537, 65_536),
        ),
        (
            snapshot_with(&[
                &zero,
                &zero,
                &zero,
                &[0],
                &zero,
                &1_u64.to_be_bytes(),
                &zero,
                &1_048_577_u32.to_be_bytes(),
            ]),
            ClusterError::too_many("sessions", 1_048_577, 1_048_576),
        ),
        (
            snapshot_with(&[
                &zero,
                &zero,
                &zero,
                &[0],
                &zero,
                &1_u64.to_be_bytes(),
                &zero,
                &zero,
                &65_537_u32.to_be_bytes(),
            ]),
            ClusterError::too_many("session evictions awaiting a close", 65_537, 65_536),
        ),
    ];
    for (bytes, expected) in cases {
        assert_eq!(refusal(&bytes)?, expected.0);
    }
    // At the limit, the count is taken and the values it counts are then read: here they are missing.
    let at_limit = snapshot_with(&[&zero, &zero, &zero, &[0], &1_024_u32.to_be_bytes()]);
    assert_eq!(
        refusal(&at_limit)?,
        ClusterError::malformed("a partition from 1", 34).0
    );
    Ok(())
}

#[test]
fn test_every_fuzz_seed_decodes_as_its_name_says() -> TestResult {
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("snapshot_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(seeds.len(), 4);
    for (name, bytes) in seeds {
        match MetadataSnapshot::decode(&bytes) {
            Ok(snapshot) => {
                assert!(!name.starts_with("malformed"), "{name} accepted");
                assert_eq!(encoded(&snapshot), bytes, "{name}");
            }
            Err(error) => assert!(name.starts_with("malformed"), "{name}: {error}"),
        }
    }
    Ok(())
}

#[test]
fn test_the_fuzz_seeds_are_the_encoders() -> TestResult {
    // The valid seeds are the encodings of the empty and the small state.
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("snapshot_decode");
    let mut seeds: Vec<Vec<u8>> = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?
        .into_iter()
        .filter(|(name, _)| !name.starts_with("malformed"))
        .map(|(_, bytes)| bytes)
        .collect();
    let mut expected = vec![
        encoded(&MetadataSnapshot::new(Position::ZERO, MetadataState::new())),
        small_bytes(),
    ];
    seeds.sort();
    expected.sort();
    assert_eq!(seeds, expected);
    Ok(())
}
