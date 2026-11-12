/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_quads::{QuadError, Statistics};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_deltas_merge_and_round_trip() -> TestResult {
    let mut a = Statistics::new();
    (0u64..500).for_each(|i| a.inserted(&i.to_be_bytes(), &(i % 50).to_be_bytes(), "iri"));
    let mut b = Statistics::new();
    b.inserted(b"s", b"o", "http://www.w3.org/2001/XMLSchema#integer");
    b.deleted("iri");
    let mut total = a.clone();
    total.merge(&b);
    assert_eq!(total.statements(), 500);
    assert_eq!(
        (
            total.kind("iri"),
            total.kind("http://www.w3.org/2001/XMLSchema#integer")
        ),
        (499, 1)
    );
    // 64 registers estimate with about 13 % standard error.
    assert!(
        (300..=700).contains(&total.distinct_subjects()),
        "{}",
        total.distinct_subjects()
    );
    assert!(
        (30..=70).contains(&total.distinct_objects()),
        "{}",
        total.distinct_objects()
    );
    let bytes = total.encode()?;
    assert_eq!(Statistics::decode(b"k", &bytes), Ok(total.clone()));
    for end in 0..bytes.len() {
        assert!(Statistics::decode(b"k", &bytes[..end]).is_err());
    }
    assert!(Statistics::new().is_empty() && Statistics::new().distinct_subjects() == 0);
    Ok(())
}

#[test]
fn test_a_kind_past_its_u16_length_is_refused() -> TestResult {
    let longest = usize::from(u16::MAX);
    let mut fits = Statistics::new();
    fits.inserted(b"s", b"o", &"k".repeat(longest));
    let bytes = fits.encode()?;
    assert_eq!(Statistics::decode(b"k", &bytes), Ok(fits));
    let mut past = Statistics::new();
    past.inserted(b"s", b"o", &"k".repeat(longest + 1));
    assert_eq!(past.encode(), Err(QuadError::kind_too_long(longest + 1)));
    Ok(())
}

#[test]
fn test_kinds_past_their_u16_count_are_refused() -> TestResult {
    let longest = usize::from(u16::MAX);
    let mut fits = Statistics::new();
    (0..longest).for_each(|i| fits.inserted(b"s", b"o", &i.to_string()));
    assert_eq!(fits.kinds().len(), longest);
    let bytes = fits.encode()?;
    assert_eq!(Statistics::decode(b"k", &bytes), Ok(fits.clone()));
    let mut past = fits;
    past.inserted(b"s", b"o", "one more");
    assert_eq!(past.encode(), Err(QuadError::too_many_kinds(longest + 1)));
    Ok(())
}
