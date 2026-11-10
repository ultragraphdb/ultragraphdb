/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{ABSENT, BodyBuilder, Monotonic, ResultBatch, ResultTerm};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::golden;

pub const LONG: &str = "a long lexical form beyond fifteen";
/// A blank node's skolem IRI in tenant 0: bucket 0x00002a in 6 hexadecimal digits, then 104 random bits in 26
/// (PROTOCOL.md §10.2).
pub const SKOLEM_6: &str =
    "https://w3id.org/ultragraphdb/ns/tenant/0/.well-known/genid/00002a-0123456789abcdef0123456789";

/// The golden batch's reifier: the skolem IRI of a statement of bucket 0x00002a (PROTOCOL.md §10.2).
pub const REIFIER: &str =
    "https://w3id.org/ultragraphdb/ns/tenant/0/.well-known/genid/00002a-fedcba9876543210fedcba9876";

pub fn golden_terms() -> TestResult<[ResultTerm; 5]> {
    let mut big = vec![0x21];
    big.extend_from_slice(Hash128::of(LONG.as_bytes()).as_bytes());
    Ok([
        ResultTerm::iri("http://ex.org/a"),
        ResultTerm::new(EncodedTerm::from_bytes(&[0x20, 2, b'A', b'l'])?),
        ResultTerm::blank_node(REIFIER),
        ResultTerm::new(EncodedTerm::from_bytes(&big)?).with_text(LONG),
        ResultTerm::blank_node(SKOLEM_6),
    ])
}

/// The golden batch: two unbound slots and the reifier column, read at epoch 30 in partition 2.
pub fn golden_batch() -> TestResult<ResultBatch> {
    let [a, al, reifier, long, b] = golden_terms()?;
    let mut batch = ResultBatch::new(3)
        .with_resume(b"rk".to_vec())
        .with_read_at(30, 2);
    batch.push_row(&[Some(&a), Some(&al), Some(&reifier)])?;
    batch.push_row(&[Some(&a), Some(&long), None])?;
    batch.push_row(&[Some(&b), None, None])?;
    Ok(batch)
}

#[test]
fn test_the_golden_batch_builds_the_golden_frame() -> TestResult {
    let batch = golden_batch()?;
    assert_eq!(batch.columns(), 3);
    assert_eq!(batch.rows(), 3);
    assert_eq!((batch.epoch(), batch.partition()), (30, 2));
    assert_eq!(batch.terms(), &golden_terms()?);
    assert_eq!(batch.resume(), b"rk");
    assert_eq!(
        batch.frame(golden("result")?.header())?,
        golden("result")?.bytes()?
    );
    assert_eq!(batch.encoded_len(), batch.body().len());
    assert_eq!(batch.encoded_len(), golden("result")?.size as usize - 256);
    Ok(())
}

#[test]
fn test_cells_index_the_term_table_in_order_of_first_use() -> TestResult {
    let batch = golden_batch()?;
    assert_eq!(batch.cell(0, 0), Some(0));
    assert_eq!(batch.cell(0, 1), Some(0));
    assert_eq!(batch.cell(0, 2), Some(4));
    assert_eq!(batch.cell(1, 0), Some(1));
    assert_eq!(batch.cell(1, 1), Some(3));
    assert_eq!(batch.cell(1, 2), Some(ABSENT));
    assert_eq!(batch.cell(2, 0), Some(2));
    assert_eq!(batch.cell(2, 1), Some(ABSENT));
    assert_eq!(batch.cell(3, 0), None);
    assert_eq!(batch.cell(0, 3), None);
    Ok(())
}

#[test]
fn test_a_row_of_the_wrong_width_is_refused() -> TestResult {
    let [a, ..] = golden_terms()?;
    let mut batch = ResultBatch::new(2);
    assert_eq!(
        batch.push_row(&[Some(&a)]).err().context("a refusal")?.0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "row",
            count: 1,
            expected: 2
        }
    );
    assert_eq!(batch.rows(), 0);
    assert_eq!(batch.push_row(&[None, None]), Ok(()));
    assert_eq!((batch.rows(), batch.cell(1, 0)), (1, Some(ABSENT)));
    let mut none = ResultBatch::new(0);
    assert_eq!(none.push_row(&[]), Ok(()));
    assert_eq!(none.rows(), 1);
    Ok(())
}

