/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A frontend played by the test: what compute asks of it, the placement map and where each partition listens.

use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::{Answer, Session};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BucketRange, Command, ErrorCode, ErrorReply, Members, Placement, PreferredLeader,
};

use super::played_partition::frame_of;

/// One connection at the played frontend.
#[derive(Debug)]
pub struct FrontendConnection {
    /// The cluster.
    pub cluster: u64,
    /// The partitions, from 1.
    pub partitions: u32,
    /// Where each partition listens.
    pub listed: Vec<(u32, String)>,
}

/// The even placement over `partitions`, as a real frontend's first start forms it.
fn placement(partitions: u32) -> Result<Placement, ErrorReply> {
    let map = PlacementMap::even(partitions, NodeId::new(1))
        .map_err(|error| ErrorReply::new(ErrorCode::Unavailable, error.to_string()))?;
    let mut reply = Placement::reply(0, 0, 1, 0);
    for (range, partition) in map.ranges_at(Epoch::ZERO) {
        reply.push_range(BucketRange::new(
            range.start(),
            range.end(),
            partition.get(),
        ));
    }
    for partition in 1..=partitions {
        reply.push_leader(PreferredLeader::new(partition, 1));
    }
    Ok(reply)
}

impl Session for FrontendConnection {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let view = match frame.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let command = view.command();
        let built = if command == Command::Members {
            let mut members = Members::new();
            for (partition, address) in &self.listed {
                members.push_storage(*partition, address.clone());
            }
            frame_of(self.cluster, &members, stream, true)
        } else if command == Command::Placement {
            placement(self.partitions)
                .and_then(|reply| frame_of(self.cluster, &reply, stream, true))
        } else {
            Err(ErrorReply::new(
                ErrorCode::Unavailable,
                format!("command {} is not played", command.number()),
            ))
        };
        match built {
            Ok(reply) => Answer::Reply(reply),
            Err(refusal) => Answer::Refuse(refusal),
        }
    }
}
