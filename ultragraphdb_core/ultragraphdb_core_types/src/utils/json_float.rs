/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The JSON form of a finite double: the shortest digits that read back to it, written out in full from 1e-6 to below
/// 1e21 and in exponent form (`1e-7`, `1e21`) outside, so a whole number takes no `.0`. `None` for NaN and the
/// infinities, which JSON has no form for.
pub(crate) fn json_float(f: f64) -> Option<String> {
    if !f.is_finite() {
        return None;
    }
    let magnitude = f.abs();
    Some(if magnitude == 0.0 || (1e-6..1e21).contains(&magnitude) {
        format!("{f}")
    } else {
        format!("{f:e}")
    })
}

/// How an error names a double JSON has no form for: `NaN`, `+Inf` or `-Inf`.
pub(crate) fn non_finite_name(f: f64) -> &'static str {
    if f.is_nan() {
        "NaN"
    } else if f.is_sign_positive() {
        "+Inf"
    } else {
        "-Inf"
    }
}
