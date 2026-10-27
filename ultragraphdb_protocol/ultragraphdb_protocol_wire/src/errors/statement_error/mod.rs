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
    /// The session holds no statement `statement`: never prepared, or evicted.
    Unknown { statement: u64 },
    /// Statement `statement` was prepared against schema id `prepared`; its namespace's is now `current`.
    Stale {
        statement: u64,
        prepared: u64,
        current: u64,
    },
    /// Statement `statement` takes `expected` parameters; the execution bound `found`.
    Parameters {
        statement: u64,
        expected: usize,
        found: usize,
    },
    /// Parameter `parameter` is declared `declared`, which its slot, of type `slot`, does not take.
    ParameterType {
        parameter: usize,
        declared: ColumnType,
        slot: ColumnType,
    },
}

/// Why a statement was refused; `.0` classifies it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatementError(pub StatementErrorEnum);

impl StatementError {
    #[allow(non_snake_case)]
    pub fn Unknown(statement: u64) -> Self {
        Self(StatementErrorEnum::Unknown { statement })
    }

    #[allow(non_snake_case)]
    pub fn Stale(statement: u64, prepared: u64, current: u64) -> Self {
        Self(StatementErrorEnum::Stale {
            statement,
            prepared,
            current,
        })
    }

    #[allow(non_snake_case)]
    pub fn Parameters(statement: u64, expected: usize, found: usize) -> Self {
        Self(StatementErrorEnum::Parameters {
            statement,
            expected,
            found,
        })
    }

    #[allow(non_snake_case)]
    pub fn ParameterType(parameter: usize, declared: ColumnType, slot: ColumnType) -> Self {
        Self(StatementErrorEnum::ParameterType {
            parameter,
            declared,
            slot,
        })
    }

    /// The Error frame's code: [`ErrorCode::UnknownStatement`], [`ErrorCode::StaleStatement`] or
    /// [`ErrorCode::Parameters`].
    pub fn code(&self) -> ErrorCode {
        match self.0 {
            StatementErrorEnum::Unknown { .. } => ErrorCode::UnknownStatement,
            StatementErrorEnum::Stale { .. } => ErrorCode::StaleStatement,
            StatementErrorEnum::Parameters { .. } | StatementErrorEnum::ParameterType { .. } => {
                ErrorCode::Parameters
            }
        }
    }

    /// The error reply: the code, the statement as its detail (zero for a parameter type), and the error's text.
    pub fn reply(&self) -> ErrorReply {
        let statement = match self.0 {
            StatementErrorEnum::Unknown { statement }
            | StatementErrorEnum::Stale { statement, .. }
            | StatementErrorEnum::Parameters { statement, .. } => statement,
            StatementErrorEnum::ParameterType { .. } => 0,
        };
        ErrorReply::new(self.code(), self.to_string()).with_detail(statement)
    }
}
