/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;

/// One row of the read kernel ([`crate::QuadStore::scan`]): a list's key, without its version, and the list read at
/// the scan's version, each object with the statement IDs it is held under.
#[derive(Debug, Clone, PartialEq)]
pub struct KernelRow {
    key: Vec<u8>,
    objects: Vec<(EncodedTerm, Vec<SystemId>)>,
}

impl KernelRow {
    /// The row of the list `key` holding `objects`.
    #[must_use]
    pub fn new(key: Vec<u8>, objects: Vec<(EncodedTerm, Vec<SystemId>)>) -> Self {
        Self { key, objects }
    }

    /// A page's sink gathering what it is offered into rows appended to `out`, every object taken: how a read wanting whole
    /// rows reads pages.
    pub fn gather(
        out: &mut Vec<KernelRow>,
    ) -> impl FnMut(&[u8], &EncodedTerm, &[SystemId]) -> ControlFlow<()> + '_ {
        let first = out.len();
        move |key, object, statements| {
            let object = (object.clone(), statements.to_vec());
            let last = out.get_mut(first..).and_then(<[Self]>::last_mut);
            match last.filter(|row| row.key == key) {
                Some(row) => row.objects.push(object),
                None => out.push(Self::new(key.to_vec(), vec![object])),
            }
            ControlFlow::Continue(())
        }
    }

    /// The list's key, without its version.
    #[must_use]
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// The list's objects at the scan's version.
    #[must_use]
    pub fn objects(&self) -> &[(EncodedTerm, Vec<SystemId>)] {
        &self.objects
    }
}
