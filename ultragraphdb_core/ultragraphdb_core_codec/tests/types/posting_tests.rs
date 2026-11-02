/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Facet, Posting, PostingType};
use ultragraphdb_tool_bdd::TestResult;

use crate::codec_tests::id;

/// A posting with a distinct value in every field, so that a builder or getter crossing two fields fails.
fn every_field() -> TestResult<Posting> {
    Ok(Posting::default()
        .with_uid(id(1, 3, 11)?.to_u128())
        .with_value(b"v".to_vec())
        .with_posting_type(PostingType::Value)
        .with_lang_tag(b"en".to_vec())
        .with_facets(vec![Facet::new("f")])
        .with_op(3)
        .with_start_ts(13)
        .with_commit_ts(17)
        .with_statement_ids(vec![id(2, 3, 19)?, id(2, 3, 23)?]))
}

#[test]
fn test_a_posting_returns_what_it_was_built_with() -> TestResult {
    let p = every_field()?;
    assert_eq!(p.uid(), id(1, 3, 11)?.to_u128());
    assert_eq!(p.value(), b"v");
    assert_eq!(p.posting_type(), PostingType::Value);
    assert_eq!(p.lang_tag(), b"en");
    assert_eq!(p.facets(), &[Facet::new("f")]);
    assert_eq!(p.op(), 3);
    assert_eq!(p.start_ts(), 13);
    assert_eq!(p.commit_ts(), 17);
    assert_eq!(p.statement_ids(), &[id(2, 3, 19)?, id(2, 3, 23)?]);
    Ok(())
}

#[test]
fn test_a_fingerprint_key_sorts_below_every_system_id() -> TestResult {
    // A value posting's key is a 64-bit fingerprint; the smallest system ID is 2^112 + 2^48.
    let fingerprint = Posting::default().with_uid(u128::from(u64::MAX));
    let reference = Posting::default().with_uid(id(1, 0, 1)?.to_u128());
    assert!(fingerprint.uid() < reference.uid());
    Ok(())
}

#[test]
fn test_the_default_posting_is_an_empty_reference() {
    let p = Posting::default();
    assert_eq!((p.uid(), p.op(), p.start_ts(), p.commit_ts()), (0, 0, 0, 0));
    assert_eq!(p.posting_type(), PostingType::Ref);
    assert!(p.value().is_empty() && p.lang_tag().is_empty());
    assert!(p.facets().is_empty() && p.statement_ids().is_empty());
}

#[test]
fn test_a_setter_changes_its_field_alone() -> TestResult {
    let mut p = every_field()?;
    p.set_op(1);
    p.set_start_ts(29);
    p.set_commit_ts(31);
    assert_eq!((p.op(), p.start_ts(), p.commit_ts()), (1, 29, 31));
    assert_eq!(
        p,
        every_field()?
            .with_op(1)
            .with_start_ts(29)
            .with_commit_ts(31)
    );
    assert_ne!(p, every_field()?);
    Ok(())
}
