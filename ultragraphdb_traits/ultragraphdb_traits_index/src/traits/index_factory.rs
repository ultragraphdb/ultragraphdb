/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/index.go (IndexFactory). Changed for ultragraphdb: translated to Rust;
 * Go's `error` as an associated type, and the name of an index as one, where Go names an index by a string.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::traits::float::Float;
use crate::traits::vector_index::VectorIndex;
use crate::types::allowed_options::AllowedOptions;
use crate::types::options::Options;

/// Dgraph's `index.IndexFactory`: creates, finds and removes vector indexes by name; there is no update, only
/// remove and create.
pub trait IndexFactory<T: Float> {
    /// The indexes it creates.
    type Index: VectorIndex<T>;
    /// What names an index: where it keeps its lists.
    type Name: ?Sized;
    /// Why an operation failed.
    type Error;

    /// The factory's name, not an index's.
    fn name(&self) -> &str;

    /// The options in use, as text.
    fn get_options(&self, o: &Options) -> String;

    /// The options it takes and how to parse them.
    fn allowed_options(&self) -> AllowedOptions;

    /// A new index under `name`; an error when the name is taken.
    fn create(
        &self,
        name: &Self::Name,
        o: &Options,
        float_bits: usize,
    ) -> Result<Self::Index, Self::Error>;

    /// The index under `name`, `None` when there is none.
    fn find(&self, name: &Self::Name) -> Result<Option<Self::Index>, Self::Error>;

    /// Removes the index under `name`; nothing happens when there is none.
    fn remove(&self, name: &Self::Name) -> Result<(), Self::Error>;

    /// Creates the index under `name`, replacing any that exists.
    fn create_or_replace(
        &self,
        name: &Self::Name,
        o: &Options,
        float_bits: usize,
    ) -> Result<Self::Index, Self::Error>;
}
