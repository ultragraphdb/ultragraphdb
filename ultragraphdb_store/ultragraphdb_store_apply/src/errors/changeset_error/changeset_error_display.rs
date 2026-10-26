/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::changeset_error::{ChangesetError, ChangesetErrorEnum};

impl fmt::Display for ChangesetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ChangesetErrorEnum::Catalog(e) => write!(f, "{e}"),
            ChangesetErrorEnum::Mapping(e) => write!(f, "{e}"),
            ChangesetErrorEnum::Quad(e) => write!(f, "{e}"),
            ChangesetErrorEnum::Posting(e) => write!(f, "{e}"),
            ChangesetErrorEnum::Store(e) => write!(f, "{e}"),
            ChangesetErrorEnum::Tok(e) => write!(f, "{e}"),
            ChangesetErrorEnum::Literal { predicate, error } => {
                write!(
                    f,
                    "Predicate <{predicate}> cannot hold the literal: {error}"
                )
            }
            ChangesetErrorEnum::Unique {
                predicate,
                value,
                holder,
            } => write!(
                f,
                "Predicate <{predicate}> holds unique values, and {holder} already holds {value}"
            ),
        }
    }
}
