/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_options::OptionsError;

#[test]
fn test_display_is_dgraphs_text() {
    assert_eq!(
        OptionsError::NotAllowed("option metric is not allowed!".into()).to_string(),
        "option metric is not allowed!"
    );
    assert_eq!(
        OptionsError::invalid("Can't create a vector index for l1").to_string(),
        "Can't create a vector index for l1"
    );
}
