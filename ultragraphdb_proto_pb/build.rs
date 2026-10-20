/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // pb.proto imports dgo's api.proto, which ultragraphdb_proto_dgraph already generates; the extern path
    // points the generated code at that crate, as rules_rust_prost does for a proto_library dependency.
    // Zero's service has an RPC named Connect, whose client method collides with tonic's transport constructor
    // `connect`; without the transport constructors, clients are built from a Channel with `new`.
    tonic_prost_build::configure()
        .build_transport(false)
        .extern_path(".api", "::ultragraphdb_proto_dgraph::api")
        .compile_protos(
            &["proto/pb.proto", "proto/badgerpb4.proto"],
            &["proto", "../ultragraphdb_proto_dgraph/proto"],
        )?;
    Ok(())
}
