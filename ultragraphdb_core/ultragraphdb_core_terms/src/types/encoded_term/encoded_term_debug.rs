/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::encoded_term::EncodedTerm;

impl fmt::Debug for EncodedTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex: String = self.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        write!(f, "EncodedTerm({hex})")
    }
}
