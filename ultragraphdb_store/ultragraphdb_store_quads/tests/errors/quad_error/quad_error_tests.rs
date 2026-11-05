/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{IdError, TypePrefix};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_each_constructor_builds_its_variant() -> TestResult {
    assert_eq!(
        QuadError::no_sequence(TypePrefix::STATEMENT).0,
        QuadErrorEnum::NoSequence { prefix: 2 }
    );
    assert_eq!(
        QuadError::pending_full(9).0,
        QuadErrorEnum::PendingFull { capacity: 9 }
    );
    assert_eq!(
        QuadError::no_owned_bucket(4).0,
        QuadErrorEnum::NoOwnedBucket { tenant: 4 }
    );
    assert_eq!(
        QuadError::not_owned("http://ex.org/b", 0xC7_0001).0,
        QuadErrorEnum::NotOwned {
            iri: "http://ex.org/b".to_string(),
            bucket: 0xC7_0001
        }
    );
    assert_eq!(
        QuadError::skolem_draws(4).0,
        QuadErrorEnum::SkolemDraws { draws: 4 }
    );
    assert_eq!(
        QuadError::unknown_skolem("i").0,
        QuadErrorEnum::UnknownSkolem {
            iri: "i".to_string()
        }
    );
    assert_eq!(
        QuadError::corrupted(b"k", "why").0,
        QuadErrorEnum::Corrupted {
            key: b"k".to_vec(),
            reason: "why".to_string()
        }
    );
    assert_eq!(
        QuadError::from(IdError::zero_sequence()).0,
        QuadErrorEnum::Id(IdError::zero_sequence())
    );
    Ok(())
}

#[test]
fn test_each_text_names_what_it_refused() {
    let cases = [
        (
            QuadError::not_owned("http://ex.org/b", 12),
            "http://ex.org/b is of bucket 12, another partition's, and the partition holds no ID for it",
        ),
        (
            QuadError::no_sequence(TypePrefix::RESOURCE),
            "No sequence of Resource is held, and the block source leased no block",
        ),
        (
            QuadError::no_sequence(TypePrefix::STATEMENT),
            "No sequence of Statement is held, and the block source leased no block",
        ),
        (
            QuadError::pending_full(3),
            "3 new IDs await their commit, all the dictionary holds: retry once a commit writes them",
        ),
        (
            QuadError::no_owned_bucket(5),
            "The partition owns no bucket of tenant 5's run, so it mints no blank node of it",
        ),
        (
            QuadError::skolem_draws(4),
            "4 skolem IRIs drawn in turn each named an IRI the dictionary holds",
        ),
        (
            QuadError::unknown_skolem("http://x/.well-known/genid/000001-0"),
            "http://x/.well-known/genid/000001-0 lies under the skolem base, and the dictionary holds no blank node or reifier by it",
        ),
        (
            QuadError::corrupted(&[0x1B, 0xAB], "lost"),
            "Corrupted dictionary at key 1bab: lost",
        ),
    ];
    for (error, text) in &cases {
        assert_eq!(error.to_string(), *text);
    }
    assert_eq!(
        QuadError::from(IdError::zero_sequence()).to_string(),
        IdError::zero_sequence().to_string()
    );
}

#[test]
fn test_a_declared_type_is_named_by_its_prefix() -> TestResult {
    let error = QuadError::no_sequence(TypePrefix::new(300)?);
    assert_eq!(
        error.to_string(),
        "No sequence of type 300 is held, and the block source leased no block"
    );
    assert!(std::error::Error::source(&error).is_none());
    Ok(())
}

#[test]
fn test_a_carried_error_reads_as_its_own_text() -> TestResult {
    use ultragraphdb_core_catalog::CatalogError;
    use ultragraphdb_core_keys::QuadKey;
    use ultragraphdb_core_terms::TermError;
    use ultragraphdb_error_store::StoreError;
    use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};
    let key = QuadKey::parse(&[0x42])
        .err()
        .ok_or_else(|| ultragraphdb_tool_bdd::TestError::new("refused"))?;
    let carried: [(QuadError, String); 5] = [
        (
            StoreError::poisoned().into(),
            StoreError::poisoned().to_string(),
        ),
        (
            PostingError(PostingErrorEnum::HighPriorityOp).into(),
            PostingError(PostingErrorEnum::HighPriorityOp).to_string(),
        ),
        (
            TermError::unknown_tag(0x7f).into(),
            TermError::unknown_tag(0x7f).to_string(),
        ),
        (key.clone().into(), key.to_string()),
        (
            CatalogError::literal_on_edge("p", "x".into()).into(),
            CatalogError::literal_on_edge("p", "x".into()).to_string(),
        ),
    ];
    for (error, text) in carried {
        assert_eq!(error.to_string(), text);
    }
    Ok(())
}
