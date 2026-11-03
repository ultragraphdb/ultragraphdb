/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use ultragraphdb_client_tokio::TokioSimStream;
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

/// A write sends one message the peer reads; a shutdown is read as the end once what was sent before is read.
#[test]
fn test_a_write_reaches_the_peer_and_a_shutdown_ends_it() -> TestResult {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("a runtime")?;
    rt.block_on(async {
        let network = SimNetwork::new(1, NetworkFaults::none());
        let (client, server) = network.connect();
        let mut client = TokioSimStream::new(client);
        let mut server = TokioSimStream::new(server);
        assert_eq!(client.write(b"frame").await?, 5);
        client.flush().await?;
        client.shutdown().await?;
        network.run_until_idle();
        let mut read = Vec::new();
        let _read: usize = server.read_to_end(&mut read).await?;
        assert_eq!(read, b"frame");
        let _sim: &ultragraphdb_protocol_transport::SimStream = client.sim();
        Ok(())
    })
}
