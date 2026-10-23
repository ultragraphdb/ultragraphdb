/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_num::NumError;

use crate::errors::types_error::TypesError;

/// A number Go's `strconv` refused, with its text.
impl From<NumError> for TypesError {
    fn from(e: NumError) -> Self {
        TypesError::number(e.to_string())
    }
}
