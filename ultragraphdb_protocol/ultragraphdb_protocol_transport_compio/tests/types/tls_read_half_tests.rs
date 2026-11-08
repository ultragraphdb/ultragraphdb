/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A split TLS stream's receiving half reads the peer's plaintext, as the whole stream does.

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_transport_tls::TestPki;

use crate::utils::bytes::{receive, send};
use crate::utils::probe::split_pair;
use crate::utils::ready::ready;

#[test]
fn test_the_read_half_reads_the_peers_plaintext() -> TestResult {
    let pki = TestPki::new()?;
    let (mut server, mut read, _write, _probed) = split_pair(&pki)?;
    ready(send(&mut server, b"pong"))?;
    assert_eq!(ready(receive(&mut read, 4))?, b"pong");
    Ok(())
}