/// `PROTOCOL.md` §9.1 and §10.2: the reifier column holds a skolem IRI tagged as a blank node, a declared relation's
/// handle IRI tagged as an IRI, or `ABSENT`.
#[test]
fn test_a_reifier_is_a_blank_node_an_iri_or_absent() -> TestResult {
    let [a, al, reifier, ..] = golden_terms()?;
    let mut batch = ResultBatch::new(2);
    // A literal in the reifier column, the last.
    assert_eq!(
        batch
            .push_row(&[None, Some(&al)])
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "reifier",
            value: 1
        }
    );
    assert_eq!(batch.rows(), 0, "a refused row is not added");
    // A blank node elsewhere is any term; in the reifier column it names the statement.
    batch.push_row(&[Some(&reifier), Some(&reifier)])?;
    batch.push_row(&[Some(&a), None])?;
    // An IRI there names an edge-encoded instance of a declared relation by its handle IRI.
    batch.push_row(&[None, Some(&a)])?;
    assert_eq!(
        (
            batch.rows(),
            batch.cell(1, 0),
            batch.cell(1, 1),
            batch.cell(1, 2)
        ),
        (3, Some(0), Some(ABSENT), Some(1))
    );
    // The command header: epoch at 8, partition at 16, monotonic at 20, nothing after.
    let header = batch
        .with_read_at(7, 9)
        .with_monotonic(Monotonic::Immutable)
        .command_header();
    assert_eq!(&header[8..16], &7u64.to_le_bytes());
    assert_eq!(&header[16..20], &9u32.to_le_bytes());
    assert_eq!(header[20], Monotonic::Immutable.number());
    assert!(header[21..].iter().all(|b| *b == 0));
    // A batch holds twelve sections (PROTOCOL.md §9.1), the resume key the seventh, the column names the eighth and
    // ninth and the warnings the last three: no statement IDs. Section 0 starts after the table, at 12 × 8 bytes.
    let body = ResultBatch::new(1).with_resume(b"k".to_vec()).body();
    let (offset, len) = crate::utils::golden_bodies::section(&body, 0, 6);
    assert_eq!((len, &body[offset..offset + len]), (1, &b"k"[..]));
    assert_eq!(crate::utils::golden_bodies::u32_at(&body, 0), 96);
    Ok(())
}

#[test]
fn test_a_blank_node_and_an_iri_of_one_text_are_two_terms() -> TestResult {
    // One hash is either a blank node's or an IRI's; a batch keys its table by the result's encoding, tag included.
    let iri = ResultTerm::iri(SKOLEM_6);
    let blank = ResultTerm::blank_node(SKOLEM_6);
    let mut batch = ResultBatch::new(2);
    batch.push_row(&[Some(&iri), Some(&blank)])?;
    assert_eq!(batch.terms().len(), 2);
    assert_eq!(batch.cell(1, 0), Some(1));
    Ok(())
}

/// A query's last column is a projection like the others: an IRI there is no reifier.
#[test]
fn test_a_querys_row_holds_any_term_in_its_last_column() -> TestResult {
    let mut batch = ResultBatch::new(2)
        .with_column_names(vec!["n".to_string(), "age".to_string()])
        .with_monotonic(Monotonic::Retractable)
        .with_read_at(30, 0);
    let a = ResultTerm::iri("http://ex.org/a");
    batch.push_query_row(&[Some(&a), None])?;
    assert_eq!(batch.monotonic(), Monotonic::Retractable);
    assert_eq!(batch.column_names(), ["n", "age"]);
    assert_eq!(
        batch.frame(golden("result_query")?.header())?,
        golden("result_query")?.bytes()?
    );
    let mut last = ResultBatch::new(1);
    last.push_query_row(&[Some(&a)])?;
    assert_eq!(last.rows(), 1);
    assert_eq!(
        last.push_query_row(&[]).err().context("a refusal")?.0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "row",
            count: 0,
            expected: 1
        }
    );
    Ok(())
}

#[test]
fn test_a_frame_stays_within_what_the_size_with_a_row_says() -> TestResult {
    use ultragraphdb_protocol_wire::{Command, Header};

    let terms: Vec<ResultTerm> = (0..200)
        .map(|at| ResultTerm::iri(format!("http://ex.org/term/{at}")))
        .collect();
    let mut batch = ResultBatch::new(2);
    for term in &terms {
        let row = [Some(term), None];
        let promised = batch.frame_size_with(&row, 16);
        batch.push_row(&row)?;
        let held = batch
            .clone()
            .with_resume(vec![7; 16])
            .frame(Header::new(1, Command::Result).with_stream(1))?;
        assert!(held.len() <= promised, "{} above {promised}", held.len());
    }
    // A term the batch holds takes a cell and no more.
    let first = terms.first().context("a term")?;
    let again = [Some(first), None];
    let more = batch.frame_size_with(&again, 0) - batch.frame_size_with(&[], 0);
    assert!(
        more <= 8 + 64,
        "two cells, and a chunk's checksum at most: {more}"
    );
    Ok(())
}

/// The padding `body`'s twelve sections take, read from its section table (`PROTOCOL.md` §4.1, §9.1): the table is
/// 12 × 8 bytes, and each section starts where the one before it ended, rounded up.
fn section_padding(body: &[u8]) -> usize {
    use crate::utils::golden_bodies::section;

    (0..12)
        .fold((96, 0), |(end, padding), index| {
            let (offset, len) = section(body, 0, index);
            (offset + len, padding + offset - end)
        })
        .1
}

