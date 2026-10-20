/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

#![allow(clippy::all)]

//! Dgraph's internal protobuf messages and services (`protos/pb.proto`), with the Badger messages they use.
//!
//! The modules are named after the proto packages, `pb` and `badgerpb4`, which is what `rules_rust_prost` emits
//! under Bazel, so both builds expose the same paths: `ultragraphdb_proto_pb::pb::Posting`.

pub mod pb {
    tonic::include_proto!("pb");
}

pub mod badgerpb4 {
    tonic::include_proto!("badgerpb4");
}
