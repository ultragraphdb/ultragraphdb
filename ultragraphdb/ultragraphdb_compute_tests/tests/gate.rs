/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A gate in front of the system group, on the scenario's runtime: it accepts compute's connections at once and carries the
//! `i`th of them, counted from 0, to the group only after `i` steps, so compute's cores' links to the group open spread over
//! seconds, as on a loaded host they may.

use std::cell::Cell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::spawn;
use ultragraphdb_tool_bdd::TestResult;

/// How many bytes each side is read in at a time.
const CHUNK: usize = 65_536;

/// Listens at `address` and carries each connection to `target`, the `i`th after `i` times `step`: the connections it took.
/// It runs as long as the runtime it was started on.
pub async fn gate(address: &str, target: SocketAddr, step: Duration) -> TestResult<Rc<Cell<u32>>> {
    let listener = TcpListener::bind(address.trim_start_matches("tcp://")).await?;
    let taken: Rc<Cell<u32>> = Rc::default();
    let counted = Rc::clone(&taken);
    spawn(async move {
        // Until the runtime ends or an accept fails (Ultra rule 2).
        while let Ok((from, _)) = listener.accept().await {
            let at = counted.get();
            counted.set(at.saturating_add(1));
            spawn(async move {
                compio::time::sleep(step.saturating_mul(at)).await;
                // A group that cannot be reached closes compute's side with the gate's.
                drop(splice(from, target).await);
            })
            .detach();
        }
    })
    .detach();
    Ok(taken)
}

/// Carries `from` to a connection to `target`, a task each way.
async fn splice(from: TcpStream, target: SocketAddr) -> std::io::Result<()> {
    let to = TcpStream::connect(target).await?;
    let (mut up_from, mut down_to) = from.into_split();
    let (mut down_from, mut up_to) = to.into_split();
    spawn(async move {
        // Until either side ends (Ultra rule 2).
        loop {
            let BufResult(read, bytes) = up_from.read(Vec::with_capacity(CHUNK)).await;
            if !matches!(read, Ok(n) if n > 0) || up_to.write_all(bytes).await.0.is_err() {
                break;
            }
        }
        drop(up_to.shutdown().await);
    })
    .detach();
    spawn(async move {
        // Until either side ends (Ultra rule 2).
        loop {
            let BufResult(read, bytes) = down_from.read(Vec::with_capacity(CHUNK)).await;
            if !matches!(read, Ok(n) if n > 0) || down_to.write_all(bytes).await.0.is_err() {
                break;
            }
        }
        drop(down_to.shutdown().await);
    })
    .detach();
    Ok(())
}
