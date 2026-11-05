/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::Hash128;
use ultragraphdb_protocol_wire::{BodyBuilder, Scan, ScanAt, ScanRange, TermTable};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::bind::bind_tests::{TRANSACTION, WATERMARK};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden Scan: at 1, the version bound of epoch 30, once epoch 30 closed, principal 9, no transaction, 1,000
/// entries per range; partition 3 from 0x10 `abc` to 0x10 `abd`, partition 4 from 0x20 `k` to its keyspace's end.
pub fn golden_scan() -> Scan {
    let mut scan = Scan::new(
        ScanAt::ReadVersion,
        (30 << 22) | 0x3F_FFFF,
        30,
        9,
        [0; 16],
        1_000,
    );
    scan.push_range(ScanRange::new(3, b"\x10abc", b"\x10abd"));
    scan.push_range(ScanRange::new(4, b"\x20k", b""));
    scan
}

/// The golden read-write transaction's Scan: the one key 0x30 `key` of partition 3 at the Bind's watermark.
pub fn golden_scan_transaction() -> Scan {
    let mut scan = Scan::new(ScanAt::ReadVersion, WATERMARK, 0, 9, TRANSACTION, 0);
    scan.push_range(ScanRange::new(3, b"\x30key", b"\x30key\x00"));
    scan
}

/// The IRIs the golden transaction's Scan names, in its term table's order.
pub const IRIS: [&str; 2] = ["http://ex.org/a", "http://ex.org/b"];

/// The store's dictionary key of `iri` in namespace 5, the golden Bind's: keyspace byte 0x1B, the namespace big-endian,
/// kind 1, the IRI's 128-bit hash.
pub fn dictionary_key(iri: &str) -> Vec<u8> {
    let mut key = vec![0x1B];
    key.extend_from_slice(&5u64.to_be_bytes());
    key.push(1);
    key.extend_from_slice(Hash128::of(iri.as_bytes()).as_bytes());
    key
}

/// `key` and the key past it, the range holding it alone.
fn holding(key: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let past = [key, &[0]].concat();
    (key.to_vec(), past)
}

/// The golden transaction's Scan naming [`IRIS`]: partition 3, a range holding each IRI's dictionary key alone, in the
/// table's order, then one holding a forward list's key.
pub fn golden_scan_terms() -> Scan {
    let mut terms = TermTable::new();
    let mut scan = Scan::new(ScanAt::ReadVersion, WATERMARK, 0, 9, TRANSACTION, 0);
    for iri in IRIS {
        let _index: u32 = terms.iri(iri);
        let (first, past) = holding(&dictionary_key(iri));
        scan.push_range(ScanRange::new(3, &first, &past));
    }
    let id = |sequence: u64, bucket: u32| {
        [
            &1u16.to_be_bytes()[..],
            &sequence.to_be_bytes()[..],
            &bucket.to_be_bytes()[1..],
            &[0; 3][..],
        ]
        .concat()
    };
    let forward = [
        &[0x10][..],
        &5u64.to_be_bytes()[..],
        &[0; 16][..],
        &id(7, 0x2A_0001)[..],
        &id(8, 0x2A_0002)[..],
    ]
    .concat();
    let (first, past) = holding(&forward);
    scan.push_range(ScanRange::new(3, &first, &past));
    scan.with_terms(terms)
}

#[test]
fn test_a_scan_names_no_term_until_given_a_table() -> TestResult {
    let mut terms = TermTable::new();
    let _index: u32 = terms.iri(IRIS[0]);
    let scan = Scan::new(ScanAt::ReadVersion, WATERMARK, 0, 9, TRANSACTION, 0);
    assert!(scan.terms().is_empty());
    assert_eq!(scan.with_terms(terms.clone()).terms(), &terms);
    Ok(())
}

#[test]
fn test_a_scan_naming_iris_builds_the_golden_frame() -> TestResult {
    let golden = golden_role("scan_transaction_terms")?;
    assert_eq!(
        golden_scan_terms().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

#[test]
fn test_a_scan_builds_the_golden_frames() -> TestResult {
    let scan = golden_scan();
    assert_eq!(
        (scan.at(), scan.read_version(), scan.epoch()),
        (ScanAt::ReadVersion, (30 << 22) | 0x3F_FFFF, 30)
    );
    assert_eq!(
        (scan.principal(), scan.transaction(), scan.limit()),
        (9, [0; 16], 1_000)
    );
    let ranges: Vec<ScanRange<'_>> = scan.ranges().collect();
    assert_eq!(
        ranges,
        vec![
            ScanRange::new(3, b"\x10abc", b"\x10abd"),
            ScanRange::new(4, b"\x20k", b"")
        ]
    );
    let golden = golden_role("scan")?;
    assert_eq!(scan.frame(golden.header())?, role_bytes(&golden)?);
    let golden_2 = golden_role("scan_transaction")?;
    assert_eq!(
        golden_scan_transaction().frame(golden_2.header())?,
        role_bytes(&golden_2)?
    );
    Ok(())
}
