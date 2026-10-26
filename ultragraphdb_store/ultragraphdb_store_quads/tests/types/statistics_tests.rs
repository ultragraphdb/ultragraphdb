/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_quads::Statistics;

#[test]
fn test_deltas_merge_and_round_trip() {
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
    assert_eq!(Statistics::decode(b"k", &total.encode()), Ok(total.clone()));
    let bytes = total.encode();
    for end in 0..bytes.len() {
        assert!(Statistics::decode(b"k", &bytes[..end]).is_err());
    }
    assert!(Statistics::new().is_empty() && Statistics::new().distinct_subjects() == 0);
}
