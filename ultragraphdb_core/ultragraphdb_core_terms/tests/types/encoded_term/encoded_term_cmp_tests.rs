/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ultragraphdb_core_terms::EncodedTerm;

fn hash_of<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

#[test]
fn test_a_term_hashes_as_its_encoding_bytes() {
    // Hashing by the encoding keeps Hash consistent with Eq, which compares encodings: the term hashes exactly as the
    // byte slice `as_bytes` returns, under the same hasher.
    let a = EncodedTerm::uid(5);
    let b = EncodedTerm::uid(6);
    assert_eq!(hash_of(&a), hash_of(a.as_bytes()));
    assert_eq!(hash_of(&b), hash_of(b.as_bytes()));
    assert_ne!(hash_of(&a), hash_of(&b));
}
