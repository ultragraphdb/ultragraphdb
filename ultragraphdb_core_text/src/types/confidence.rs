/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, language/language.go (type Confidence). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The level of certainty of an inferred subtag, ordered from none to exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Confidence {
    /// Full confidence that there was no match.
    No,
    /// The most likely value picked out of a set of alternatives.
    Low,
    /// The value is generally assumed to be the correct match.
    High,
    /// An exact match or explicitly specified value.
    Exact,
}
