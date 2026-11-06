/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The snapshot decoder's checks of order and range, each over bytes written by hand from the format's documentation
//! with exactly one value out of place, and its neighbour in place accepted.

use ultragraphdb_cluster_metadata::MetadataSnapshot;
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// One retained version: its number, its effective epoch, its bytes.
type Version<'a> = (u64, Option<u64>, &'a [u8]);

/// One catalog: its tenant, latest and released versions, retained versions.
type Catalog<'a> = (u64, u64, u64, Vec<Version<'a>>);

/// One pending placement version: its number and its ranges (first bucket, end, partition).
type Pending = (u64, Vec<(u32, u32, u32)>);

/// The parts of a snapshot written by hand.
#[derive(Default)]
struct Raw<'a> {
    catalogs: Vec<Catalog<'a>>,
    defaults: Vec<(u64, &'a str)>,
    marks: Vec<(u16, u64)>,
    placement: Option<Vec<u8>>,
    reports: Vec<(u32, u64)>,
    next: u64,
    principals: Vec<(u64, &'a str)>,
    sessions: Vec<(u128, u64)>,
    /// The last placement version's number.
    placement_version: u64,
    /// The pending placement versions: each its number and ranges (first bucket, end, partition).
    placement_pending: Vec<Pending>,
}

impl Raw<'_> {
    /// The bytes, as `MetadataSnapshot::encode` documents them, at position 9 and the learned close 4.
    fn bytes(&self) -> Vec<u8> {
        let mut out = vec![1];
        out.extend_from_slice(&9_u64.to_be_bytes());
        out.extend_from_slice(&4_u64.to_be_bytes());
        self.catalogs_section(&mut out);
        put_count(&mut out, self.defaults.len());
        for (tenant, predicate) in &self.defaults {
            out.extend_from_slice(&tenant.to_be_bytes());
            out.extend_from_slice(
                &u16::try_from(predicate.len())
                    .unwrap_or_default()
                    .to_be_bytes(),
            );
            out.extend_from_slice(predicate.as_bytes());
            put_count(&mut out, 0);
        }
        put_count(&mut out, self.marks.len());
        for (prefix, next) in &self.marks {
            out.extend_from_slice(&prefix.to_be_bytes());
            out.extend_from_slice(&next.to_be_bytes());
        }
        match &self.placement {
            Some(map) => {
                out.push(1);
                put_count(&mut out, map.len());
                out.extend_from_slice(map);
            }
            None => out.push(0),
        }
        put_count(&mut out, self.reports.len());
        for (partition, epoch) in &self.reports {
            out.extend_from_slice(&partition.to_be_bytes());
            out.extend_from_slice(&epoch.to_be_bytes());
        }
        out.extend_from_slice(&self.next.to_be_bytes());
        put_count(&mut out, self.principals.len());
        for (id, name) in &self.principals {
            out.extend_from_slice(&id.to_be_bytes());
            out.push(u8::try_from(name.len()).unwrap_or_default());
            out.extend_from_slice(name.as_bytes());
            // A salt of one byte, 4,096 iterations, two zero keys, no administrator.
            out.extend_from_slice(&[1, 0, 0, 0, 0x10, 0]);
            out.extend_from_slice(&[0; 64]);
            out.push(0);
        }
        put_count(&mut out, self.sessions.len());
        for (session, principal) in &self.sessions {
            out.extend_from_slice(&session.to_be_bytes());
            out.extend_from_slice(&principal.to_be_bytes());
        }
        put_count(&mut out, 0);
        self.placement_tail(&mut out);
        put_count(&mut out, 0);
        out.push(0);
        out
    }

    /// Each tenant's catalog versions.
    fn catalogs_section(&self, out: &mut Vec<u8>) {
        put_count(out, self.catalogs.len());
        for (tenant, current, released, versions) in &self.catalogs {
            for value in [tenant, current, released] {
                out.extend_from_slice(&value.to_be_bytes());
            }
            put_count(out, versions.len());
            for (version, effective, data) in versions {
                out.extend_from_slice(&version.to_be_bytes());
                match effective {
                    Some(epoch) => {
                        out.push(1);
                        out.extend_from_slice(&epoch.to_be_bytes());
                    }
                    None => out.push(0),
                }
                put_count(out, data.len());
                out.extend_from_slice(data);
                out.push(0);
            }
        }
    }

    /// The last placement version's number and the pending placement versions.
    fn placement_tail(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.placement_version.to_be_bytes());
        put_count(out, self.placement_pending.len());
        for (version, ranges) in &self.placement_pending {
            out.extend_from_slice(&version.to_be_bytes());
            put_count(out, ranges.len());
            for (start, end, partition) in ranges {
                for value in [start, end, partition] {
                    out.extend_from_slice(&value.to_be_bytes());
                }
            }
        }
    }
}

fn put_count(out: &mut Vec<u8>, count: usize) {
    out.extend_from_slice(&u32::try_from(count).unwrap_or_default().to_be_bytes());
}

