/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vendored toml-test documents and the fuzz seeds.

use std::fs;

use ultragraphdb_tool_bdd::{Context, TestResult, testdata_dir};
use ultragraphdb_tool_toml::TomlDocument;

use crate::utils::document::reread_alike;
use crate::utils::toml_test_suite::{
    INVALID_DOCUMENTS, VALID_DOCUMENTS, check_invalid, check_valid, files, invalid_cases,
    valid_cases,
};

#[test]
fn test_every_valid_document_of_the_subset_parses_to_its_expected_json() -> TestResult {
    let cases = valid_cases()?;
    assert_eq!(cases.len(), VALID_DOCUMENTS);
    for (path, bytes, expected) in &cases {
        check_valid(path, bytes, expected)?;
    }
    Ok(())
}

#[test]
fn test_every_invalid_document_is_refused_as_unsupported_or_invalid() -> TestResult {
    let cases = invalid_cases()?;
    assert_eq!(cases.len(), INVALID_DOCUMENTS);
    for (path, bytes) in &cases {
        check_invalid(path, bytes)?;
    }
    Ok(())
}

#[test]
fn test_the_fuzz_seeds_are_the_valid_documents_and_parse() -> TestResult {
    let seeds = files(&testdata_dir()?.join("fuzz_seeds").join("document"), "toml")?;
    assert_eq!(seeds.len(), VALID_DOCUMENTS);
    for seed in &seeds {
        let bytes = fs::read(seed).context("a seed")?;
        let parsed = TomlDocument::parse(&bytes);
        assert!(parsed.is_ok(), "{}: {parsed:?}", seed.display());
    }
    Ok(())
}

#[test]
fn test_every_valid_document_of_the_subset_rereads_alike() -> TestResult {
    for (path, bytes, _) in &valid_cases()? {
        let document = TomlDocument::parse(bytes).context(&path.display().to_string())?;
        reread_alike(document)?;
    }
    Ok(())
}
