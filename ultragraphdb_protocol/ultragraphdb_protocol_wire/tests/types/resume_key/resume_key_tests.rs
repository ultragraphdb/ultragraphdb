/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    RequestTerm, RequestTermView, ResumeKey, ResumeKeyView, ResumeRow, TermTable, xxh3_64,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

const PATTERN: u64 = 0x1122_3344_5566_7788;
const SCOPE: u64 = 0x99AA_BBCC_DDEE_FF00;
const FIXED: usize = 24;
const RESUME_ROW: usize = 20;

/// A key of a row in a named graph with a reifier and a literal object.
fn key() -> TestResult<Vec<u8>> {
    let mut terms = TermTable::new();
    let graph = terms.iri("http://ex.org/g");
    let subject = terms.iri("http://ex.org/a");
    let predicate = terms.iri("http://ex.org/p");
    let object = terms.intern(RequestTerm::Literal {
        lexical: "x".to_string(),
        datatype: None,
    })?;
    let reifier = terms.iri("http://ex.org/r");
    let row = ResumeRow::new(Some(graph), subject, predicate, object, Some(reifier));
    Ok(ResumeKey::new(PATTERN, SCOPE, terms, row).encode())
}

/// `bytes` with its checksum written again, so a rule behind it is what refuses.
fn checksummed(mut bytes: Vec<u8>) -> TestResult<Vec<u8>> {
    let sum = xxh3_64(bytes.get(8..).context("the rest")?).to_le_bytes();
    bytes
        .get_mut(..8)
        .context("the checksum")?
        .copy_from_slice(&sum);
    Ok(bytes)
}

#[test]
fn test_a_key_reads_back_as_it_was_written() -> TestResult {
    let bytes = key()?;
    let view = ResumeKeyView::read(&bytes)?;
    assert_eq!(view.pattern(), PATTERN);
    assert_eq!(view.scope(), SCOPE);
    let row = view.row();
    let iri_at = |index| view.terms().get(index);
    assert_eq!(
        iri_at(row.graph().context("a graph")?),
        Some(RequestTermView::Iri("http://ex.org/g"))
    );
    assert_eq!(
        iri_at(row.subject()),
        Some(RequestTermView::Iri("http://ex.org/a"))
    );
    assert_eq!(
        iri_at(row.predicate()),
        Some(RequestTermView::Iri("http://ex.org/p"))
    );
    assert_eq!(
        iri_at(row.reifier().context("a reifier")?),
        Some(RequestTermView::Iri("http://ex.org/r"))
    );
    assert!(iri_at(row.object()).is_some());
    Ok(())
}

#[test]
fn test_the_default_graph_and_no_reifier_read_as_none() -> TestResult {
    let mut terms = TermTable::new();
    let (a, p) = (terms.iri("http://ex.org/a"), terms.iri("http://ex.org/p"));
    let bytes = ResumeKey::new(1, 2, terms, ResumeRow::new(None, a, p, a, None)).encode();
    let row = ResumeKeyView::read(&bytes)?.row();
    assert_eq!(row.graph(), None);
    assert_eq!(row.reifier(), None);
    Ok(())
}

#[test]
fn test_a_changed_byte_fails_the_checksum() -> TestResult {
    let mut bytes = key()?;
    let last = bytes.last_mut().context("a last byte")?;
    *last ^= 1;
    let refused = ResumeKeyView::read(&bytes).err().context("a refusal")?;
    assert!(
        matches!(refused.0, ProtocolErrorEnum::FieldValue { field, .. } if field == "resume key"),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_a_key_shorter_than_its_checksum_is_refused() {
    assert!(ResumeKeyView::read(&[0; 4]).is_err());
    assert!(ResumeKeyView::read(&[]).is_err());
}

#[test]
fn test_a_last_row_of_another_length_is_refused() -> TestResult {
    let bytes = key()?;
    // Section 0's length, behind the checksum, in the section table after the fixed part. Four bytes more take in the
    // padding before section 1, which still starts where the sections' rule puts it, so the row's own rule refuses.
    let mut longer = bytes.clone();
    let len_at = FIXED + 4;
    let longer_row = RESUME_ROW + 4;
    longer
        .get_mut(len_at..len_at + 4)
        .context("the section's length")?
        .copy_from_slice(&u32::try_from(longer_row)?.to_le_bytes());
    let refused = ResumeKeyView::read(&checksummed(longer)?)
        .err()
        .context("a refusal")?;
    assert!(
        matches!(
            refused.0,
            ProtocolErrorEnum::CountMismatch { field, count, expected, .. }
                if field == "resume row" && count == longer_row as u64 && expected == RESUME_ROW as u64
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_a_term_table_that_does_not_read_is_refused() -> TestResult {
    let mut bytes = key()?;
    // Section 1, the term records, from its offset in the section table; its first record's kind byte made one no term has.
    let offset_at = FIXED + 8;
    let records_at = u32::from_le_bytes(
        bytes
            .get(offset_at..offset_at + 4)
            .context("section 1's offset")?
            .try_into()?,
    ) as usize;
    *bytes.get_mut(records_at).context("the first kind byte")? = 0xFF;
    let refused = ResumeKeyView::read(&checksummed(bytes)?)
        .err()
        .context("a refusal")?;
    assert!(
        !matches!(refused.0, ProtocolErrorEnum::FieldValue { field, .. } if field == "resume key"),
        "the checksum holds, so the table refuses: {refused:?}"
    );
    Ok(())
}

#[test]
fn test_an_index_past_the_table_is_refused() -> TestResult {
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let bytes = ResumeKey::new(1, 2, terms, ResumeRow::new(None, a, 9, a, None)).encode();
    let refused = ResumeKeyView::read(&bytes).err().context("a refusal")?;
    assert!(
        matches!(refused.0, ProtocolErrorEnum::IndexOutOfRange { .. }),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_absent_is_refused_for_the_subject_predicate_and_object() -> TestResult {
    for slot in 0..3 {
        let mut terms = TermTable::new();
        let a = terms.iri("http://ex.org/a");
        let mut positions = [a, a, a];
        if let Some(position) = positions.get_mut(slot) {
            *position = u32::MAX;
        }
        let [subject, predicate, object] = positions;
        let bytes = ResumeKey::new(
            1,
            2,
            terms,
            ResumeRow::new(None, subject, predicate, object, None),
        )
        .encode();
        assert!(ResumeKeyView::read(&bytes).is_err(), "slot {slot}");
    }
    Ok(())
}
