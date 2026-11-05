/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BuildOutcome, RecordOperation};
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

/// A build's completion argument (`PROTOCOL.md` §12.2, operation 7): the version, then the outcome's byte.
fn completion(version: u64, outcome: u8) -> [u8; 16] {
    let mut out = catalog(version);
    out[8] = outcome;
    out
}

#[test]
fn test_each_operation_has_its_code_and_argument() -> TestResult {
    let id = sid(1, 4, 0x2A)?;
    let cases = [
        (RecordOperation::DropPredicate(id), 1u16, ID_4, true),
        (RecordOperation::DropGraph(id), 2, ID_4, true),
        (RecordOperation::DropTenantData, 3, [0; 16], true),
        (RecordOperation::DropTenant, 4, [0; 16], true),
        (
            RecordOperation::CatalogTakesEffect(12),
            6,
            catalog(12),
            false,
        ),
        (
            RecordOperation::CompleteBuild {
                version: 12,
                outcome: BuildOutcome::Swap,
            },
            7,
            completion(12, 1),
            false,
        ),
        (
            RecordOperation::CompleteBuild {
                version: u64::MAX,
                outcome: BuildOutcome::Undo,
            },
            7,
            completion(u64::MAX, 2),
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
    // Catalog version 0 taking effect takes the zero argument.
    assert_eq!(
        RecordOperation::from_parts(6, [0; 16]),
        Some(RecordOperation::CatalogTakesEffect(0))
    );
    // A completion's outcome is 1 or 2, followed by zero bytes; a version taking effect has no outcome byte.
    for outcome in [0u8, 3, u8::MAX] {
        assert_eq!(
            RecordOperation::from_parts(7, completion(12, outcome)),
            None
        );
    }
    let mut trailing = completion(12, 1);
    trailing[15] = 1;
    assert_eq!(RecordOperation::from_parts(7, trailing), None);
    assert_eq!(RecordOperation::from_parts(6, completion(12, 1)), None);
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
    // The drops of a tenant's data, of everything in it and of everything take none.
    for code in [3u16, 4, 5] {
        assert_eq!(RecordOperation::from_parts(code, ID_4), None);
        let mut last = [0u8; 16];
        last[15] = 1;
        assert_eq!(RecordOperation::from_parts(code, last), None);
    }
    // A catalog version taking effect leaves bytes 8 to 15 zero; a build's completion holds its outcome at byte 8 and
    // leaves bytes 9 to 15 zero (PROTOCOL.md section 12.2).
    let mut set = catalog(12);
    set[8] = 1;
    assert_eq!(RecordOperation::from_parts(6, set), None);
    for (code, outcome) in [(6u16, 0u8), (7, 1)] {
        let mut last = completion(12, outcome);
        last[15] = 1;
        assert_eq!(RecordOperation::from_parts(code, last), None);
    }
}

/// Operation 5 stays unassigned; 6 and 7 keep their numbers.
#[test]
fn test_operation_5_names_nothing() {
    assert_eq!(RecordOperation::from_parts(5, [0; 16]), None);
    assert!(RecordOperation::from_parts(6, catalog(1)).is_some());
    assert!(!RecordOperation::CatalogTakesEffect(1).is_drop());
}
