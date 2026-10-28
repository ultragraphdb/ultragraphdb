/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::OperationResult;

use crate::errors::commit_error::CommitError;
use crate::traits::operator::Operator;
use crate::types::no_operations::NoOperations;

impl<P> Operator<P> for NoOperations {
    type Staged = ();

    fn begin_round(&mut self) {}

    fn operate<'p, I>(
        &mut self,
        _proposal: &P,
        _unapplied: I,
        _results: &mut Vec<OperationResult>,
    ) -> Result<(), CommitError>
    where
        I: Iterator<Item = &'p P>,
        P: 'p,
    {
        Err(CommitError::Refused(
            "the commit step runs no commit-time operations".to_string(),
        ))
    }

    fn staged_record<'s>(&self, _staged: &'s ()) -> &'s [u8] {
        &[]
    }

    fn adopt(&mut self, _proposal: &mut P, _staged: ()) {}

    fn admitted(&mut self, _proposal: &P) {}
}