/// What decoding `raw` refuses with, or `None` when it is accepted.
fn refused(raw: &Raw<'_>) -> Option<ClusterErrorEnum> {
    MetadataSnapshot::decode(&raw.bytes()).err().map(|e| e.0)
}

/// Whether decoding `raw` is refused as malformed naming `what` at byte `offset`.
fn refuses(raw: &Raw<'_>, what: &'static str, offset: usize) -> bool {
    refused(raw) == Some(ClusterError::malformed(what, offset).0)
}

#[test]
fn test_tenants_rise() {
    let one = |tenant| (tenant, 1, 0, vec![(1, Some(2), &b""[..])]);
    let ok = Raw {
        catalogs: vec![one(1), one(2)],
        next: 1,
        ..Raw::default()
    };
    assert_eq!(refused(&ok), None);
    let twice = Raw {
        catalogs: vec![one(2), one(2)],
        next: 1,
        ..Raw::default()
    };
    assert!(refuses(&twice, "a tenant after the one before it", 71));
}

#[test]
fn test_the_first_retained_version_lies_above_the_released() {
    let released = |released, version| Raw {
        catalogs: vec![(1, version, released, vec![(version, Some(2), &b""[..])])],
        next: 1,
        ..Raw::default()
    };
    assert_eq!(refused(&released(1, 2)), None);
    assert!(refuses(
        &released(2, 2),
        "a version after the one before it",
        49
    ));
}

#[test]
fn test_versions_rise_and_their_epochs_never_fall() {
    let versions = |versions: Vec<Version<'static>>, current| Raw {
        catalogs: vec![(1, current, 0, versions)],
        next: 1,
        ..Raw::default()
    };
    // Two versions taking effect at one epoch are taken; a pending one may follow an effective one.
    let ok = versions(
        vec![(1, Some(3), b""), (2, Some(3), b""), (3, None, b"")],
        3,
    );
    assert_eq!(refused(&ok), None);
    let twice = versions(vec![(1, Some(3), b""), (1, Some(3), b"")], 1);
    assert!(refuses(&twice, "a version after the one before it", 71));
    let falling = versions(vec![(1, Some(3), b""), (2, Some(2), b"")], 2);
    assert!(refuses(
        &falling,
        "an effective epoch at or after the one before it",
        80
    ));
    let after_pending = versions(vec![(1, None, b""), (2, Some(2), b"")], 2);
    assert!(refuses(
        &after_pending,
        "an effective epoch at or after the one before it",
        72
    ));
    let latest = versions(vec![(1, Some(3), b"")], 2);
    assert!(refuses(
        &latest,
        "a tenant's latest version, its last retained",
        29
    ));
}

#[test]
fn test_default_entries_marks_and_reports_rise() {
    let defaults = |defaults| Raw {
        defaults,
        next: 1,
        ..Raw::default()
    };
    assert_eq!(refused(&defaults(vec![(1, "a"), (1, "b")])), None);
    let what = "a default entry after the one before it";
    assert!(refuses(&defaults(vec![(1, "a"), (1, "a")]), what, 40));
    let marks = |marks| Raw {
        marks,
        next: 1,
        ..Raw::default()
    };
    assert_eq!(refused(&marks(vec![(1, 5), (2, 5)])), None);
    assert!(refuses(
        &marks(vec![(2, 5), (2, 6)]),
        "a type prefix after the one before it",
        39
    ));
    let reports = |reports| Raw {
        reports,
        next: 1,
        ..Raw::default()
    };
    assert_eq!(refused(&reports(vec![(1, 5), (2, 5)])), None);
    assert!(refuses(
        &reports(vec![(2, 5), (2, 6)]),
        "a partition after the one before it",
        46
    ));
}

#[test]
fn test_principals_rise_by_name_below_the_next_identity() {
    let principals = |principals, next| Raw {
        principals,
        next,
        ..Raw::default()
    };
    assert_eq!(refused(&principals(vec![(2, "amy"), (1, "zed")], 3)), None);
    let what = "a principal after the one before it, below the next";
    assert!(refuses(
        &principals(vec![(1, "amy"), (2, "amy")], 3),
        what,
        129
    ));
    assert!(refuses(
        &principals(vec![(1, "amy"), (3, "zed")], 3),
        what,
        129
    ));
    assert!(refuses(
        &principals(vec![(1, "amy"), (1, "zed")], 3),
        what,
        129
    ));
    assert!(refuses(&principals(vec![(1, "")], 3), what, 46));
}

#[test]
fn test_sessions_are_registered_once() {
    let sessions = |sessions| Raw {
        sessions,
        next: 1,
        ..Raw::default()
    };
    assert_eq!(refused(&sessions(vec![(5, 1), (4, 1)])), None);
    assert!(refuses(
        &sessions(vec![(5, 1), (5, 2)]),
        "a session registered once",
        74
    ));
}

