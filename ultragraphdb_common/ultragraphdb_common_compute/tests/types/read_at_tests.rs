/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! How a read's first Scan of a partition chooses its version (`PROTOCOL.md` §13.12, §13.13), as the played partition is asked it.

use compio::runtime::Runtime;
use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{ComputeRead, ReadLevel, ScanAt};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::played_answers::{Pattern, answer_on, read_of, snapshot};
use crate::utils::played_cluster::{Asked, EPOCH, Played, WAIT, id, version, within};

const S: &str = "http://ex.org/s";
const P: &str = "http://ex.org/p";
const O: &str = "http://ex.org/o";

/// The first Scan one partition holding `<s> <p> <o>` is asked by `read` of `?s <p> ?o`, the read answered whole.
fn first_scan(read: impl FnOnce(&Pattern) -> TestResult<ComputeRead>) -> TestResult<Asked> {
    Runtime::new()?.block_on(within(WAIT, "the read", async {
        let played = Played::start(1, read_times_of(None, None)).await?;
        let partition = played.partition(1)?;
        let (s, p, o) = (id(1)?, id(2)?, id(3)?);
        partition.term(S, s);
        partition.term(P, p);
        partition.term(O, o);
        partition.quads(Graph::Default, s, p, &[EncodedTerm::id(o)]);
        let mut client = played.client().await?;
        let pattern = Pattern::of(None, Some(P), None);
        let rows = answer_on(&mut client, &read(&pattern)?).await?.whole()?;
        // The columns: the unbound subject and object, then the reifier, which no statement names here.
        assert_eq!(
            rows,
            vec![vec![Some(S.to_string()), Some(O.to_string()), None]]
        );
        partition.scans().into_iter().next().context("a first Scan")
    }))
}

/// A read-only read at snapshot level reads at its version once the partition has closed its epoch: `at` 1, the read's version
/// and epoch.
#[test]
fn test_a_snapshot_read_scans_at_its_version_past_its_epoch() -> TestResult {
    let first = first_scan(snapshot)?;
    assert_eq!(first.at, ScanAt::ReadVersion);
    assert_eq!(first.read_version, version()?);
    assert_eq!(first.epoch, EPOCH);
    Ok(())
}

/// A read at a version the client named reads at it without waiting: `at` 4, that version, and epoch 0.
#[test]
fn test_a_named_read_scans_at_its_version_waiting_for_no_epoch() -> TestResult {
    let named = version()? - 5;
    let first = first_scan(|pattern| {
        Ok(read_of(pattern, true, ReadLevel::Snapshot, Vec::new())?.with_named_version(named))
    })?;
    assert_eq!(first.at, ScanAt::Named);
    assert_eq!(first.read_version, named);
    assert_eq!(first.epoch, 0);
    Ok(())
}
