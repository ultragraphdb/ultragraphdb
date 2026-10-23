/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/options/option_parser.go (OptionParser). Changed for ultragraphdb: translated
 * to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_error_options::OptionsError;

use crate::types::option_value::OptionValue;

/// Dgraph's `OptionParser`: how the text of an option's value is read.
pub type OptionParser = fn(&str) -> Result<OptionValue, OptionsError>;
