/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Literal;
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_quads::{QuadErrorEnum, StatementRecord};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn id(prefix: TypePrefix, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        prefix,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

fn year() -> TestResult<EncodedTerm> {
    Ok(EncodedTerm::literal(Literal::from(2020).as_ref(), |_| {
        Err(TermError::malformed(
            Vec::new(),
            "an integer names no datatype ID",
        ))
    })?
    .into_term())
}

fn record() -> TestResult<StatementRecord> {
    let r = |n| id(TypePrefix::RESOURCE, 0xAB_CDEF, n);
    let mut record =
        StatementRecord::new(r(3)?, r(4)?, EncodedTerm::id(r(5)?), vec![(r(6)?, year()?)]);
    record
        .properties_mut()
        .push((r(7)?, EncodedTerm::id(r(8)?)));
    Ok(record)
}

#[test]
fn test_a_record_round_trips() -> TestResult {
    let record = record()?;
    let read = StatementRecord::decode(b"k", &record.encode())?;
    assert_eq!(read, record);
    let r = |n| id(TypePrefix::RESOURCE, 0xAB_CDEF, n);
    assert_eq!(
        (read.subject(), read.predicate(), read.object()),
        (r(3)?, r(4)?, &EncodedTerm::id(r(5)?))
    );
    assert_eq!(
        read.properties(),
        &[(r(6)?, year()?), (r(7)?, EncodedTerm::id(r(8)?))]
    );
    Ok(())
}

#[test]
fn test_a_record_is_its_format_written_by_hand() -> TestResult {
    let r = |n| id(TypePrefix::RESOURCE, 0x00_0102, n);
    let record = StatementRecord::new(
        r(1)?,
        r(2)?,
        EncodedTerm::id(r(3)?),
        vec![(r(4)?, EncodedTerm::id(r(5)?))],
    );
    // The format from its layout: version 2; the subject's 16 bytes (type 1, sequence 1, bucket 0x000102); the
    // predicate's; the object as the ID tag 0x01 and its 16 bytes; a u16 count of 1; the property's predicate and its
    // value, an ID encoding.
    let id_bytes = |n: u8| [0, 1, 0, 0, 0, 0, 0, 0, 0, n, 0x00, 0x01, 0x02, 0, 0, 0];
    let mut expected = vec![2];
    expected.extend_from_slice(&id_bytes(1));
    expected.extend_from_slice(&id_bytes(2));
    expected.push(0x01);
    expected.extend_from_slice(&id_bytes(3));
    expected.extend_from_slice(&[0, 1]);
    expected.extend_from_slice(&id_bytes(4));
    expected.push(0x01);
    expected.extend_from_slice(&id_bytes(5));
    assert_eq!(record.encode(), expected);
    assert_eq!(StatementRecord::decode(b"k", &expected)?, record);
    Ok(())
}

#[test]
fn test_a_record_without_properties() -> TestResult {
    let r = |n| id(TypePrefix::RESOURCE, 1, n);
    let record = StatementRecord::new(r(1)?, r(2)?, year()?, Vec::new());
    let bytes = record.encode();
    assert_eq!(
        bytes.get(bytes.len() - 2..),
        Some(&[0u8, 0][..]),
        "a count of 0 ends it"
    );
    assert_eq!(StatementRecord::decode(b"k", &bytes)?, record);
    Ok(())
}

fn refused(bytes: &[u8]) -> TestResult<(Vec<u8>, String)> {
    let error = StatementRecord::decode(b"the key", bytes)
        .err()
        .context("refused")?;
    let QuadErrorEnum::Corrupted { key, reason } = error.0 else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!(
            "not corruption: {error:?}"
        )));
    };
    Ok((key, reason))
}

#[test]
fn test_every_cut_of_a_record_and_a_byte_after_it_are_refused_naming_the_key() -> TestResult {
    let bytes = record()?.encode();
    for end in 0..bytes.len() {
        let (key, _) = refused(bytes.get(..end).context("a prefix")?)?;
        assert_eq!(key, b"the key".to_vec(), "cut at {end}");
    }
    let mut longer = bytes;
    longer.push(0);
    assert!(refused(&longer)?.1.contains("follow"), "a trailing byte");
    Ok(())
}

#[test]
fn test_another_format_is_refused() -> TestResult {
    let mut bytes = record()?.encode();
    for version in [0, 1, 3, 255] {
        if let Some(first) = bytes.first_mut() {
            *first = version;
        }
        assert!(refused(&bytes)?.1.contains("format"), "version {version}");
    }
    Ok(())
}

#[test]
fn test_an_invalid_id_is_refused() -> TestResult {
    let bytes = record()?.encode();
    // Byte 1 starts the subject: a zero type prefix; byte 17 the predicate; byte 34 the object's ID after its tag; the
    // first property's predicate follows the object (1 + 32 + 17) and the count (2).
    for at in [1usize, 17, 52] {
        let mut bad = bytes.clone();
        if let Some(b) = bad.get_mut(at + 1) {
            *b = 0;
        }
        let _refused: (Vec<u8>, String) = refused(&bad)?;
    }
    // A subject with a reserved bit set.
    let mut bad = bytes;
    if let Some(b) = bad.get_mut(16) {
        *b = 1;
    }
    let _refused: (Vec<u8>, String) = refused(&bad)?;
    Ok(())
}
