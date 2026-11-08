/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::{Future, poll_fn};
use std::pin::pin;
use std::rc::Rc;
use std::task::Poll;

use ultragraphdb_client_compio::{Anonymous, Client};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};

use crate::utils::serve::{SimConnect, config, serve_sim, server};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

/// A Begin whose future is dropped once its request is out, as a caller's own timeout drops it, is let go when its
/// answer arrives: the core holds no more requests than before, the transaction is aborted on the server, and the
/// Registers after it are answered each with a session of its own.
#[test]
fn test_a_dropped_request_is_let_go_and_a_later_one_answered() -> TestResult {
    let rt = compio::runtime::Runtime::new().context("a runtime")?;
    let scripted = server();
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        client.connect().await?;
        let held = client.core().pending_requests();
        {
            let mut begin = pin!(client.begin(0, false));
            let polled = poll_fn(|cx| Poll::Ready(begin.as_mut().poll(cx))).await;
            assert!(polled.is_pending(), "the Begin waits for its answer");
        }
        assert_eq!(
            client.core().pending_requests(),
            held + 1,
            "its answer is still to come"
        );
        let session = client.register().await?;
        assert_eq!(client.core().next_request(session), Some(1));
        assert_eq!(client.core().pending_requests(), held);
        // The Abort went out before this Register, so the server answers it once the transaction is gone.
        let again = client.register().await?;
        assert_ne!(again.number(), session.number());
        assert_eq!(
            scripted.borrow().open_transactions(),
            0,
            "the dropped Begin's transaction is aborted"
        );
        assert_eq!(client.core().pending_requests(), held);
        Ok::<(), TestError>(())
    })?;
    Ok(())
}
