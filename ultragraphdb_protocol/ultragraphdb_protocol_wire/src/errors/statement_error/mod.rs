/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod statement_error_display;
mod statement_error_error;

use crate::types::column_type::ColumnType;
use crate::types::error_code::ErrorCode;
use crate::types::error_reply::ErrorReply;

/// Why a prepare or an execution was refused by its command's own rules: an error reply on the request's stream,
/// after which the connection stays open.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StatementErrorEnum {
    /// The connection holds no statement `statement`: never prepared, or evicted.
    Unknown {
        /// The statement's number on the connection.
        statement: u64,
    },
    /// Statement `statement` was prepared against schema id `prepared`; its tenant's is now `current`.
    Stale {
        /// The statement's number on the connection.
        statement: u64,
        /// The schema id the statement was prepared against.
        prepared: u64,
        /// The tenant's schema id now.
        current: u64,
    },
    /// Statement `statement` takes `expected` parameters; the execution bound `found`.
    Parameters {
        /// The statement's number on the connection.
        statement: u64,
        /// The parameters the statement takes.
        expected: usize,
        /// The parameters the execution bound.
        found: usize,
    },
    /// Parameter `parameter` is declared `declared`, which its slot, of type `slot`, does not take.
    ParameterType {
        /// The parameter's index.
        parameter: usize,
        /// The type the parameter is declared.
        declared: ColumnType,
        /// The type of the slot it fills.
        slot: ColumnType,
    },
    /// Parameter `parameter` of statement `statement` is bound to a literal or a triple term at a slot taking a node.
    NotANode {
        /// The statement's number on the connection.
        statement: u64,
        /// The parameter's index.
        parameter: usize,
    },
}

/// Why a statement was refused; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatementError(pub StatementErrorEnum);

impl StatementError {
    /// The connection holds no statement `statement`.
    pub fn unknown(statement: u64) -> Self {
        Self(StatementErrorEnum::Unknown { statement })
    }

    /// Statement `statement` was prepared against schema id `prepared`; its tenant's is now `current`.
    pub fn stale(statement: u64, prepared: u64, current: u64) -> Self {
        Self(StatementErrorEnum::Stale {
            statement,
            prepared,
            current,
        })
    }

    /// Statement `statement` takes `expected` parameters; the execution bound `found`.
    pub fn parameters(statement: u64, expected: usize, found: usize) -> Self {
        Self(StatementErrorEnum::Parameters {
            statement,
            expected,
            found,
        })
    }

    /// Parameter `parameter` is declared `declared`, which its slot, of type `slot`, does not take.
    pub fn parameter_type(parameter: usize, declared: ColumnType, slot: ColumnType) -> Self {
        Self(StatementErrorEnum::ParameterType {
            parameter,
            declared,
            slot,
        })
    }

    /// Parameter `parameter` of statement `statement` is bound to a term that is not a node at a slot taking one.
    pub fn not_a_node(statement: u64, parameter: usize) -> Self {
        Self(StatementErrorEnum::NotANode {
            statement,
            parameter,
        })
    }

    /// The Error frame's code: [`ErrorCode::UnknownStatement`], [`ErrorCode::StaleStatement`] or
    /// [`ErrorCode::Parameters`].
    pub fn code(&self) -> ErrorCode {
        match self.0 {
            StatementErrorEnum::Unknown { .. } => ErrorCode::UnknownStatement,
            StatementErrorEnum::Stale { .. } => ErrorCode::StaleStatement,
            StatementErrorEnum::Parameters { .. }
            | StatementErrorEnum::ParameterType { .. }
            | StatementErrorEnum::NotANode { .. } => ErrorCode::Parameters,
        }
    }

    /// The error reply: the code, the statement as its detail (zero for a parameter type), and the error's text.
    pub fn reply(&self) -> ErrorReply {
        let statement = match self.0 {
            StatementErrorEnum::Unknown { statement }
            | StatementErrorEnum::Stale { statement, .. }
            | StatementErrorEnum::Parameters { statement, .. }
            | StatementErrorEnum::NotANode { statement, .. } => statement,
            StatementErrorEnum::ParameterType { .. } => 0,
        };
        ErrorReply::new(self.code(), self.to_string()).with_detail(statement)
    }
}
