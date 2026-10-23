/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ports of the Go `math` functions golang/geo calls. Go implements these in pure Go on amd64
//! and arm64, so the platform libm that Rust's `f64` methods use may differ in the last bit.
//! Each sub-module is a translation of the Go file of the same name.

mod asin;
mod atan;
mod atan2;
mod bits;
mod dim;
mod fmod;
mod frexp;
mod ldexp;
mod logb;
mod nextafter;
mod remainder;
mod sin;
mod trig_reduce;

pub(crate) use asin::asin;
pub(crate) use atan2::atan2;
pub(crate) use dim::{max, min};
pub(crate) use frexp::frexp;
pub(crate) use ldexp::ldexp;
pub(crate) use logb::ilogb;
pub(crate) use nextafter::nextafter;
pub(crate) use remainder::remainder;
pub(crate) use sin::{cos, sin};

/// Go's `math.NaN()`, the quiet NaN with payload 1.
pub(crate) const NAN: f64 = f64::from_bits(0x7FF8000000000001);
