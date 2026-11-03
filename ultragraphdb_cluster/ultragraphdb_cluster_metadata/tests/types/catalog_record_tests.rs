/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::CatalogRecord;
use ultragraphdb_core_ids::{Epoch, Namespace};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_record_holds_its_fields() -> TestResult {
    let record = CatalogRecord::new(
        Namespace::new(3),
        7,
        Some(Epoch::new(40)?),
        b"bytes".to_vec(),
    );
    assert_eq!(record.namespace(), Namespace::new(3));
    assert_eq!(record.version(), 7);
    assert_eq!(record.effective(), Some(Epoch::new(40)?));
    assert_eq!(record.bytes(), b"bytes");
    let pending = CatalogRecord::new(Namespace::new(3), 8, None, Vec::new());
    assert_eq!(pending.effective(), None);
    assert!(pending.bytes().is_empty());
    Ok(())
}
