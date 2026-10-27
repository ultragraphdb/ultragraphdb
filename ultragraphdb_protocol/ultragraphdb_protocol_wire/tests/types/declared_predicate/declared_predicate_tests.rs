/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{DeclaredKind, DeclaredPredicate};

#[test]
fn test_a_predicate_declares_nothing_but_its_name_until_told() {
    let bare = DeclaredPredicate::new("http://ex.org/p");
    assert_eq!(bare.predicate(), "http://ex.org/p");
    assert_eq!(bare.kind(), DeclaredKind::Either);
    assert!(!bare.single() && !bare.unique() && !bare.count_index());
    assert_eq!(bare.datatype(), None);
    assert!(bare.tokenizers().is_empty() && bare.domains().is_empty());
    assert_eq!(bare.vector_index(), None);
    let full = DeclaredPredicate::new("p")
        .with_kind(DeclaredKind::Property)
        .with_single(true)
        .with_unique(true)
        .with_datatype("d")
        .with_tokenizers(vec!["term".to_string()])
        .with_count_index(true)
        .with_domains(vec!["T".to_string()])
        .with_vector_index("hnsw", vec![("metric".to_string(), "cosine".to_string())]);
    assert_eq!(full.kind(), DeclaredKind::Property);
    assert!(full.single() && full.unique() && full.count_index());
    assert_eq!(full.datatype(), Some("d"));
    assert_eq!(full.tokenizers(), &["term".to_string()]);
    assert_eq!(full.domains(), &["T".to_string()]);
    assert_eq!(
        full.vector_index(),
        Some(&(
            "hnsw".to_string(),
            vec![("metric".to_string(), "cosine".to_string())]
        ))
    );
    assert_ne!(full, bare);
}
