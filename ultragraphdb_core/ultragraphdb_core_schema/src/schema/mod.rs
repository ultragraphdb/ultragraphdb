/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The storage-free parts of Dgraph's schema/schema.go besides the state: the initial schema and types, and the
//! checks of updates against them.

pub(crate) mod initial;
pub(crate) mod pre_defined;
