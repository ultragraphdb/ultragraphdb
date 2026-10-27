/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_protocol::ProtocolError;

#[test]
fn test_is_a_std_error_without_a_source() {
    let error: Box<dyn Error> = Box::new(ProtocolError::UnknownCommand(18));
    assert!(error.source().is_none());
    assert_eq!(error.to_string(), "unknown command 18");
}
