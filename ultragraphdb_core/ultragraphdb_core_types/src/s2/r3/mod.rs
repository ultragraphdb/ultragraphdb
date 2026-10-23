/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Package r3 of golang/geo: types and functions for working with geometry in ℝ³.

mod exact_float;
mod precisevector;
mod vector;

pub(crate) use precisevector::PreciseVector;
pub(crate) use vector::Axis;
pub use vector::R3Vector;
