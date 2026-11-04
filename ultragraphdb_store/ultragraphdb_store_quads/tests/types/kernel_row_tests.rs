/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_quads::KernelRow;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_row_holds_its_key_and_objects() -> TestResult {
    let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(4)?, Sequence::new(9)?);
    let statement = SystemId::new(TypePrefix::STATEMENT, Bucket::new(4)?, Sequence::new(10)?);
    let objects = vec![(EncodedTerm::id(id), vec![statement])];
    let row = KernelRow::new(vec![0x10, 1], objects.clone());
    assert_eq!(row.key(), &[0x10, 1]);
    assert_eq!(row.objects(), objects.as_slice());
    Ok(())
}
