/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_tool_toml::{TomlError, TomlFeature};

#[test]
fn test_is_a_std_error_without_a_source() {
    let error: Box<dyn Error> = Box::new(TomlError::unsupported(TomlFeature::Floats, 7));
    assert!(error.source().is_none());
    assert_eq!(
        error.to_string(),
        "line 7: floats are outside the TOML subset settings files use"
    );
}
