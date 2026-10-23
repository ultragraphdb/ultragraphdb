/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_apply::{ApplyError, ApplyErrorEnum};

#[test]
fn test_the_texts_are_dgraphs() {
    let cases = [
        (
            ApplyError::Aborted(),
            "Transaction has been aborted. Please retry",
        ),
        (
            ApplyError::NoSchema("0-name"),
            "runMutation: Unable to find schema for 0-name",
        ),
        (ApplyError::NeverReach(), "We should never reach here"),
        (
            ApplyError::EmptyPredicate(1, 2, b"ab".to_vec()),
            "Predicate cannot be empty for edge with subject: [1], object: [2] and value: [[97 98]]",
        ),
        (ApplyError::InvalidUid(), "invalid UID with value 0"),
        (
            ApplyError::TsTooOld("Add mutation count index"),
            "Add mutation count index: Transaction is too old",
        ),
        (
            ApplyError::UidIsScalar("name", "entity:1"),
            r#"Input for predicate "name" of type uid is scalar. Edge: entity:1"#,
        ),
        (
            ApplyError::PasswordChange("STRING", "PASSWORD"),
            "Schema change not allowed from STRING to PASSWORD",
        ),
        (
            ApplyError::ListToScalar("string", "string", "tag"),
            "Schema change not allowed from [string] => string without deleting pred: tag",
        ),
        (
            ApplyError::ScalarUid("friend"),
            "Schema change not allowed from scalar to uid or vice versa while there is data for pred: friend",
        ),
        (
            ApplyError::ListToScalarEmpty("tag"),
            "Type can't be changed from list to scalar for attr: [tag] without dropping it first.",
        ),
        (
            ApplyError::VectorOption("hnsw", "bad"),
            "IndexFactory: hnsw issues this error: 'bad'",
        ),
        (
            ApplyError::DropUniqueIndex(vec!["hash".into(), "exact".into()], "id"),
            "could not drop index [hash exact] from [id] predicate when @unique directive specified",
        ),
        (
            ApplyError::IndexingInProgress(),
            "errIndexingInProgress. Please retry",
        ),
        (
            ApplyError::Mutations(vec![ApplyError::InvalidUid(), ApplyError::NeverReach()]),
            "Got error while running mutation: invalid UID with value 0: We should never reach here",
        ),
    ];
    for (error, text) in cases {
        assert_eq!(error.to_string(), text);
    }
}

#[test]
fn test_only_an_abort_is_an_abort() {
    assert!(ApplyError::Aborted().is_aborted());
    assert!(!ApplyError::NeverReach().is_aborted());
    assert!(matches!(
        ApplyError::StartTsRequired().0,
        ApplyErrorEnum::StartTsRequired
    ));
}
