#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

"""Dependency helpers shared by the workspace's BUILD files."""

# all_crate_deps maps a Cargo path dependency on a proto crate to that package's default target, which is the
# proto_library; the Rust crate generated from it is the package's *_bindings target.
_PROTO_BINDINGS = {
    "//ultragraphdb_proto/ultragraphdb_proto_dgraph": "//ultragraphdb_proto/ultragraphdb_proto_dgraph:ultragraphdb_proto_dgraph_bindings",
    "//ultragraphdb_proto/ultragraphdb_proto_pb": "//ultragraphdb_proto/ultragraphdb_proto_pb:ultragraphdb_proto_pb_bindings",
}

def with_proto_bindings(deps):
    """Replaces proto_library labels among `deps` with their generated Rust crates."""
    return [_PROTO_BINDINGS.get(dep, dep) for dep in deps]
