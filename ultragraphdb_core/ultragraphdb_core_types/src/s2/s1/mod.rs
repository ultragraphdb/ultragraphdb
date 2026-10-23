/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Package s1 of golang/geo: types and functions for working with geometry in S¹ (circular
//! geometry).

mod angle;
mod chordangle;
mod interval;

pub use angle::Angle;
pub use chordangle::ChordAngle;
pub use interval::S1Interval;
