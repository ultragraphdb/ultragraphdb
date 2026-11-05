/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion, TestCatalogSource};

/// The contents in effect for each epoch of `epochs`, read through the trait alone, as a partition reads them.
fn contents_at<S: CatalogSource>(
    source: &S,
    tenant: u64,
    epochs: &[u64],
) -> Result<Vec<Option<Vec<u8>>>, S::Error> {
    epochs
        .iter()
        .map(|e| {
            source
                .version_at(tenant, *e)
                .map(|v| v.map(|v| v.contents().to_vec()))
        })
        .collect()
}

#[test]
fn test_a_partition_reads_the_version_in_effect_through_the_trait() -> TestResult {
    let source = TestCatalogSource::new();
    source.commit(HeldVersion::new(0, 2, 40, vec![2]));
    source.commit(HeldVersion::new(0, 3, 50, vec![3]));
    assert_eq!(
        contents_at(&source, 0, &[39, 40, 49, 50, 51])?,
        vec![
            None,
            Some(vec![2]),
            Some(vec![2]),
            Some(vec![3]),
            Some(vec![3])
        ]
    );
    Ok(())
}
