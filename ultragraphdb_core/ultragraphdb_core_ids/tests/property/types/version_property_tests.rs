/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_core_ids::{Epoch, Version};

// A version is epoch << 22 | sequence, and a commit's sequence is even and at most 2^22 − 2 (`PROTOCOL.md` section 1).
// Each property computes what it expects from the drawn numbers alone, by that formula, never from the type under test.

const EPOCH_MAX: u64 = (1 << 42) - 1;
const SEQUENCE_MAX: u32 = (1 << 22) - 2;

/// An epoch, and an even sequence within the commit bound.
fn epoch_and_sequence(tc: &TestCase) -> (u64, u32) {
    let epoch = tc.draw(gs::integers::<u64>().min_value(0).max_value(EPOCH_MAX));
    let half = tc.draw(
        gs::integers::<u32>()
            .min_value(0)
            .max_value(SEQUENCE_MAX / 2),
    );
    (epoch, half * 2)
}

fn commit(epoch: u64, sequence: u32) -> Option<Version> {
    Epoch::new(epoch)
        .and_then(|epoch| Version::commit(epoch, sequence))
        .ok()
}

#[hegel::test]
fn test_a_commit_version_packs_its_epoch_above_its_sequence(tc: TestCase) {
    let (epoch, sequence) = epoch_and_sequence(&tc);
    let version = commit(epoch, sequence);
    assert_eq!(
        version.map(Version::to_u64),
        Some(epoch << 22 | u64::from(sequence))
    );
    assert_eq!(version.map(|version| version.epoch().get()), Some(epoch));
    assert_eq!(version.map(Version::sequence), Some(sequence));
}

#[hegel::test]
fn test_a_version_round_trips_through_its_bytes(tc: TestCase) {
    let (epoch, sequence) = epoch_and_sequence(&tc);
    let version = commit(epoch, sequence);
    let decoded = version.and_then(|version| Version::decode(&version.to_be_bytes()).ok());
    assert!(version.is_some());
    assert_eq!(decoded, version);
}

#[hegel::test]
fn test_versions_order_as_their_epoch_then_their_sequence(tc: TestCase) {
    let (left, right) = (epoch_and_sequence(&tc), epoch_and_sequence(&tc));
    let versions = commit(left.0, left.1).zip(commit(right.0, right.1));
    assert_eq!(
        versions.map(|(a, b)| a.to_u64().cmp(&b.to_u64())),
        Some(left.cmp(&right))
    );
}

#[hegel::test]
fn test_a_sequence_past_the_commit_bound_is_refused(tc: TestCase) {
    let epoch = tc.draw(gs::integers::<u64>().min_value(0).max_value(EPOCH_MAX));
    let sequence = tc.draw(gs::integers::<u32>().min_value(SEQUENCE_MAX + 1));
    assert_eq!(commit(epoch, sequence), None);
}

#[hegel::test]
fn test_an_odd_sequence_is_refused(tc: TestCase) {
    let (epoch, sequence) = epoch_and_sequence(&tc);
    assert_eq!(commit(epoch, sequence + 1), None);
}
