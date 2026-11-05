/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::posting_error::PostingError;
use crate::types::list::List;

/// Where the read kernel's one walk ([`crate::walk_lists`]) gets what the store's keys alone do not say: the keys a source
/// adds to theirs, and each key's list. A plain read's source adds no key and reads the store. A transaction's adds its
/// written keys and answers with its own writes on top, so a transaction's scan and a plain one cannot disagree about a
/// key's row.
pub trait ListSource {
    /// The keys the source adds in the range from `start`, inclusive, to `end`, exclusive, or to the last key, ascending.
    fn added_keys<'a>(
        &'a self,
        start: &'a [u8],
        end: Option<&'a [u8]>,
    ) -> impl Iterator<Item = &'a [u8]>;

    /// What `read` makes of the list of `key` as this source sees it at `version`, changing nothing the source holds.
    ///
    /// # Errors
    ///
    /// The store's error; corruption of the list.
    fn with_list<R>(
        &self,
        key: &[u8],
        version: u64,
        read: impl FnOnce(&List) -> R,
    ) -> Result<R, PostingError>;
}
