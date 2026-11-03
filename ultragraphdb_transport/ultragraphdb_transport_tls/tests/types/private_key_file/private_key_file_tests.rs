/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_tls::PrivateKeyFile;

#[test]
fn test_a_private_key_file_keeps_its_path() {
    let file = PrivateKeyFile::new(String::from("/etc/ultragraph/tls/key.pem"));
    assert_eq!(file.path(), "/etc/ultragraph/tls/key.pem");
    assert_eq!(file.clone(), file);
    assert_ne!(file, PrivateKeyFile::new("/other.pem"));
}
