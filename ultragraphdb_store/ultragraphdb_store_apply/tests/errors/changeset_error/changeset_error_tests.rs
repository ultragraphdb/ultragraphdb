/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Every variant of `ChangesetError`: its constructor, its text, and the conversions that wrap another crate's error.
//! The texts are written out from the variants' documented formats, not computed.

use std::error::Error;

use ultragraphdb_core_catalog::CatalogError;
use ultragraphdb_core_mapping::MappingError;
use ultragraphdb_core_terms::TermError;
use ultragraphdb_core_tok::TokError;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_hnsw::HnswError;
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum};

#[test]
fn test_the_constructors_build_their_variants() {
    let literal = ChangesetError::Literal("p", TermError::InvalidVector("[]".into(), "empty"));
    assert_eq!(
        literal.0,
        ChangesetErrorEnum::Literal {
            predicate: "p".into(),
            error: TermError::InvalidVector("[]".into(), "empty"),
        }
    );
    assert_eq!(
        ChangesetError::Unique("p", "\"x\"".into(), "<a>".into()).0,
        ChangesetErrorEnum::Unique {
            predicate: "p".into(),
            value: "\"x\"".into(),
            holder: "<a>".into(),
        }
    );
    assert_eq!(
        ChangesetError::Contradicted("p", "<a>".into(), "holds a literal").0,
        ChangesetErrorEnum::Contradicted {
            predicate: "p".into(),
            subject: "<a>".into(),
            reason: "holds a literal".into(),
        }
    );
    assert_eq!(
        ChangesetError::Rebuilding("p").0,
        ChangesetErrorEnum::Rebuilding {
            predicate: "p".into()
        }
    );
    assert_eq!(
        ChangesetError::VectorOption("hnsw", "bad".into()).0,
        ChangesetErrorEnum::VectorOption {
            factory: "hnsw".into(),
            error: "bad".into(),
        }
    );
    // 768 and 512: two different dimensions, so a swapped pair shows.
    assert_eq!(
        ChangesetError::Dimension("p", 768, 512).0,
        ChangesetErrorEnum::Dimension {
            predicate: "p".into(),
            held: 768,
            written: 512,
        }
    );
}

#[test]
fn test_the_texts_of_the_applier_s_own_variants() {
    let cases = [
        (
            ChangesetError::Literal(
                "http://ex.org/v",
                TermError::InvalidVector("[1.0]".into(), "not in canonical form"),
            ),
            "Predicate <http://ex.org/v> cannot hold the literal: Invalid vector literal \"[1.0]\": not in canonical form",
        ),
        (
            ChangesetError::Unique(
                "http://ex.org/id",
                "\"x\"".into(),
                "<http://ex.org/a>".into(),
            ),
            "Predicate <http://ex.org/id> holds unique values, and <http://ex.org/a> already holds \"x\"",
        ),
        (
            ChangesetError::Contradicted(
                "http://ex.org/nick",
                "<http://ex.org/a>".into(),
                "holds more than one object",
            ),
            "The declaration of <http://ex.org/nick> is refused by its data: <http://ex.org/a> holds more than one object",
        ),
        (
            ChangesetError::Rebuilding("http://ex.org/name"),
            "An index is being built, so <http://ex.org/name> cannot be declared now. Please retry",
        ),
        (
            ChangesetError::VectorOption("hnsw", "no such vector index".into()),
            "IndexFactory: hnsw issues this error: 'no such vector index'",
        ),
        (
            ChangesetError::Dimension("http://ex.org/e", 768, 512),
            "Predicate <http://ex.org/e> indexes vectors of 768 elements, and a vector of 512 was written",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
        assert!(error.source().is_none(), "{error}");
    }
}

#[test]
fn test_a_wrapped_error_converts_to_its_variant_and_shows_its_own_text() {
    let catalog = ChangesetError::from(CatalogError::LiteralOnEdge(
        "http://ex.org/k",
        "\"Bob\"".into(),
    ));
    assert!(matches!(catalog.0, ChangesetErrorEnum::Catalog(_)));
    assert_eq!(
        catalog.to_string(),
        "Predicate <http://ex.org/k> is declared an edge and cannot hold the literal \"Bob\""
    );
    let mapping = ChangesetError::from(MappingError::NoSuchStatement("<a> <p> <b>".into()));
    assert!(matches!(mapping.0, ChangesetErrorEnum::Mapping(_)));
    assert_eq!(mapping.to_string(), "No statement <a> <p> <b> is held");
    let quad = ChangesetError::from(QuadError(QuadErrorEnum::Aborted));
    assert!(matches!(quad.0, ChangesetErrorEnum::Quad(_)));
    assert_eq!(
        quad.to_string(),
        "Transaction has been aborted. Please retry"
    );
    let posting = ChangesetError::from(PostingError(PostingErrorEnum::HighPriorityOp));
    assert!(matches!(posting.0, ChangesetErrorEnum::Posting(_)));
    assert_eq!(
        posting.to_string(),
        "Cancelled rollup to make way for high priority operation"
    );
    let store = ChangesetError::from(StoreError(StoreErrorEnum::Poisoned));
    assert!(matches!(store.0, ChangesetErrorEnum::Store(_)));
    assert_eq!(
        store.to_string(),
        "The store is poisoned by an earlier failed write and must be reopened"
    );
    let tok = ChangesetError::from(TokError::WrongValue("wrong".into()));
    assert!(matches!(tok.0, ChangesetErrorEnum::Tok(_)));
    assert_eq!(tok.to_string(), "wrong");
    let hnsw = ChangesetError::from(HnswError::NoValue());
    assert!(matches!(hnsw.0, ChangesetErrorEnum::Hnsw(_)));
    assert_eq!(hnsw.to_string(), "No value found");
}
