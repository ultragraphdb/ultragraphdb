/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::panic::catch_unwind;

use compio::io::AsyncRead;
use ultragraphdb_protocol_bus::{NetworkFaults, SimNetwork};

use crate::utils::ready::ready;

#[test]
fn test_a_ready_future_gives_its_output_and_a_waiting_one_fails() {
    assert_eq!(ready(async { 7 }), 7);
    // A read with nothing in flight and its peer open never completes: the helper fails rather than hangs.
    let refused = catch_unwind(|| {
        let network = SimNetwork::new(1, NetworkFaults::none());
        let (_client, mut server) = network.connect();
        ready(server.read(Vec::with_capacity(8)))
    });
    assert!(refused.is_err());
}
