/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::changeset_error::ChangesetError;
use crate::types::executed::Executed;

/// A catalog version's taking effect in a partition ([`crate::ChangesetApplier::execute_catalog`]): the execution its
/// round applies, which installs the version and starts the build it asks for, and the declarations the partition's
/// data contradicts, each refused, the predicate keeping its previous declaration, for the metadata group to restore.
#[derive(Debug)]
pub struct CatalogExecution {
    executed: Executed,
    refusals: Vec<ChangesetError>,
}

impl CatalogExecution {
    pub(crate) fn new(executed: Executed, refusals: Vec<ChangesetError>) -> Self {
        Self { executed, refusals }
    }

    /// The declarations the data contradicts, each naming its predicate and the subject or object at fault.
    #[must_use]
    pub fn refusals(&self) -> &[ChangesetError] {
        &self.refusals
    }

    /// The execution, for the commit step to apply in a round of its own.
    #[must_use]
    pub fn into_executed(self) -> Executed {
        self.executed
    }
}
