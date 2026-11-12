/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, MAX_EPOCH, RequestTerm, RequestTermView, Scan, ScanAt, ScanRange,
    ScanView, TermTable,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::scan::scan_tests::{
    DEADLINE_MS, IRIS, dictionary_key, golden_scan, golden_scan_terms, golden_scan_transaction,
};
use crate::utils::golden_bodies::{
    body_data, frame_of, put, reframe, section, view_any, with_command_header, with_section,
};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The sections of a Scan's body.
const SECTIONS: usize = 4;

fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

fn refused(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(ScanView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn field(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 524,
        field,
        value,
    }
}

/// The golden Scan with its ranges' section changed by `change`.
fn ranges(change: impl FnOnce(&mut Vec<u8>)) -> TestResult<Vec<u8>> {
    Ok(with_section(&bytes("scan")?, SECTIONS, 0, change))
}

/// The golden Scan with its keys' section changed by `change`.
fn keys(change: impl FnOnce(&mut Vec<u8>)) -> TestResult<Vec<u8>> {
    Ok(with_section(&bytes("scan")?, SECTIONS, 1, change))
}

/// The golden Scan naming IRIs with section `index` changed by `change`.
fn naming(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> TestResult<Vec<u8>> {
    Ok(with_section(
        &bytes("scan_transaction_terms")?,
        SECTIONS,
        index,
        change,
    ))
}

/// The refusal `CountMismatch` `"terms"`.
fn terms(count: u64, expected: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::CountMismatch {
        command: 524,
        field: "terms",
        count,
        expected,
    }
}

/// The term table's records of the golden Scan naming IRIs.
fn golden_records() -> TestResult<Vec<u8>> {
    let frame = bytes("scan_transaction_terms")?;
    let (at, len) = section(body_data(&frame), 0, 2);
    Ok(body_data(&frame)[at..at + len].to_vec())
}

/// `bytes` with one byte more at the end of its body.
fn with_trailing_byte(bytes: &[u8]) -> Vec<u8> {
    let mut data = body_data(bytes).to_vec();
    data.push(0);
    reframe(bytes, &data)
}

#[test]
fn test_a_scan_reads_each_iri_it_names_by_its_text() -> TestResult {
    let frame = bytes("scan_transaction_terms")?;
    let view = ScanView::from_frame(&frame_of(&frame)?)?;
    let read: Vec<_> = (0..u32::try_from(IRIS.len())?)
        .filter_map(|index| view.terms().get(index))
        .collect();
    assert_eq!(read, IRIS.map(RequestTermView::Iri));
    // Its first ranges hold each IRI's dictionary key alone, in the table's order.
    for (index, iri) in IRIS.iter().enumerate() {
        assert_eq!(
            view.range(index).map(|range| range.first().to_vec()),
            Some(dictionary_key(iri))
        );
    }
    assert_eq!(view.to_body()?, golden_scan_terms());
    assert_eq!(view_any(&frame)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_scan_naming_no_iri_reads_an_empty_table() -> TestResult {
    for name in ["scan", "scan_transaction"] {
        let frame = bytes(name)?;
        assert!(ScanView::from_frame(&frame_of(&frame)?)?.terms().is_empty());
    }
    Ok(())
}

#[test]
fn test_with_no_transaction_the_term_table_is_refused() -> TestResult {
    let records = golden_records()?;
    let with_records = with_section(&bytes("scan")?, SECTIONS, 2, |s| s.clone_from(&records));
    assert_eq!(refused(&with_records)?, terms(records.len() as u64, 0));
    let with_text = with_section(&bytes("scan")?, SECTIONS, 3, |s| s.push(b'a'));
    assert_eq!(refused(&with_text)?, terms(1, 0));
    let with_both = with_section(&with_records, SECTIONS, 3, |s| s.push(b'a'));
    assert_eq!(refused(&with_both)?, terms(records.len() as u64 + 1, 0));
    Ok(())
}

#[test]
fn test_a_table_holds_at_most_as_many_terms_as_ranges() -> TestResult {
    let frame = bytes("scan_transaction_terms")?;
    let view = ScanView::from_frame(&frame_of(&frame)?)?;
    let iris = IRIS.len();
    // As many ranges as terms is accepted: the last range and its keys dropped.
    let dropped = view.range(iris).context("the last range")?;
    let cut = dropped.first().len() + dropped.past().len();
    let equal = naming(0, |r| r.truncate(iris * 24))
        .map(|f| with_section(&f, SECTIONS, 1, |k| k.truncate(k.len().saturating_sub(cut))))?;
    assert_eq!(
        ScanView::from_frame(&frame_of(&equal)?)?.terms().len(),
        iris
    );
    // One range fewer than terms is refused.
    let fewer = naming(0, |r| r.truncate((iris - 1) * 24))?;
    assert_eq!(refused(&fewer)?, terms(iris as u64, iris as u64 - 1));
    Ok(())
}

#[test]
fn test_the_term_table_is_checked_as_sections_2_and_3() -> TestResult {
    let frame = bytes("scan_transaction_terms")?;
    let records_at = section(body_data(&frame), 0, 2).0;
    assert_eq!(
        refused(&naming(2, |r: &mut Vec<u8>| r.truncate(r.len().saturating_sub(1)))?)?,
        ProtocolErrorEnum::SectionLength {
            command: 524,
            section: 2,
            len: golden_records()?.len() - 1,
            unit: 24
        }
    );
    assert_eq!(refused(&naming(2, |r| r[24] = 6)?)?, field("term kind", 6));
    assert_eq!(
        refused(&naming(2, |r| r[24 + 2] = 1)?)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: 524,
            offset: records_at + 24 + 2
        }
    );
    let text_len = section(body_data(&frame), 0, 3).1;
    assert_eq!(
        refused(&naming(3, |t| t.push(b'a'))?)?,
        ProtocolErrorEnum::CountMismatch {
            command: 524,
            field: "term text",
            count: text_len as u64 + 1,
            expected: text_len as u64
        }
    );
    assert_eq!(
        refused(&naming(3, |t| t[0] = 0xFF)?)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 524,
            section: 3,
            offset: 0
        }
    );
    Ok(())
}

#[test]
fn test_a_scan_naming_a_transaction_is_at_1() -> TestResult {
    for choice in [ScanAt::Latest, ScanAt::LatestClosed, ScanAt::Named] {
        let bad = with_command_header(&bytes("scan_transaction")?, |h| h[0] = choice.number());
        assert_eq!(refused(&bad)?, field("at", choice.number().into()));
    }
    Ok(())
}

#[test]
fn test_a_scan_naming_a_transaction_waits_for_no_epoch() -> TestResult {
    let waiting = with_command_header(&bytes("scan_transaction")?, |h| put(h, 16, 30, 8));
    assert_eq!(refused(&waiting)?, field("epoch", 30));
    let past = with_command_header(&bytes("scan_transaction")?, |h| {
        put(h, 16, MAX_EPOCH + 1, 8);
    });
    assert_eq!(refused(&past)?, field("epoch", MAX_EPOCH + 1));
    Ok(())
}

/// A transaction's `at` comes before its read version's rule and its epoch's.
#[test]
fn test_a_transactions_at_comes_before_its_read_version_and_epoch() -> TestResult {
    let at_latest = with_command_header(&bytes("scan_transaction")?, |h| {
        h[0] = ScanAt::Latest.number();
        put(h, 16, 30, 8);
    });
    assert_eq!(
        refused(&at_latest)?,
        field("at", ScanAt::Latest.number().into())
    );
    Ok(())
}

/// Every entry of a Scan's table is an IRI, the first of any other kind named by its index.
#[test]
fn test_a_scan_term_is_an_iri() -> TestResult {
    let header = golden_role("scan_transaction_terms")?.header();
    for iris in 0..IRIS.len() {
        let mut table = TermTable::new();
        for iri in IRIS.iter().take(iris) {
            let _index = table.iri(iri);
        }
        let _other = table.intern(RequestTerm::BlankNode("b1".to_string()))?;
        let mut scan = Scan::new(ScanAt::ReadVersion, 2, 0, 9, [7; 16], 0, 0).with_terms(table);
        scan.push_range(ScanRange::new(3, b"\x10a", b""));
        scan.push_range(ScanRange::new(3, b"\x10b", b""));
        assert_eq!(
            refused(&scan.frame(header.clone())?)?,
            field("scan term", iris as u64)
        );
    }
    Ok(())
}

/// The table's own rules, then the count against the ranges, then each entry's kind, then the ranges.
#[test]
fn test_a_scan_terms_kind_lies_between_the_count_and_the_ranges() -> TestResult {
    let blank = naming(2, |r| r[24] = 2)?;
    assert_eq!(refused(&blank)?, field("scan term", 1));
    let over = with_section(&blank, SECTIONS, 0, |r| r.truncate(24));
    assert_eq!(refused(&over)?, terms(IRIS.len() as u64, 1));
    let range = with_section(&blank, SECTIONS, 0, |r| r[4] = 1);
    assert_eq!(refused(&range)?, field("scan term", 1));
    Ok(())
}

#[test]
fn test_a_named_transaction_reads_one_partition() -> TestResult {
    // Range 1's partition differs from range 0's, and range 0's own being 0 comes first.
    let other = naming(0, |r| put(r, 24, 4, 4))?;
    assert_eq!(refused(&other)?, field("partition", 4));
    let none = naming(0, |r| put(r, 0, 0, 4))?;
    assert_eq!(refused(&none)?, field("partition", 0));
    Ok(())
}

#[test]
fn test_the_checks_run_in_the_protocols_order() -> TestResult {
    // The ranges' records before the term table.
    let short_ranges = naming(0, |r: &mut Vec<u8>| r.truncate(r.len().saturating_sub(1)))
        .map(|f| with_section(&f, SECTIONS, 2, |t| t.fill(0xFF)))?;
    assert!(matches!(
        refused(&short_ranges)?,
        ProtocolErrorEnum::SectionLength { section: 0, .. }
    ));
    let no_ranges = naming(0, Vec::clear)?;
    assert_eq!(
        refused(&no_ranges)?,
        ProtocolErrorEnum::CountMismatch {
            command: 524,
            field: "ranges",
            count: 0,
            expected: 1
        }
    );
    // The term table before the ranges.
    let bad_term_and_range =
        naming(0, |r| r[4] = 1).map(|f| with_section(&f, SECTIONS, 2, |t| t[0] = 6))?;
    assert_eq!(refused(&bad_term_and_range)?, field("term kind", 6));
    let no_transaction = with_section(&ranges(|r| r[4] = 1)?, SECTIONS, 3, |t| t.push(b'a'));
    assert_eq!(refused(&no_transaction)?, terms(1, 0));
    // More terms than ranges before the ranges.
    let iris = IRIS.len();
    let more_terms = naming(0, |r| {
        r.truncate((iris - 1) * 24);
        r[4] = 1;
    })?;
    assert_eq!(refused(&more_terms)?, terms(iris as u64, iris as u64 - 1));
    // Within a range: the partition, the offsets, the keys' bounds, the past key.
    let partition_and_offset = ranges(|r| {
        put(r, 0, 0, 4);
        put(r, 8, 1, 4);
    })?;
    assert_eq!(refused(&partition_and_offset)?, field("partition", 0));
    let offset_and_bounds = ranges(|r| {
        put(r, 8, 1, 4);
        put(r, 20, 1_000, 4);
    })?;
    assert_eq!(refused(&offset_and_bounds)?, field("range", 0));
    let bounds_and_past = keys(|k| k[7] = b'b').map(|f| {
        with_section(&f, SECTIONS, 0, |r| {
            put(r, 20, 1_000, 4);
        })
    })?;
    assert!(matches!(
        refused(&bounds_and_past)?,
        ProtocolErrorEnum::IndexOutOfRange { at: 0, .. }
    ));
    // Every range before the section's end.
    let past_and_end = keys(|k| {
        k[7] = b'b';
        k.push(0);
    })?;
    assert_eq!(refused(&past_and_end)?, field("range", 0));
    Ok(())
}

/// A deadline bounds the wait for an epoch, so a Scan waiting for none names none; one waiting may name 0, refused at once.
#[test]
fn test_a_scan_names_a_deadline_only_with_an_epoch() -> TestResult {
    let none = with_command_header(&bytes("scan")?, |h| put(h, 52, 0, 4));
    assert_eq!(ScanView::from_frame(&frame_of(&none)?)?.deadline_ms(), 0);
    let least = with_command_header(&bytes("scan")?, |h| put(h, 52, 1, 4));
    assert_eq!(ScanView::from_frame(&frame_of(&least)?)?.deadline_ms(), 1);
    let named = with_command_header(&bytes("scan_transaction")?, |h| put(h, 52, 1, 4));
    assert_eq!(refused(&named)?, field("deadline_ms", 1));
    let plain = with_command_header(&bytes("scan")?, |h| {
        put(h, 16, 0, 8);
        put(h, 52, 7, 4);
    });
    assert_eq!(refused(&plain)?, field("deadline_ms", 7));
    Ok(())
}

/// `deadline_ms` comes after `epoch`'s rules and before the sections'.
#[test]
fn test_a_deadline_lies_between_the_epoch_and_the_sections() -> TestResult {
    let scan = bytes("scan")?;
    let bad_epoch = with_command_header(&scan, |h| {
        put(h, 16, MAX_EPOCH + 1, 8);
        put(h, 52, 0, 4);
    });
    assert_eq!(refused(&bad_epoch)?, field("epoch", MAX_EPOCH + 1));
    let bad_sections = with_command_header(&with_trailing_byte(&scan), |h| {
        put(h, 16, 0, 8);
        put(h, 52, 7, 4);
    });
    assert_eq!(refused(&bad_sections)?, field("deadline_ms", 7));
    Ok(())
}

/// The sections come after the header's rules and before section 0's.
#[test]
fn test_the_sections_lie_between_the_header_and_the_ranges() -> TestResult {
    let scan = bytes("scan")?;
    let trailing = with_trailing_byte(&scan);
    let end = body_data(&scan).len();
    assert_eq!(
        refused(&trailing)?,
        ProtocolErrorEnum::TrailingBytes {
            command: 524,
            end,
            len: end + 1
        }
    );
    let bad_epoch = with_command_header(&trailing, |h| put(h, 16, MAX_EPOCH + 1, 8));
    assert_eq!(refused(&bad_epoch)?, field("epoch", MAX_EPOCH + 1));
    let no_ranges = ranges(Vec::clear)?;
    let shorter = body_data(&no_ranges).len();
    assert_eq!(
        refused(&with_trailing_byte(&no_ranges))?,
        ProtocolErrorEnum::TrailingBytes {
            command: 524,
            end: shorter,
            len: shorter + 1
        }
    );
    Ok(())
}

#[test]
fn test_the_golden_scans_read_back_to_their_bodies() -> TestResult {
    let scan = bytes("scan")?;
    let view = ScanView::from_frame(&frame_of(&scan)?)?;
    assert_eq!(
        (view.at(), view.read_version(), view.epoch()),
        (ScanAt::ReadVersion, (30 << 22) | 0x3F_FFFF, 30)
    );
    assert_eq!(
        (view.principal(), view.transaction(), view.limit()),
        (9, [0; 16], 1_000)
    );
    assert_eq!(view.deadline_ms(), DEADLINE_MS);
    assert_eq!(view.range_count(), 2);
    assert_eq!(view.range(1), Some(ScanRange::new(4, b"\x20k", b"")));
    assert_eq!(view.range(2), None);
    assert_eq!(view.ranges().count(), 2);
    assert_eq!(view.to_body()?, golden_scan());
    let transaction = bytes("scan_transaction")?;
    let named = ScanView::from_frame(&frame_of(&transaction)?)?;
    assert_eq!(named.deadline_ms(), 0);
    assert_eq!(named.to_body()?, golden_scan_transaction());
    for frame in [scan, transaction] {
        view_any(&frame)??;
    }
    Ok(())
}

#[test]
fn test_unused_bytes_and_at_come_first() -> TestResult {
    let scan = bytes("scan")?;
    for at in [1usize, 7, 56, 127] {
        let bad = with_command_header(&scan, |h| {
            h[at] = 1;
            h[0] = 0;
        });
        assert_eq!(
            refused(&bad)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    for choice in [0u8, 5, 255] {
        let bad_2 = with_command_header(&scan, |h| h[0] = choice);
        assert_eq!(refused(&bad_2)?, field("at", choice.into()));
    }
    Ok(())
}

#[test]
fn test_only_at_1_and_4_name_a_version_and_at_4_waits_for_no_epoch() -> TestResult {
    let scan = bytes("scan")?;
    for choice in [2u8, 3] {
        let bad = with_command_header(&scan, |h| {
            h[0] = choice;
            put(h, 16, MAX_EPOCH + 1, 8);
        });
        assert_eq!(
            refused(&bad)?,
            field("read_version", (30 << 22) | 0x3F_FFFF)
        );
        let latest = with_command_header(&scan, |h| {
            h[0] = choice;
            put(h, 8, 0, 8);
        });
        assert_eq!(ScanView::from_frame(&frame_of(&latest)?)?.read_version(), 0);
    }
    let bad_2 = with_command_header(&scan, |h| h[0] = 4);
    assert_eq!(refused(&bad_2)?, field("epoch", 30));
    let named = with_command_header(&scan, |h| {
        h[0] = 4;
        put(h, 16, 0, 8);
        put(h, 52, 0, 4);
    });
    assert_eq!(
        ScanView::from_frame(&frame_of(&named)?)?.at(),
        ScanAt::Named
    );
    let bad_3 = with_command_header(&scan, |h| put(h, 16, MAX_EPOCH + 1, 8));
    assert_eq!(refused(&bad_3)?, field("epoch", MAX_EPOCH + 1));
    Ok(())
}

#[test]
fn test_the_ranges_are_whole_records_and_at_least_one() -> TestResult {
    assert_eq!(
        refused(&ranges(
            |s: &mut Vec<u8>| s.truncate(s.len().saturating_sub(1))
        )?)?,
        ProtocolErrorEnum::SectionLength {
            command: 524,
            section: 0,
            len: 47,
            unit: 24
        }
    );
    let none = with_section(&ranges(Vec::clear)?, SECTIONS, 1, Vec::clear);
    assert_eq!(
        refused(&none)?,
        ProtocolErrorEnum::CountMismatch {
            command: 524,
            field: "ranges",
            count: 0,
            expected: 1
        }
    );
    Ok(())
}

#[test]
fn test_a_range_holds_a_partition() -> TestResult {
    assert_eq!(
        refused(&ranges(|r| put(r, 0, 0, 4))?)?,
        field("partition", 0)
    );
    let bad_2 = ranges(|r| {
        put(r, 24, 0, 4);
        put(r, 32, 9, 4);
    })?;
    assert_eq!(refused(&bad_2)?, field("partition", 0));
    // A transaction's Scan reads one partition: range 1's partition 4 differs from range 0's.
    let bad_3 = with_command_header(&bytes("scan")?, |h| {
        h[32] = 1;
        put(h, 16, 0, 8);
        put(h, 52, 0, 4);
    });
    assert_eq!(refused(&bad_3)?, field("partition", 4));
    Ok(())
}

#[test]
fn test_keys_follow_each_other_without_a_gap() -> TestResult {
    // Range 0's first key at 1, its past key at 5, its first key empty; range 1's first key at 9.
    assert_eq!(refused(&ranges(|r| put(r, 8, 1, 4))?)?, field("range", 0));
    assert_eq!(refused(&ranges(|r| put(r, 16, 5, 4))?)?, field("range", 0));
    let empty = ranges(|r| {
        put(r, 12, 0, 4);
        put(r, 16, 0, 4);
    })?;
    assert_eq!(refused(&empty)?, field("range", 0));
    assert_eq!(refused(&ranges(|r| put(r, 32, 9, 4))?)?, field("range", 1));
    Ok(())
}

#[test]
fn test_keys_lie_inside_their_section_and_fill_it() -> TestResult {
    // Range 1's past key of 3 bytes ends at 13, past the 10 bytes of keys.
    assert_eq!(
        refused(&ranges(|r| put(r, 44, 3, 4))?)?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 524,
            section: 0,
            at: 1,
            index: 13,
            limit: 10
        }
    );
    assert_eq!(
        refused(&keys(|k| k.push(0))?)?,
        ProtocolErrorEnum::CountMismatch {
            command: 524,
            field: "range keys",
            count: 11,
            expected: 10
        }
    );
    Ok(())
}

#[test]
fn test_a_past_key_lies_above_its_first() -> TestResult {
    // Range 0's past key, bytes 4 to 7, made equal to its first and then below it.
    assert_eq!(refused(&keys(|k| k[7] = b'c')?)?, field("range", 0));
    assert_eq!(refused(&keys(|k| k[7] = b'b')?)?, field("range", 0));
    // A past key longer than its first, sharing it as a prefix, lies above it.
    let longer = ranges(|r| {
        put(r, 20, 5, 4);
        put(r, 32, 9, 4);
        put(r, 40, 11, 4);
    })
    .map(|frame| with_section(&frame, SECTIONS, 1, |k| k.insert(8, 0)))?;
    let view = ScanView::from_frame(&frame_of(&longer)?)?;
    assert_eq!(view.range(0).map(|r| r.past().len()), Some(5));
    Ok(())
}

#[test]
fn test_the_golden_after_reads_between_the_past_key_and_the_next_first_key() -> TestResult {
    let frame = bytes("scan_after")?;
    let view = ScanView::from_frame(&frame_of(&frame)?)?;
    let first = view.range(0).context("range 0")?;
    assert_eq!(first.after().len(), 16);
    assert_eq!(first.past(), b"\x10abd");
    let second = view.range(1).context("range 1")?;
    assert_eq!(second.first(), first.past());
    assert!(second.after().is_empty());
    assert_eq!(view_any(&frame)?, Ok(()));
    Ok(())
}

#[test]
fn test_an_after_within_the_keys_is_counted() -> TestResult {
    let frame = bytes("scan_after")?;
    // An after longer than what is left of the keys.
    let keys_len = ScanView::from_frame(&frame_of(&frame)?)?
        .ranges()
        .map(|r| r.first().len() + r.past().len() + r.after().len())
        .sum::<usize>();
    let past = with_section(&frame, SECTIONS, 0, |r| put(r, 4, keys_len as u64, 4));
    assert!(matches!(
        refused(&past)?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 524,
            section: 0,
            at: 0,
            ..
        }
    ));
    // An after shorter than it is leaves range 1's first key where the keys before do not end.
    let short = with_section(&frame, SECTIONS, 0, |r| put(r, 4, 15, 4));
    assert_eq!(refused(&short)?, field("range", 1));
    Ok(())
}

#[test]
fn test_a_read_version_is_a_version_or_a_version_bound() -> TestResult {
    let frame = bytes("scan")?;
    let odd = (30 << 22) | 5;
    let refusal = with_command_header(&frame, |h| put(h, 8, odd, 8));
    assert_eq!(refused(&refusal)?, field("read_version", odd));
    for version in [0u64, 30 << 22, (30 << 22) | 0x3F_FFFF] {
        let read = with_command_header(&frame, |h| put(h, 8, version, 8));
        assert_eq!(
            ScanView::from_frame(&frame_of(&read)?)?.read_version(),
            version
        );
    }
    Ok(())
}
