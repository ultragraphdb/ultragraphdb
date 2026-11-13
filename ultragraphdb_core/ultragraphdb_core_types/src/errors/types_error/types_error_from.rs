/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::{ParseFloatError, ParseIntError};
use std::str::ParseBoolError;

use ultragraphdb_core_geo::GeoError;

use crate::errors::types_error::TypesError;

/// A text that is no integer.
impl From<ParseIntError> for TypesError {
    fn from(e: ParseIntError) -> Self {
        TypesError::number(e.to_string())
    }
}

/// A text that is no float.
impl From<ParseFloatError> for TypesError {
    fn from(e: ParseFloatError) -> Self {
        TypesError::number(e.to_string())
    }
}

/// A text that is no boolean.
impl From<ParseBoolError> for TypesError {
    fn from(e: ParseBoolError) -> Self {
        TypesError::number(e.to_string())
    }
}

/// A text or bytes that are no geometry.
impl From<GeoError> for TypesError {
    fn from(e: GeoError) -> Self {
        TypesError::geo(e.to_string())
    }
}
