/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Literal;

/// A node's property in the property-graph view: one value when the catalog declares its predicate single-valued, a
/// list of every value otherwise, ordered by their N-Triples text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PgValue {
    Scalar(Literal),
    List(Vec<Literal>),
}
