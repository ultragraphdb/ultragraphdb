/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::RecordOperation;
use ultragraphdb_tool_bdd::TestResult;

use crate::types::write_set_record::write_set_record_tests::sid;

/// The 16 big-endian bytes of the system ID of prefix 1, sequence 4 and bucket 0x2A, from PROTOCOL.md §1's table.
const ID_4: [u8; 16] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0x2A, 0, 0, 0];

/// A catalog version's argument: the u64 little-endian, then 8 zero bytes.
fn catalog(version: u64) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&version.to_le_bytes());
    out
}

#[test]
fn test_each_operation_has_its_code_and_argument() -> TestResult {
    let id = sid(1, 4, 0x2A)?;
    let cases = [
        (RecordOperation::DropPredicate(id), 1u16, ID_4, true),
        (RecordOperation::DropGraph(id), 2, ID_4, true),
        (RecordOperation::DropNamespaceData, 3, [0; 16], true),
        (RecordOperation::DropNamespace, 4, [0; 16], true),
        (RecordOperation::DropAll, 5, [0; 16], true),
        (RecordOperation::StartBuild(12), 6, catalog(12), false),
        (
            RecordOperation::CompleteBuild(u64::MAX),
            7,
            catalog(u64::MAX),
            false,
        ),
    ];
    for (operation, code, argument, drop) in cases {
        assert_eq!(operation.code(), code);
        assert_eq!(operation.argument(), argument);
        assert_eq!(operation.is_drop(), drop);
        assert_eq!(RecordOperation::from_parts(code, argument), Some(operation));
        let mut expected = code.to_le_bytes().to_vec();
        expected.extend_from_slice(&[0; 6]);
        expected.extend_from_slice(&argument);
        assert_eq!(operation.encode().to_vec(), expected);
    }
    // A build of catalog version 0 takes the zero argument.
    assert_eq!(
        RecordOperation::from_parts(6, [0; 16]),
        Some(RecordOperation::StartBuild(0))
    );
    Ok(())
}

#[test]
fn test_a_code_naming_no_operation_is_none() {
    for code in [0u16, 8, u16::MAX] {
        assert_eq!(RecordOperation::from_parts(code, [0; 16]), None);
        assert_eq!(RecordOperation::from_parts(code, ID_4), None);
    }
}

#[test]
fn test_an_argument_the_code_does_not_take_is_none() {
    // A drop of a predicate or graph names a valid system ID: not one of type prefix 0, sequence 0 or reserved bytes
    // set, nor zero.
    let mut prefix_zero = ID_4;
    prefix_zero[1] = 0;
    let mut sequence_zero = ID_4;
    sequence_zero[9] = 0;
    let mut reserved = ID_4;
    reserved[15] = 1;
    for code in [1u16, 2] {
        for argument in [prefix_zero, sequence_zero, reserved, [0; 16]] {
            assert_eq!(RecordOperation::from_parts(code, argument), None);
        }
    }
    // The drops of a namespace's data, of everything in it and of everything take none.
    for code in [3u16, 4, 5] {
        assert_eq!(RecordOperation::from_parts(code, ID_4), None);
        let mut last = [0u8; 16];
        last[15] = 1;
        assert_eq!(RecordOperation::from_parts(code, last), None);
    }
    // A build's catalog version leaves bytes 8 to 15 zero.
    for code in [6u16, 7] {
        let mut set = catalog(12);
        set[8] = 1;
        assert_eq!(RecordOperation::from_parts(code, set), None);
        let mut last = catalog(12);
        last[15] = 1;
        assert_eq!(RecordOperation::from_parts(code, last), None);
    }
}
