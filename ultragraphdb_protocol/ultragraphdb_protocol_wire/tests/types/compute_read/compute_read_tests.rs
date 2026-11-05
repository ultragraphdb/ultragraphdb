/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, ComputeRead, PatternSlot, QuadPattern, ReadLevel, TermTable,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::bind::bind_tests::{TRANSACTION, WATERMARK};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden read-only ComputeRead: level 2 in tenant 5, floor epoch 31, principal 9, limit 100, 50 ms left, the client's
/// `max_frame` 1 MiB and credit 4, the pattern (?, <a>, <name>, ?) and the resume key `rk1`.
pub fn golden_compute_read() -> ComputeRead {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let name = terms.iri("http://ex.org/name");
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Bound(name),
        PatternSlot::Unbound,
    );
    ComputeRead::new(
        5,
        true,
        ReadLevel::MonotonicFast,
        terms,
        pattern,
        b"rk1".to_vec(),
    )
    .with_epoch(31)
    .with_principal(9)
    .with_limit(100)
    .with_deadline_ms(50)
    .with_client_limits(1 << 20, 4)
}

/// The golden read of a read-write transaction: level 1, bound to partition 3 at the Bind's watermark, principal 9,
/// 9,000 ms left, the client's `max_frame` at its floor and credit 1, the pattern (<g>, ?, <knows>, <b>).
pub fn golden_compute_read_rw() -> ComputeRead {
    let mut terms = TermTable::new();
    let g = terms.iri("http://ex.org/g");
    let knows = terms.iri("http://ex.org/knows");
    let b = terms.iri("http://ex.org/b");
    let pattern = QuadPattern::new(
        PatternSlot::Bound(g),
        PatternSlot::Unbound,
        PatternSlot::Bound(knows),
        PatternSlot::Bound(b),
    );
    ComputeRead::new(5, false, ReadLevel::Latest, terms, pattern, Vec::new())
        .with_partition(3)
        .with_transaction(TRANSACTION)
        .with_read_version(WATERMARK)
        .with_principal(9)
        .with_deadline_ms(9_000)
        .with_client_limits(1 << 19, 1)
}

#[test]
fn test_a_compute_read_builds_the_golden_frames() -> TestResult {
    let read = golden_compute_read();
    assert_eq!(
        (read.partition(), read.read_only(), read.level()),
        (0, true, ReadLevel::MonotonicFast)
    );
    assert_eq!(
        (read.named(), read.tenant(), read.read_version()),
        (false, 5, 0)
    );
    assert_eq!(
        (read.epoch(), read.principal(), read.transaction()),
        (31, 9, [0; 16])
    );
    assert_eq!(
        (read.limit(), read.deadline_ms(), read.resume()),
        (100, 50, &b"rk1"[..])
    );
    assert_eq!((read.terms().len(), read.pattern().parameters()), (2, 0));
    let golden = golden_role("compute_read")?;
    assert_eq!(read.frame(golden.header())?, role_bytes(&golden)?);
    let rw = golden_compute_read_rw();
    assert_eq!(
        (rw.partition(), rw.transaction(), rw.read_version()),
        (3, TRANSACTION, WATERMARK)
    );
    let golden_2 = golden_role("compute_read_rw")?;
    assert_eq!(rw.frame(golden_2.header())?, role_bytes(&golden_2)?);
    Ok(())
}

#[test]
fn test_a_named_version_is_marked_and_a_later_version_unmarks_it() -> TestResult {
    let named = golden_compute_read().with_named_version(77);
    assert_eq!((named.named(), named.read_version()), (true, 77));
    let plain = named.with_read_version(78);
    assert_eq!((plain.named(), plain.read_version()), (false, 78));
    Ok(())
}
