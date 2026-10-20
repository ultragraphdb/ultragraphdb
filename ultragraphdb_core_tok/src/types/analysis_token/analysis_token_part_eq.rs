/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::analysis_token::AnalysisToken;

impl PartialEq for AnalysisToken {
    fn eq(&self, other: &Self) -> bool {
        self.term == other.term
            && self.start == other.start
            && self.end == other.end
            && self.typ == other.typ
    }
}

impl Eq for AnalysisToken {}
