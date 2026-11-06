/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use ultragraphdb_common_role::{RoleClient, RoleReader, RoleWriter};
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::FLAG_LAST;

use crate::types::link_event::LinkEvent;
use crate::types::role_link::RoleLink;

/// Dials, then runs the link; a dial that fails closes it, telling every stream.
pub(super) fn spawn_dialled(
    link: RoleLink,
    outgoing: UnboundedReceiver<Vec<u8>>,
    address: Address,
    name: String,
    password: String,
) {
    spawn(async move {
        match RoleClient::dial(&address, link.cluster, &name, &password).await {
            Ok(client) => run(link, outgoing, client).await,
            Err(error) => {
                tracing::debug!(address = %address, %error, "a role connection could not be opened");
                closed(&link);
            }
        }
    })
    .detach();
}

/// Runs the link over a connection already dialled.
pub(super) fn spawn_over(link: RoleLink, outgoing: UnboundedReceiver<Vec<u8>>, client: RoleClient) {
    spawn(run(link, outgoing, client)).detach();
}

/// Writes on one task and reads on another, until either finds the connection closed.
async fn run(link: RoleLink, outgoing: UnboundedReceiver<Vec<u8>>, client: RoleClient) {
    let (reader, writer, welcome) = client.split();
    link.max_frame.set(welcome.max_frame());
    spawn(write(link.clone(), outgoing, writer)).detach();
    read(link, reader).await;
}

async fn write(link: RoleLink, mut outgoing: UnboundedReceiver<Vec<u8>>, mut writer: RoleWriter) {
    while let Some(frame) = outgoing.next().await {
        if let Err(error) = writer.send(frame).await {
            tracing::debug!(%error, "a role connection's write failed");
            closed(&link);
            return;
        }
    }
    writer.shutdown().await;
}

async fn read(link: RoleLink, mut reader: RoleReader) {
    loop {
        let frame = match reader.next_frame().await {
            Ok(frame) => frame,
            Err(error) => {
                tracing::debug!(%error, "a role connection's read ended");
                closed(&link);
                return;
            }
        };
        if let Err(refused) = deliver(&link, &frame) {
            tracing::warn!(%refused, "a role connection sent what it may not, and closes");
            closed(&link);
            return;
        }
    }
}

/// Hands `frame` to the session whose stream it answers: a frame on a stream that has ended is late, and dropped; a stream not
/// yet opened answers nothing, which ends the connection (`PROTOCOL.md` §2.5, step 3).
fn deliver(link: &RoleLink, frame: &ReceivedFrame) -> Result<(), String> {
    let view = frame.view().map_err(|rule| rule.to_string())?;
    let stream = view.header().stream();
    let mut routes = link.routes.borrow_mut();
    if stream == 0 || stream >= routes.next {
        return Err(format!(
            "a frame on stream {stream}, which it holds no request on"
        ));
    }
    let last = view.header().flags() & FLAG_LAST != 0;
    let sink = if last {
        routes.open.remove(&stream)
    } else {
        routes.open.get(&stream).cloned()
    };
    drop(routes);
    if let Some(sink) = sink {
        sink.tell(LinkEvent::Frame(frame.bytes().to_vec()));
    }
    Ok(())
}

/// Marks the connection closed, and tells each stream on it.
fn closed(link: &RoleLink) {
    let mut routes = link.routes.borrow_mut();
    routes.closed = true;
    let sinks: Vec<_> = routes.open.drain().map(|(_, sink)| sink).collect();
    drop(routes);
    for sink in sinks {
        sink.tell(LinkEvent::Closed);
    }
}
