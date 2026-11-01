/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use tokio::task::LocalSet;
use ultragraphdb_client_tokio::{Anonymous, Client, ClientErrorEnum};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};

use crate::utils::serve::{SimConnect, config, serve_sim, server};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}

/// A commit left in doubt is resolved through the shell: the first attempt applied and its reply lost with the
/// connection, the second answered busy, so two attempts leave request 1 in doubt; the resolution answers it from the
/// server's record, committed once.
#[test]
fn test_a_commit_in_doubt_is_resolved_from_the_servers_record() {
    let rt = runtime();
    let scripted = server();
    scripted.borrow_mut().drop_after_next_commit();
    scripted.borrow_mut().answer_busy_retries(1);
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    LocalSet::new().block_on(&rt, async {
        tokio::task::spawn_local(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config().with_commit_attempts(2), Anonymous, 16);
        let session = client.register().await.unwrap();
        let tx = client.begin(0, false).await.unwrap();
        assert_eq!(
            client.commit(tx, session).await.unwrap_err().0,
            ClientErrorEnum::RetriesExhausted {
                session: session.number(),
                request: 1,
                attempts: 2
            }
        );
        let tx = client.begin(0, false).await.unwrap();
        assert_eq!(
            client.commit(tx, session).await.unwrap_err().0,
            ClientErrorEnum::InDoubt {
                session: session.number(),
                request: 1
            }
        );
        let committed = client.resolve(session).await.expect("resolved");
        assert_eq!(committed.request(), 1);
        assert_eq!(scripted.borrow().applied(), 1, "applied once");
    });
}
