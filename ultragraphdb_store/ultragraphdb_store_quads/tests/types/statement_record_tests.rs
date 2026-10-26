/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Literal;
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_quads::StatementRecord;

#[test]
fn test_a_record_round_trips_and_a_cut_one_is_refused() {
    let value = EncodedTerm::literal(Literal::from(2020).as_ref(), |_| Ok::<_, TermError>(0))
        .unwrap()
        .into_term();
    let mut record = StatementRecord::new(3, 4, EncodedTerm::uid(5), vec![(6, value.clone())]);
    record.properties_mut().push((7, EncodedTerm::uid(8)));
    let bytes = record.encode();
    let read = StatementRecord::decode(b"k", &bytes).unwrap();
    assert_eq!(read, record);
    assert_eq!(
        (read.subject(), read.predicate(), read.object()),
        (3, 4, &EncodedTerm::uid(5))
    );
    assert_eq!(read.properties(), &[(6, value), (7, EncodedTerm::uid(8))]);
    for end in 0..bytes.len() {
        assert!(
            StatementRecord::decode(b"k", &bytes[..end]).is_err(),
            "cut at {end}"
        );
    }
}
