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

#[test]
fn test_a_reifier_is_a_blank_node_or_absent() -> TestResult {
    let [a, al, reifier, ..] = golden_terms()?;
    let mut batch = ResultBatch::new(2);
    // An IRI in the reifier column, the last.
    assert_eq!(
        batch
            .push_row(&[Some(&reifier), Some(&a)])
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "reifier",
            value: 1
        }
    );
    // A literal there too.
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
    assert_eq!(
        (batch.rows(), batch.cell(1, 0), batch.cell(1, 1)),
        (2, Some(0), Some(ABSENT))
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
    // A batch holds nine sections, the resume key the seventh and the column names the last two: no statement IDs.
    let body = ResultBatch::new(1).with_resume(b"k".to_vec()).body();
    let (offset, len) = crate::utils::golden_bodies::section(&body, 0, 6);
    assert_eq!((len, &body[offset..offset + len]), (1, &b"k"[..]));
    assert_eq!(crate::utils::golden_bodies::u32_at(&body, 0), 72);
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
