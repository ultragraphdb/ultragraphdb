/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Literal;
use ultragraphdb_core_codec::{Posting, PostingType};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_posting::{DEL, SET};
use ultragraphdb_store_quads::{QuadErrorEnum, object_posting, posting_object};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn fingerprint(_: &[u8]) -> u64 {
    0xFEED
}

fn id(prefix: TypePrefix, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        prefix,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

#[test]
fn test_an_id_object_is_a_reference_posting_keyed_by_its_128_bits() -> TestResult {
    let object = id(TypePrefix::RESOURCE, 0xAB_CDEF, 42)?;
    let statement = id(TypePrefix::STATEMENT, 0xAB_CDEF, 7)?;
    let posting = object_posting(&EncodedTerm::id(object), SET, &[statement], fingerprint);
    // Type 1, sequence 42, bucket 0xABCDEF: 0x0001 << 112 | 42 << 48 | 0xABCDEF << 24, by hand.
    assert_eq!(
        posting.uid(),
        (1u128 << 112) | (42u128 << 48) | (0xAB_CDEFu128 << 24)
    );
    assert_eq!(posting.posting_type(), PostingType::Ref);
    assert!(posting.value().is_empty());
    assert_eq!(posting.op(), SET);
    assert_eq!(posting.statement_ids(), &[statement]);
    assert_eq!(posting_object(b"k", &posting)?, EncodedTerm::id(object));
    Ok(())
}

#[test]
fn test_a_literal_is_a_value_posting_keyed_by_its_fingerprint() -> TestResult {
    let literal = EncodedTerm::literal(Literal::new_simple_literal("Ann").as_ref(), |_| {
        Err(TermError::malformed(
            Vec::new(),
            "a simple literal names no datatype",
        ))
    })?
    .into_term();
    let posting = object_posting(&literal, DEL, &[], fingerprint);
    assert_eq!(posting.uid(), 0xFEED, "below every ID");
    assert_eq!(posting.posting_type(), PostingType::Value);
    assert_eq!(posting.value(), literal.as_bytes());
    assert_eq!(posting.op(), DEL);
    assert!(posting.statement_ids().is_empty());
    assert_eq!(posting_object(b"k", &posting)?, literal);
    Ok(())
}

#[test]
fn test_a_reference_naming_no_valid_id_is_corruption_naming_the_key() -> TestResult {
    // 0, a key below every ID; 1 << 48, a zero type prefix; a reserved bit set under a valid type and sequence.
    for uid in [0u128, 1 << 48, (1u128 << 112) | (1 << 48) | 1] {
        let posting = Posting::default().with_uid(uid);
        let error = posting_object(b"the key", &posting)
            .err()
            .context("refused")?;
        let QuadErrorEnum::Corrupted { key, .. } = error.0 else {
            return Err(ultragraphdb_tool_bdd::TestError::new(format!("{error:?}")));
        };
        assert_eq!(key, b"the key".to_vec());
    }
    Ok(())
}

#[test]
fn test_a_value_holding_no_encoding_is_corruption_naming_the_key() -> TestResult {
    for value in [Vec::new(), vec![0x7F], vec![0x20, 9]] {
        let posting = Posting::default()
            .with_uid(5)
            .with_value(value)
            .with_posting_type(PostingType::Value);
        let error = posting_object(b"the key", &posting)
            .err()
            .context("refused")?;
        let QuadErrorEnum::Corrupted { key, .. } = error.0 else {
            return Err(ultragraphdb_tool_bdd::TestError::new(format!("{error:?}")));
        };
        assert_eq!(key, b"the key".to_vec());
    }
    Ok(())
}