/// Where no string is shared, the size a batch states with a row is the frame it builds but for the padding the frame
/// did not need: the section table, a cell per column, per new term its offset, its encoding, its two string indices,
/// its strings' two ends and texts, and up to 7 bytes of padding before each of the nine sections that can follow an
/// end off a multiple of 8 (`PROTOCOL.md` §4.1, §4.9, §9.1): every one of the twelve but section 0, which starts at the
/// table's end, and sections 4 and 10, which follow the term texts' 8-byte pairs and the warnings' 24 bytes.
#[test]
fn test_the_size_with_a_row_is_the_frame_with_its_padding_at_most() -> TestResult {
    use ultragraphdb_protocol_wire::{Command, Header};

    use crate::utils::golden_bodies::body_data;

    let terms: [ResultTerm; 6] = std::array::from_fn(|at| {
        ResultTerm::iri(format!("http://ex.org/t{at}")).with_annotation(format!("note {at}"))
    });
    let [t0, t1, t2, t3, t4, t5] = &terms;
    // A row of new terms, a held term, one new term twice, no term, then held and new together.
    let rows = [
        [Some(t0), Some(t1)],
        [Some(t0), None],
        [Some(t2), Some(t2)],
        [None, None],
        [Some(t3), Some(t4)],
        [Some(t5), Some(t0)],
    ];
    let mut batch = ResultBatch::new(2);
    for (row, resume) in rows.iter().zip([0usize, 1, 3, 9, 5, 2]) {
        let promised = batch.frame_size_with(row, resume);
        batch.push_query_row(row)?;
        let frame = batch
            .clone()
            .with_resume(vec![7; resume])
            .frame(Header::new(1, Command::Result).with_stream(1))?;
        let padding = section_padding(body_data(&frame));
        assert_eq!(
            promised,
            frame.len() + 9 * 7 - padding,
            "row {}",
            batch.rows()
        );
    }
    Ok(())
}

/// A query's first batch carries its warnings (`PROTOCOL.md` §4.9, §9.1): read back as built, and counted in the size
/// with a row as the frame lays them out, each warning's 24 bytes, its message's end and its bytes, so the size is the
/// frame's with its padding at most, as for a batch without them: no message shared, the term holding a text and an
/// annotation, as the test above's do.
#[test]
fn test_a_batch_with_warnings_counts_them_and_reads_them_back() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyView, Command, Header, ResultView, Warning};

    use crate::utils::golden_bodies::{CLUSTER, body_data, frame_of};

    let warnings = vec![
        Warning::new((3 << 16) | 110, "UQL0110 1:7 Person is retired").with_location(6..12),
        Warning::new((3 << 16) | 111, "UQL0111 2:1 Org is retired: use Company")
            .with_location(20..23),
    ];
    let term = ResultTerm::iri("http://ex.org/t0").with_annotation("note 0");
    let row = [Some(&term)];
    let mut batch = ResultBatch::new(1).with_warnings(warnings.clone());
    assert_eq!(batch.warnings(), warnings.as_slice());
    let promised = batch.frame_size_with(&row, 0);
    batch.push_query_row(&row)?;
    let frame = batch.frame(Header::new(CLUSTER, Command::Result).with_stream(1))?;
    let padding = section_padding(body_data(&frame));
    assert_eq!(promised, frame.len() + 9 * 7 - padding);
    // Read back from a batch whose IRI holds no annotation, which §10.3 gives no IRI.
    let iri = ResultTerm::iri("http://ex.org/t0");
    let mut readable = ResultBatch::new(1).with_warnings(warnings.clone());
    readable.push_query_row(&[Some(&iri)])?;
    let frame = readable.frame(Header::new(CLUSTER, Command::Result).with_stream(1))?;
    let view = ResultView::from_frame(&frame_of(&frame)?)?;
    assert_eq!(view.warnings().len(), 2);
    assert!(!view.warnings().is_empty());
    assert_eq!(view.to_body()?.warnings(), warnings.as_slice());
    Ok(())
}

/// A query's first batch holds its column names in sections 7 and 8 (`PROTOCOL.md` §9.1), a string list: per name its
/// end, a u32, and its bytes (§4.3). The size with a row counts them, so it is the frame's with its padding at most, as
/// for a batch without them: two names, `n` and `label`, beside a row of two terms each holding a text and an annotation.
#[test]
fn test_the_size_with_a_row_counts_the_column_names() -> TestResult {
    use ultragraphdb_protocol_wire::{Command, Header};

    use crate::utils::golden_bodies::body_data;

    let terms: [ResultTerm; 2] = std::array::from_fn(|at| {
        ResultTerm::iri(format!("http://ex.org/t{at}")).with_annotation(format!("note {at}"))
    });
    let row = [Some(&terms[0]), Some(&terms[1])];
    let mut batch =
        ResultBatch::new(2).with_column_names(vec!["n".to_string(), "label".to_string()]);
    let promised = batch.frame_size_with(&row, 0);
    batch.push_query_row(&row)?;
    let frame = batch.frame(Header::new(1, Command::Result).with_stream(1))?;
    let padding = section_padding(body_data(&frame));
    assert_eq!(promised, frame.len() + 9 * 7 - padding);
    Ok(())
}