#[test]
fn test_a_placement_maps_refusal_is_named_at_its_offset_in_the_snapshot() -> TestResult {
    let mut map = Vec::new();
    PlacementMap::even(1, NodeId::new(1))?.encode(&mut map);
    let accepted = Raw {
        placement: Some(map.clone()),
        next: 1,
        ..Raw::default()
    };
    assert_eq!(refused(&accepted), None);
    // The map's layout count (its bytes 1 and 2) made 0, refused by the map at its offset 1.
    if let Some(byte) = map.get_mut(2) {
        *byte = 0;
    }
    let raw = Raw {
        placement: Some(map.clone()),
        next: 1,
        ..Raw::default()
    };
    let at = 1 + 8 + 8 + 4 + 4 + 4 + 1 + 4;
    let inner = PlacementMap::decode(&map)
        .err()
        .context("the map's refusal")?;
    let ClusterErrorEnum::Malformed { what, offset } = inner.0 else {
        return Err(ultragraphdb_tool_bdd::TestError::new("a malformed map"));
    };
    assert!(offset > 0);
    assert_eq!(
        refused(&raw),
        Some(ClusterError::malformed(what, at + offset).0)
    );
    Ok(())
}

/// The whole bucket space to partition 1.
const WHOLE: (u32, u32, u32) = (0, 1 << 24, 1);

/// A snapshot holding the last placement version `version` and `pending`, beside the map `map` when one is given.
fn placed(version: u64, pending: Vec<Pending>, map: Option<&[u8]>) -> Raw<'static> {
    Raw {
        placement: map.map(<[u8]>::to_vec),
        next: 1,
        placement_version: version,
        placement_pending: pending,
        ..Raw::default()
    }
}

/// The offset of the pending placement versions' count: after the head, the tables with a map of `map` bytes (or no
/// map), the access tables and the last placement version.
fn pending_count_at(map: Option<usize>) -> usize {
    1 + 8 + 8 + 4 + 4 + 4 + 1 + map.map_or(0, |len| 4 + len) + 4 + 8 + 4 + 4 + 4 + 8
}

/// The even map of one partition, encoded.
fn one_partition_map() -> TestResult<Vec<u8>> {
    let mut map = Vec::new();
    PlacementMap::even(1, NodeId::new(1))?.encode(&mut map);
    Ok(map)
}

#[test]
fn test_pending_placement_versions_are_held_beside_a_map() -> TestResult {
    let map = one_partition_map()?;
    assert_eq!(refused(&placed(0, vec![], Some(&map))), None);
    assert_eq!(refused(&placed(7, vec![], Some(&map))), None);
    let two = vec![(6, vec![WHOLE]), (7, vec![WHOLE])];
    assert_eq!(refused(&placed(7, two, Some(&map))), None);
    let split = vec![(1, vec![(0, 100, 1), (100, 1 << 24, 2)])];
    assert_eq!(refused(&placed(1, split, Some(&map))), None);
    // Without a map, no version is pending; the last version's number alone is held.
    assert_eq!(refused(&placed(3, vec![], None)), None);
    assert!(refuses(
        &placed(1, vec![(1, vec![WHOLE])], None),
        "no pending placement version without a placement map",
        pending_count_at(None)
    ));
    Ok(())
}

#[test]
fn test_pending_placement_versions_follow_one_another() -> TestResult {
    let map = one_partition_map()?;
    let first_at = pending_count_at(Some(map.len())) + 4;
    let second_at = first_at + 8 + 4 + 12;
    let order = "a placement version one above the one before, the last the last committed";
    // A gap, a repeat, a last other than the last committed, and a version 0.
    let cases = [
        (7, vec![(5, vec![WHOLE]), (7, vec![WHOLE])], first_at),
        (7, vec![(6, vec![WHOLE]), (6, vec![WHOLE])], second_at),
        (8, vec![(6, vec![WHOLE]), (7, vec![WHOLE])], first_at),
        (0, vec![(0, vec![WHOLE])], first_at),
    ];
    for (version, pending, at) in cases {
        assert!(refuses(&placed(version, pending, Some(&map)), order, at));
    }
    Ok(())
}

#[test]
fn test_pending_placement_versions_are_bounded_and_their_ranges_checked() -> TestResult {
    let map = one_partition_map()?;
    let ranges_at = pending_count_at(Some(map.len())) + 4 + 8 + 4;
    let seventeen: Vec<_> = (1..=17).map(|v| (v, vec![WHOLE])).collect();
    assert_eq!(
        refused(&placed(17, seventeen, Some(&map))),
        Some(ClusterErrorEnum::TooMany {
            what: "pending placement versions",
            count: 17,
            max: 16
        })
    );
    let past = vec![(1, vec![(0, (1 << 24) + 1, 1)])];
    assert!(refuses(
        &placed(1, past, Some(&map)),
        "a bucket range within 2^24",
        ranges_at
    ));
    let zero = vec![(1, vec![(0, 1 << 24, 0)])];
    assert!(refuses(
        &placed(1, zero, Some(&map)),
        "a partition from 1",
        ranges_at + 8
    ));
    Ok(())
}
