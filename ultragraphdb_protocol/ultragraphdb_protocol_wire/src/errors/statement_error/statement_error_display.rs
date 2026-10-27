/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::statement_error::{StatementError, StatementErrorEnum};

impl fmt::Display for StatementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            StatementErrorEnum::Unknown { statement } => {
                write!(f, "no statement {statement} is prepared in this session")
            }
            StatementErrorEnum::Stale {
                statement,
                prepared,
                current,
            } => write!(
                f,
                "statement {statement} is stale: prepared against schema {prepared:#018x}, the namespace's is now \
                 {current:#018x}"
            ),
            StatementErrorEnum::Parameters {
                statement,
                expected,
                found,
            } => write!(
                f,
                "statement {statement} takes {expected} parameters, not {found}"
            ),
            StatementErrorEnum::ParameterType {
                parameter,
                declared,
                slot,
            } => write!(
                f,
                "parameter {parameter} is declared {declared:?}, which its slot, of type {slot:?}, does not take"
            ),
        }
    }
}
