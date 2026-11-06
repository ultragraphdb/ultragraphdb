/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;

/// What a read drops of the list or record it resumes in: the row it stopped after and those before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Skip {
    /// In the list of `key`, the object `encoding`: whole with no `statement`, else its statements at or below that ID.
    Object {
        key: Vec<u8>,
        encoding: Vec<u8>,
        statement: Option<SystemId>,
    },
    /// In a statement's record, the properties up to and including this one.
    Property {
        predicate: SystemId,
        value: EncodedTerm,
    },
}
