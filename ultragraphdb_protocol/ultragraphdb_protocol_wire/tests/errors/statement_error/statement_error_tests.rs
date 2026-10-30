/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_protocol_wire::{ColumnType, ErrorCode, StatementError, StatementErrorEnum};

#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        StatementError::Unknown(9).0,
        StatementErrorEnum::Unknown { statement: 9 }
    );
    assert_eq!(
        StatementError::Stale(1, 2, 3).0,
        StatementErrorEnum::Stale {
            statement: 1,
            prepared: 2,
            current: 3
        }
    );
    assert_eq!(
        StatementError::Parameters(1, 2, 3).0,
        StatementErrorEnum::Parameters {
            statement: 1,
            expected: 2,
            found: 3
        }
    );
    assert_eq!(
        StatementError::ParameterType(0, ColumnType::Term, ColumnType::Node).0,
        StatementErrorEnum::ParameterType {
            parameter: 0,
            declared: ColumnType::Term,
            slot: ColumnType::Node
        }
    );
    assert_eq!(
        StatementError::NotANode(4, 1).0,
        StatementErrorEnum::NotANode {
            statement: 4,
            parameter: 1
        }
    );
    assert_ne!(
        StatementError::NotANode(4, 1),
        StatementError::NotANode(1, 4)
    );
    assert_ne!(
        StatementError::Stale(1, 2, 3),
        StatementError::Stale(1, 3, 2)
    );
    assert_ne!(
        StatementError::Parameters(1, 2, 3),
        StatementError::Parameters(1, 3, 2)
    );
}

#[test]
fn test_each_maps_to_its_error_code_and_reply() {
    assert_eq!(
        StatementError::Unknown(9).code(),
        ErrorCode::UnknownStatement
    );
    assert_eq!(
        StatementError::Stale(1, 2, 3).code(),
        ErrorCode::StaleStatement
    );
    assert_eq!(
        StatementError::Parameters(1, 2, 3).code(),
        ErrorCode::Parameters
    );
    assert_eq!(
        StatementError::ParameterType(0, ColumnType::Term, ColumnType::Node).code(),
        ErrorCode::Parameters
    );
    let reply = StatementError::Stale(4, 2, 3).reply();
    assert_eq!(reply.code(), ErrorCode::StaleStatement);
    assert_eq!(reply.detail(), 4);
    assert_eq!(reply.message(), StatementError::Stale(4, 2, 3).to_string());
    assert!(!reply.retryable());
    assert_eq!(
        StatementError::ParameterType(0, ColumnType::Term, ColumnType::Node)
            .reply()
            .detail(),
        0
    );
    assert_eq!(StatementError::Unknown(7).reply().detail(), 7);
    assert_eq!(StatementError::NotANode(4, 1).code(), ErrorCode::Parameters);
    assert_eq!(StatementError::NotANode(4, 1).reply().detail(), 4);
}

#[test]
fn test_each_names_its_values() {
    assert_eq!(
        StatementError::Unknown(9).to_string(),
        "no statement 9 is prepared on this connection"
    );
    assert_eq!(
        StatementError::Stale(1, 0x10, 0x20).to_string(),
        "statement 1 is stale: prepared against schema 0x0000000000000010, the namespace's is now 0x0000000000000020"
    );
    assert_eq!(
        StatementError::Parameters(1, 2, 3).to_string(),
        "statement 1 takes 2 parameters, not 3"
    );
    assert_eq!(
        StatementError::ParameterType(0, ColumnType::Term, ColumnType::Node).to_string(),
        "parameter 0 is declared Term, which its slot, of type Node, does not take"
    );
    assert_eq!(
        StatementError::NotANode(4, 1).to_string(),
        "parameter 1 of statement 4 binds a literal or a triple term at a slot taking a node"
    );
    let error: Box<dyn Error> = Box::new(StatementError::Unknown(1));
    assert!(error.source().is_none());
}
