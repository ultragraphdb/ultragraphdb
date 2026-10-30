/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{ABSENT, BodyBuilder, ResultBatch, ResultTerm};

use crate::utils::golden_bodies::golden;

pub const LONG: &str = "a long lexical form beyond fifteen";
/// The skolem IRI of blank node 6 in namespace 0.
pub const SKOLEM_6: &str = "https://w3id.org/ultragraphdb/ns/namespace/0/.well-known/genid/6";

pub fn golden_terms() -> [ResultTerm; 4] {
    let mut big = vec![0x21];
    big.extend_from_slice(Hash128::of(LONG.as_bytes()).as_bytes());
    [
        ResultTerm::new(EncodedTerm::uid(5)).with_text("http://ex.org/a"),
        ResultTerm::new(EncodedTerm::from_bytes(&[0x20, 2, b'A', b'l']).unwrap()),
        ResultTerm::new(EncodedTerm::from_bytes(&big).unwrap()).with_text(LONG),
        ResultTerm::blank_node(6, SKOLEM_6),
    ]
}

pub fn golden_batch() -> ResultBatch {
    let [a, al, long, b] = golden_terms();
    let mut batch = ResultBatch::with_statement_ids(2).with_resume(b"rk".to_vec());
    batch
        .push_row_with_id(&[Some(&a), Some(&al)], 0x1001)
        .unwrap();
    batch
        .push_row_with_id(&[Some(&a), Some(&long)], 0x1002)
        .unwrap();
    batch.push_row_with_id(&[Some(&b), None], 0x1003).unwrap();
    batch
}

#[test]
fn test_the_golden_batch_builds_the_golden_frame() {
    let batch = golden_batch();
    assert_eq!(batch.columns(), 2);
    assert_eq!(batch.rows(), 3);
    assert_eq!(batch.terms(), &golden_terms());
    assert_eq!(batch.resume(), b"rk");
    assert_eq!(
        batch.frame(golden("result").header()).unwrap(),
        golden("result").bytes()
    );
    assert_eq!(batch.encoded_len(), batch.body().len());
    assert_eq!(batch.encoded_len(), 600 - 256);
    assert_eq!(batch.statement_ids(), Some(&[0x1001, 0x1002, 0x1003][..]));
}

#[test]
fn test_cells_index_the_term_table_in_order_of_first_use() {
    let batch = golden_batch();
    assert_eq!(batch.cell(0, 0), Some(0));
    assert_eq!(batch.cell(0, 1), Some(0));
    assert_eq!(batch.cell(0, 2), Some(3));
    assert_eq!(batch.cell(1, 0), Some(1));
    assert_eq!(batch.cell(1, 1), Some(2));
    assert_eq!(batch.cell(1, 2), Some(ABSENT));
    assert_eq!(batch.cell(2, 0), None);
    assert_eq!(batch.cell(0, 3), None);
}

#[test]
fn test_a_row_of_the_wrong_width_is_refused() {
    let [a, ..] = golden_terms();
    let mut batch = ResultBatch::new(2);
    assert_eq!(
        batch.push_row(&[Some(&a)]).unwrap_err().0,
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
}

#[test]
fn test_a_batch_carries_statement_ids_for_every_row_or_none() {
    let [a, ..] = golden_terms();
    let mut plain = ResultBatch::new(1);
    assert_eq!(plain.statement_ids(), None);
    assert_eq!(
        plain.push_row_with_id(&[Some(&a)], 1).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "statement ids",
            count: 1,
            expected: 0
        }
    );
    assert_eq!(plain.rows(), 0, "a refused row is not added");
    let mut ids = ResultBatch::with_statement_ids(1);
    assert_eq!(
        ids.push_row(&[Some(&a)]).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "statement ids",
            count: 0,
            expected: 1
        }
    );
    assert_eq!(ids.statement_ids(), Some(&[][..]));
    // A row of the wrong width with an ID adds neither.
    assert_eq!(
        ids.push_row_with_id(&[None, None], 9).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 13,
            field: "row",
            count: 2,
            expected: 1
        }
    );
    assert_eq!((ids.rows(), ids.statement_ids()), (0, Some(&[][..])));
    // Section 7 holds the IDs, u64s little-endian, one per row.
    ids.push_row_with_id(&[None], 0x0102_0304_0506_0708)
        .unwrap();
    let body = ids.body();
    let (offset, len) = crate::utils::golden_bodies::section(&body, 0, 7);
    assert_eq!(len, 8);
    assert_eq!(
        &body[offset..offset + 8],
        &0x0102_0304_0506_0708u64.to_le_bytes()
    );
    let (_, plain_len) = crate::utils::golden_bodies::section(&ResultBatch::new(1).body(), 0, 7);
    assert_eq!(plain_len, 0);
}

#[test]
fn test_a_blank_node_and_an_iri_of_one_uid_are_two_terms() {
    // One UID is either a blank node or an IRI in a store; a batch keys its table by the result's encoding.
    let iri = ResultTerm::new(EncodedTerm::uid(6)).with_text("http://ex.org/b");
    let blank = ResultTerm::blank_node(6, SKOLEM_6);
    let mut batch = ResultBatch::new(2);
    batch.push_row(&[Some(&iri), Some(&blank)]).unwrap();
    assert_eq!(batch.terms().len(), 2);
    assert_eq!(batch.cell(1, 0), Some(1));
}
