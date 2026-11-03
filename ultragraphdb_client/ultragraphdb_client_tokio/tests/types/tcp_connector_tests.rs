/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::SocketAddr;

use ultragraphdb_client_tokio::{Connect, TcpConnector};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

/// `connector`'s connection to a loopback listener of its own: whether its socket has `TCP_NODELAY` set.
fn nodelay_of(connector: TcpConnector) -> TestResult<bool> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("a runtime")?;
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address: SocketAddr = listener.local_addr()?;
        let mut connector = TcpConnector::new(address).with_tcp_nodelay(connector.tcp_nodelay());
        assert_eq!(connector.address(), address);
        let stream = connector.connect().await?;
        assert_eq!(stream.peer_addr()?, address);
        Ok(stream.nodelay()?)
    })
}

/// `tcp_nodelay` is on by default, disabling Nagle's algorithm; set false, the socket keeps Nagle's algorithm on.
#[test]
fn test_tcp_nodelay_is_on_by_default_and_can_be_turned_off() -> TestResult {
    let any: SocketAddr = "127.0.0.1:9".parse()?;
    let default = TcpConnector::new(any);
    assert!(default.tcp_nodelay());
    assert!(nodelay_of(default)?, "TCP_NODELAY set by default");
    let off = TcpConnector::new(any).with_tcp_nodelay(false);
    assert!(!off.tcp_nodelay());
    assert!(!nodelay_of(off)?, "TCP_NODELAY left clear");
    assert!(
        TcpConnector::new(any)
            .with_tcp_nodelay(false)
            .with_tcp_nodelay(true)
            .tcp_nodelay()
    );
    Ok(())
}
