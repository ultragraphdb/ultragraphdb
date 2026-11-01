/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use ultragraphdb_client_compio::{Anonymous, Client};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};

use crate::utils::serve::{SimConnect, config, serve_sim, server};

#[test]
fn test_close_gives_the_connection_up() {
    let rt = compio::runtime::Runtime::new().expect("a runtime");
    let scripted = server();
    let connector = SimConnect::new(SimNetwork::new(3, NetworkFaults::none()));
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_sim(Rc::clone(&scripted), accepted));
        let mut client = Client::new(connector, config(), Anonymous, 8);
        client.connect().await.unwrap();
        assert!(client.core().is_ready());
        client.close().await;
        assert!(!client.core().is_ready());
        assert!(!client.core().is_connected());
    });
}
