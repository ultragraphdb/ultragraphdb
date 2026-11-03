/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transport_tls::CertificateChainFile;

#[test]
fn test_a_certificate_chain_file_keeps_its_path() {
    let file = CertificateChainFile::new("/etc/ultragraph/tls/chain.pem");
    assert_eq!(file.path(), "/etc/ultragraph/tls/chain.pem");
    assert_eq!(file.clone(), file);
    assert_ne!(file, CertificateChainFile::new("/other.pem"));
}
