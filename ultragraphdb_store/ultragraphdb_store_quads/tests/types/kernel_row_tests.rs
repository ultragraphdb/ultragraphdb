/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;
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

/// Offered objects gather into rows by key, after whatever `out` already held, even a row of the same key.
#[test]
fn test_a_gathering_sink_takes_every_object_into_rows_of_its_key() -> TestResult {
    let id = |n| -> TestResult<SystemId> {
        Ok(SystemId::new(
            TypePrefix::RESOURCE,
            Bucket::new(4)?,
            Sequence::new(n)?,
        ))
    };
    let (a, b, c) = (
        EncodedTerm::id(id(1)?),
        EncodedTerm::id(id(2)?),
        EncodedTerm::id(id(3)?),
    );
    let statement = id(9)?;
    let earlier = KernelRow::new(vec![1], Vec::new());
    let mut out = vec![earlier.clone()];
    {
        let mut sink = KernelRow::gather(&mut out);
        assert_eq!(sink(&[1], &a, &[statement]), ControlFlow::Continue(()));
        assert_eq!(sink(&[1], &b, &[]), ControlFlow::Continue(()));
        assert_eq!(sink(&[2], &c, &[]), ControlFlow::Continue(()));
    }
    assert_eq!(
        out,
        vec![
            earlier,
            KernelRow::new(vec![1], vec![(a, vec![statement]), (b, Vec::new())]),
            KernelRow::new(vec![2], vec![(c, Vec::new())]),
        ]
    );
    Ok(())
}
