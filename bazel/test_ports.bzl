#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

"""The port base of every test target whose processes listen on the loopback.

Each such target listens in a block of `PORT_BLOCK` ports of its own, from the base `port_env` sets in its `env` as
`ULTRAGRAPHDB_TEST_PORT_BASE`, so the targets run side by side and every port stays fixed and known within its target.
`ultragraphdb_tool_bdd`'s `port` reads the base and adds the offset a test declares. Every block lies below 32768, where
Linux's ephemeral range starts (macOS's starts at 49152), so no outgoing connection takes a port a block holds.
"""

# The first port of the first block.
FIRST_PORT = 12000

# How many ports a block holds, as `ultragraphdb_tool_bdd`'s `PORT_BLOCK` says.
PORT_BLOCK = 200

# The first port no block may reach: Linux's ephemeral range starts there.
PORT_LIMIT = 32768

# Each package's blocks: the index of its first and how many it holds. A package's targets take its blocks by the index each
# passes to `port_env`.
PACKAGE_BLOCKS = {
    "ultragraphdb/ultragraphdb_tests": (0, 46),
    "ultragraphdb/ultragraphdb_compute_tests": (46, 22),
    "ultragraphdb/ultragraphdb_storage_tests": (68, 20),
    "ultragraphdb/ultragraphdb_config_tests": (88, 1),
    "ultragraphdb/ultragraphdb_supervisor_tests": (89, 2),
}

def _check(blocks):
    """Fails the load when two packages share a block or a block reaches `PORT_LIMIT`."""
    taken = {}
    for package, (first, count) in blocks.items():
        if FIRST_PORT + (first + count) * PORT_BLOCK > PORT_LIMIT:
            fail("the blocks of {} reach past port {}".format(package, PORT_LIMIT))
        for block in range(first, first + count):
            if block in taken:
                fail("block {} is both {}'s and {}'s".format(block, taken[block], package))
            taken[block] = package

def port_env(index, env = {}):
    """The `env` of the calling package's target taking its block `index`: `env` with the block's base.

    Args:
        index: which of the package's blocks the target takes; no two of its targets pass the same.
        env: the target's other variables.

    Returns:
        `env` with `ULTRAGRAPHDB_TEST_PORT_BASE` set to the block's first port.
    """
    _check(PACKAGE_BLOCKS)
    package = native.package_name()
    if package not in PACKAGE_BLOCKS:
        fail("{} holds no port blocks: add it to PACKAGE_BLOCKS in bazel/test_ports.bzl".format(package))
    first, count = PACKAGE_BLOCKS[package]
    if index < 0 or index >= count:
        fail("{} holds {} port blocks, and block {} is not one: widen it in bazel/test_ports.bzl".format(package, count, index))
    return dict(env, ULTRAGRAPHDB_TEST_PORT_BASE = str(FIRST_PORT + (first + index) * PORT_BLOCK))
