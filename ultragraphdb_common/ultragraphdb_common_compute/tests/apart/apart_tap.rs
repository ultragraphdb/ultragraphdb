/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A tap between compute and a played partition, on the test's runtime: every frame goes on as it came, and a Credit compute
//! never sent may follow a Scan to the partition, which the partition's connection then spends: the partition sends a stream
//! more frames than compute granted it (`PROTOCOL.md` §11.3, "A frame past its stream's credit is refused").

use std::cell::Cell;
use std::net::SocketAddr;
use std::rc::Rc;

use compio::BufResult;
use compio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::spawn;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{BodyBuilder, Command, Credit, HEADER_SIZE, Header, HeaderView};
use ultragraphdb_tool_bdd::TestResult;

use super::apart_cluster::CLUSTER;

/// How many bytes the partition's side is read in at a time.
const CHUNK: usize = 65_536;

/// A tap in front of one played partition, which compute dials in the partition's place.
#[derive(Debug)]
pub struct ApartTap {
    address: Address,
    /// The frames of the Credit that follows each Scan from here on; 0 for none.
    grants: Cell<u32>,
    connections: Cell<u32>,
}

impl ApartTap {
    /// A tap listening at a port of its own, carrying each connection to `target`.
    pub async fn start(target: &Address) -> TestResult<Rc<Self>> {
        let target: SocketAddr = target.to_string().trim_start_matches("tcp://").parse()?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = Address::parse(&format!("tcp://{}", listener.local_addr()?))?;
        let tap = Rc::new(Self {
            address,
            grants: Cell::new(0),
            connections: Cell::new(0),
        });
        let held = Rc::clone(&tap);
        spawn(async move {
            // Until the runtime ends or an accept fails (Ultra rule 2).
            while let Ok((compute, _)) = listener.accept().await {
                held.connections
                    .set(held.connections.get().saturating_add(1));
                // A partition that cannot be reached leaves compute's side to close with the tap's.
                drop(splice(compute, target, Rc::clone(&held)).await);
            }
        })
        .detach();
        Ok(tap)
    }

    /// Where compute dials it.
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Each Scan compute sends from here on is followed to the partition by a Credit of `frames` frames on its stream, which
    /// compute never sent; 0 for none.
    pub fn overgrant(&self, frames: u32) {
        self.grants.set(frames);
    }

    /// How many connections compute opened through it.
    pub fn connections(&self) -> u32 {
        self.connections.get()
    }
}

/// Carries `compute`'s connection to the partition at `target`, a task each way.
async fn splice(compute: TcpStream, target: SocketAddr, tap: Rc<ApartTap>) -> std::io::Result<()> {
    let partition = TcpStream::connect(target).await?;
    let (mut up_from, mut down_to) = compute.into_split();
    let (mut down_from, mut up_to) = partition.into_split();
    spawn(async move {
        // The frames the tap added: each frame of compute's after them takes its place one further on.
        let mut added = 0u128;
        // Until either side ends (Ultra rule 2).
        while let Some(mut frame) = whole_frame(&mut up_from).await {
            let Ok(header) = HeaderView::new(&frame) else {
                break;
            };
            let (sequence, stream, command) =
                (header.sequence(), header.stream(), header.command());
            let credit = (command == Command::Scan.number() && tap.grants.get() > 0)
                .then(|| {
                    Credit::new(tap.grants.get())
                        .frame(Header::new(CLUSTER, Command::Credit).with_stream(stream))
                        .ok()
                })
                .flatten();
            if Header::stamp_sequence(&mut frame, sequence + added).is_err()
                || up_to.write_all(frame).await.0.is_err()
            {
                break;
            }
            if let Some(mut credit) = credit {
                added += 1;
                if Header::stamp_sequence(&mut credit, sequence + added).is_err()
                    || up_to.write_all(credit).await.0.is_err()
                {
                    break;
                }
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

/// One frame from `from`, whole: `None` at the close.
async fn whole_frame(from: &mut impl AsyncRead) -> Option<Vec<u8>> {
    let BufResult(read, mut frame) = from.read_exact(Vec::with_capacity(HEADER_SIZE)).await;
    read.ok()?;
    let size = usize::try_from(HeaderView::new(&frame).ok()?.size()).ok()?;
    let rest = size.checked_sub(HEADER_SIZE)?;
    if rest > 0 {
        let BufResult(read, body) = from.read_exact(Vec::with_capacity(rest)).await;
        read.ok()?;
        frame.extend_from_slice(&body);
    }
    Some(frame)
}
