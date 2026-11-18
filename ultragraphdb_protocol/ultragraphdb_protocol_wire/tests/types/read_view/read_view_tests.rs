/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, PatternSlot, ReadView, RequestTermView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::read::read_tests::golden_read;
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header};

#[test]
fn test_the_golden_read_reads_its_values() -> TestResult {
    let bytes = golden("read")?.bytes()?;
    let view = ReadView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.transaction(), view.limit()), (5, 100));
    assert_eq!(view.resume(), &[1, 2, 3]);
    assert_eq!(view.pattern().subject(), PatternSlot::Bound(0));
    assert_eq!(view.pattern().predicate(), PatternSlot::Bound(1));
    assert_eq!(
        view.terms().get(0),
        Some(RequestTermView::Iri("http://ex.org/a"))
    );
    assert_eq!(
        view.terms().get(1),
        Some(RequestTermView::Iri("http://ex.org/name"))
    );
    assert_eq!(view.to_body()?, golden_read());
    Ok(())
}

#[test]
fn test_bytes_after_the_limit_are_unused() -> TestResult {
    let bytes = golden("read")?.bytes()?;
    // Bytes 12 to 127 are unused: the former flags among them.
    for at in [12usize, 15, 16, 127] {
        let frame = with_command_header(&bytes, |h| h[at] = 1);
        assert_eq!(
            ReadView::from_frame(&frame_of(&frame)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let limit = with_command_header(&bytes, |h| put(h, 8, u32::MAX.into(), 4));
    assert_eq!(ReadView::from_frame(&frame_of(&limit)?)?.limit(), u32::MAX);
    Ok(())
}

/// A Read of `<http://a>`'s statements in the default graph, framed by the wire's builder.
fn read_of_a(limit: u32, resume: Vec<u8>) -> TestResult<Vec<u8>> {
    use ultragraphdb_protocol_wire::{BodyBuilder, Command, Header, QuadPattern, Read, TermTable};
    let mut terms = TermTable::new();
    let a = terms.iri("http://a");
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let header = Header::new(crate::utils::golden_bodies::CLUSTER, Command::Read).with_stream(3);
    Ok(Read::new(1, limit, terms, pattern, resume).frame(header)?)
}

/// The pattern's hash is XXH3-64 of the pattern, the records and the text, laid out by hand from `PROTOCOL.md` §4.4, §4.5 and
/// §11.8 with no byte between them; the limit and the resume key, outside sections 0 to 2, take no part.
#[test]
fn test_the_pattern_hash_covers_the_pattern_and_its_terms_alone() -> TestResult {
    // §4.5: slots of graph, subject, predicate and object, each `kind` then `value` as u32 LE: the default graph (1, 0),
    // the subject bound to term 0 (2, 0), the predicate and the object unbound (0, 0).
    let mut expected: Vec<u8> = [1u32, 0, 2, 0, 0, 0, 0, 0]
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    // §4.4: one IRI record, kind 1, direction 0, six reserved bytes, its text at 0 for 8 bytes, the second text empty at 8.
    expected.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
    [0u32, 8, 8, 0]
        .iter()
        .for_each(|value| expected.extend_from_slice(&value.to_le_bytes()));
    expected.extend_from_slice(b"http://a");
    let hash = ultragraphdb_protocol_wire::xxh3_64(&expected);
    for (limit, resume) in [(0, Vec::new()), (7, vec![9; 13])] {
        let frame = read_of_a(limit, resume)?;
        assert_eq!(
            ReadView::from_frame(&frame_of(&frame)?)?.pattern_hash(),
            hash
        );
    }
    Ok(())
}

/// A Read's pattern hash is the one a ComputeRead of the same table and pattern held, so a resume key written before the
/// ComputeRead's removal names the same pattern.
#[test]
fn test_the_pattern_hash_is_the_compute_read_s() -> TestResult {
    use ultragraphdb_protocol_wire::{
        BodyBuilder, Command, ComputeRead, ComputeReadView, Header, ReadLevel,
    };
    let frame = read_of_a(0, Vec::new())?;
    let read = ReadView::from_frame(&frame_of(&frame)?)?;
    let computed = ComputeRead::new(
        1,
        true,
        ReadLevel::Snapshot,
        read.terms().to_table(),
        *read.pattern(),
        Vec::new(),
    )
    .with_client_limits(524_288, 1);
    let header =
        Header::new(crate::utils::golden_bodies::CLUSTER, Command::ComputeRead).with_stream(3);
    let bytes = computed.frame(header)?;
    let view = ComputeReadView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(read.pattern_hash(), view.pattern_hash());
    Ok(())
}
