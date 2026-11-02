/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_transport_tls::TlsError;

#[test]
fn test_is_a_std_error_without_a_source() {
    assert!(TlsError::trust_store_empty("/p".into()).source().is_none());
}
