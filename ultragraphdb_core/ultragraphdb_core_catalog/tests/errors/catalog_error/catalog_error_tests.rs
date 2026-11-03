/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::{CatalogError, CatalogErrorEnum};
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
