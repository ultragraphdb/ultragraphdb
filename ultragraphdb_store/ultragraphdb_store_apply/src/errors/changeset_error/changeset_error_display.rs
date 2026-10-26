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
            ChangesetErrorEnum::Hnsw(e) => write!(f, "{e}"),
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
            ChangesetErrorEnum::Contradicted {
                predicate,
                subject,
                reason,
            } => write!(
                f,
                "The declaration of <{predicate}> is refused by its data: {subject} {reason}"
            ),
            ChangesetErrorEnum::Rebuilding { predicate } => write!(
                f,
                "An index is being built, so <{predicate}> cannot be declared now. Please retry"
            ),
            ChangesetErrorEnum::VectorOption { factory, error } => {
                write!(f, "IndexFactory: {factory} issues this error: '{error}'")
            }
            ChangesetErrorEnum::Dimension {
                predicate,
                held,
                written,
            } => write!(
                f,
                "Predicate <{predicate}> indexes vectors of {held} elements, and a vector of {written} was written"
            ),
        }
    }
}
