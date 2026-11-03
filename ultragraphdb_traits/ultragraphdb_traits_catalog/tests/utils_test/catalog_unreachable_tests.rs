/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_catalog::CatalogUnreachable;

#[test]
fn test_the_unreachable_error_says_what_failed() -> TestResult {
    assert_eq!(
        CatalogUnreachable.to_string(),
        "the metadata group cannot be reached"
    );
    let error: &dyn std::error::Error = &CatalogUnreachable;
    assert!(error.source().is_none());
    Ok(())
}
