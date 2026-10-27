/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a declared predicate's objects may be: IRIs and blank nodes, literals, or either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum DeclaredKind {
    Edge,
    Property,
    #[default]
    Either,
}
