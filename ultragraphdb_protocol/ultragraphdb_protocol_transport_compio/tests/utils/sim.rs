/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A simulated connection's two ends as compio streams.

use ultragraphdb_protocol_transport::SimNetwork;
use ultragraphdb_protocol_transport_compio::CompioSimStream;

/// A connection of `network`: the client's end and the server's, as compio streams.
pub fn connect(network: &SimNetwork) -> (CompioSimStream, CompioSimStream) {
    let (client, server) = network.connect();
    (CompioSimStream::new(client), CompioSimStream::new(server))
}
