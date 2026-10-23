/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The parts of Dgraph's schema/schema.go besides the state: the initial schema and types, the checks of updates
//! against them, and the entries that store a schema and a type, as worker/mutation.go's `updateSchema` and
//! `updateType` write them.

pub(crate) mod entries;
pub(crate) mod initial;
pub(crate) mod pre_defined;
