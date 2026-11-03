/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::{CatalogError, CatalogErrorEnum};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_corrupted_version_names_its_namespace_version_and_reason() -> TestResult {
    let error = CatalogError::corrupted_version(3, 7, "the contents end early");
    assert_eq!(
        error.0,
        CatalogErrorEnum::CorruptedVersion {
            namespace: 3,
            version: 7,
            reason: "the contents end early".to_string(),
        }
    );
    assert_eq!(
        error.to_string(),
        "Catalog version 7 of namespace 3 does not decode: the contents end early"
    );
    Ok(())
}

#[test]
fn test_a_too_large_declaration_names_what_and_how_long() -> TestResult {
    let error = CatalogError::too_large("http://ex.org/r", "roles", 65_536, 65_535);
    assert_eq!(
        error.0,
        CatalogErrorEnum::TooLarge {
            iri: "http://ex.org/r".to_string(),
            what: "roles",
            len: 65_536,
            max: 65_535,
        }
    );
    assert_eq!(
        error.to_string(),
        "The declaration of <http://ex.org/r> holds 65536 roles, at most 65535"
    );
    Ok(())
}

#[test]
fn test_every_refusal_of_a_write_names_what_it_refused() {
    let p = "http://ex.org/p";
    let cases = [
        (
            CatalogError::literal_on_edge(p, "\"Bob\"".to_string()),
            CatalogErrorEnum::LiteralOnEdge {
                predicate: p.to_string(),
                object: "\"Bob\"".to_string(),
            },
            "Predicate <http://ex.org/p> is declared an edge and cannot hold the literal \"Bob\"",
        ),
        (
            CatalogError::resource_on_property(p, "<http://ex.org/b>".to_string()),
            CatalogErrorEnum::ResourceOnProperty {
                predicate: p.to_string(),
                object: "<http://ex.org/b>".to_string(),
            },
            "Predicate <http://ex.org/p> is declared a property and cannot hold <http://ex.org/b>",
        ),
        (
            CatalogError::datatype_mismatch(p, "http://ex.org/int", "http://ex.org/str"),
            CatalogErrorEnum::DatatypeMismatch {
                predicate: p.to_string(),
                expected: "http://ex.org/int".to_string(),
                found: "http://ex.org/str".to_string(),
            },
            "Predicate <http://ex.org/p> holds literals of <http://ex.org/int>, not of <http://ex.org/str>",
        ),
    ];
    for (error, variant, text) in cases {
        assert_eq!(error.0, variant);
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn test_every_refusal_of_a_declaration_names_what_it_refused() {
    let p = "http://ex.org/p";
    let cases = [
        (
            CatalogError::unknown_tokenizer(p, "nope"),
            CatalogErrorEnum::UnknownTokenizer {
                predicate: p.to_string(),
                tokenizer: "nope".to_string(),
            },
            "Predicate <http://ex.org/p> declares the unknown tokenizer nope",
        ),
        (
            CatalogError::tokenizer_datatype(p, "int", "http://ex.org/str"),
            CatalogErrorEnum::TokenizerDatatype {
                predicate: p.to_string(),
                tokenizer: "int".to_string(),
                datatype: "http://ex.org/str".to_string(),
            },
            "Predicate <http://ex.org/p> declares the tokenizer int, which takes no literal of <http://ex.org/str>",
        ),
        (
            CatalogError::unknown_vector_index(p, "annoy"),
            CatalogErrorEnum::UnknownVectorIndex {
                predicate: p.to_string(),
                factory: "annoy".to_string(),
            },
            "Predicate <http://ex.org/p> declares the unknown vector index annoy",
        ),
        (
            CatalogError::second_parent("http://ex.org/E", "http://ex.org/P", "http://ex.org/A"),
            CatalogErrorEnum::SecondParent {
                entity_type: "http://ex.org/E".to_string(),
                parent: "http://ex.org/P".to_string(),
                other: "http://ex.org/A".to_string(),
            },
            "Type <http://ex.org/E> has the parent <http://ex.org/P> and cannot take a second, <http://ex.org/A>",
        ),
        (
            CatalogError::parent_dropped("http://ex.org/E", "http://ex.org/P"),
            CatalogErrorEnum::ParentDropped {
                entity_type: "http://ex.org/E".to_string(),
                parent: "http://ex.org/P".to_string(),
            },
            "Type <http://ex.org/E> has the parent <http://ex.org/P> and cannot be declared without it",
        ),
        (
            CatalogError::parent_cycle("http://ex.org/A", "http://ex.org/C"),
            CatalogErrorEnum::ParentCycle {
                entity_type: "http://ex.org/A".to_string(),
                parent: "http://ex.org/C".to_string(),
            },
            "Type <http://ex.org/A> cannot take the parent <http://ex.org/C>, which descends from it",
        ),
    ];
    for (error, variant, text) in cases {
        assert_eq!(error.0, variant);
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn test_a_store_error_is_carried_and_shown_as_the_stores() {
    let store = StoreError::corrupted(vec![0xAB, 0x01], "not a catalog key".to_string());
    let carried = CatalogError::from(store.clone());
    assert_eq!(carried.0, CatalogErrorEnum::Store(store.clone()));
    assert_eq!(carried.to_string(), store.to_string());
    // A corrupted entry is the store's corruption, naming its key.
    assert_eq!(
        CatalogError::corrupted(&[0xAB, 0x01], "not a catalog key"),
        carried
    );
    assert_eq!(
        std::error::Error::source(&carried).map(ToString::to_string),
        None
    );
}
