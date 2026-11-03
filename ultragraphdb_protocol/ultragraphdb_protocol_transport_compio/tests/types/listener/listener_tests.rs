/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{Address, TransportErrorEnum};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::utils::socket::SocketDir;

#[test]
fn test_a_unix_path_already_bound_is_refused() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let dir = SocketDir::new()?;
        let address = dir.address("s.sock")?;
        let _first = Listener::bind(&address).await?;
        let Err(error) = Listener::bind(&address).await else {
            return Err(TestError::new("one socket path was bound twice"));
        };
        assert!(
            matches!(
                error.0,
                TransportErrorEnum::Io {
                    kind: std::io::ErrorKind::AddrInUse,
                    ..
                }
            ),
            "{error:?}"
        );
        Ok(())
    })
}

#[test]
fn test_a_tcp_listener_names_the_port_it_holds() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let bound = listener.local_address()?;
        let port = bound
            .host_port()
            .and_then(|host_port| host_port.strip_prefix("127.0.0.1:"))
            .ok_or_else(|| TestError::new(format!("{bound}: a loopback address")))?;
        assert_ne!(port, "0");
        // The address it names binds again once the listener is gone.
        drop(listener);
        assert_eq!(Listener::bind(&bound).await?.local_address()?, bound);
        Ok(())
    })
}

#[test]
fn test_an_address_that_cannot_be_bound_fails_as_io() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let address = Address::parse("unix:///nonexistent/ultragraphdb/role.sock")?;
        let Err(error) = Listener::bind(&address).await else {
            return Err(TestError::new(
                "a socket was bound in a directory that does not exist",
            ));
        };
        assert!(
            matches!(
                error.0,
                TransportErrorEnum::Io {
                    kind: std::io::ErrorKind::NotFound,
                    ..
                }
            ),
            "{error:?}"
        );
        Ok(())
    })
}
