#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# Writes the seeds of the fuzz target `placement_decode` into fuzz_seeds/placement_decode/, encoded from the format's
# text in PlacementMap::encode's documentation alone, not by the crate: valid maps, whose names say what they hold,
# and malformed ones, named `malformed_*`. The unit tests decode every seed: a valid one re-encodes to its bytes, a
# malformed one is refused. Run with `python3 -I seeds.py` from this directory.

import os
import struct

END = 1 << 24
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fuzz_seeds", "placement_decode")


def encode(layouts, leaders):
    """layouts: [(epoch, [(start, partition), ...])]; leaders: [(partition, node)] in partition order."""
    out = bytes([1]) + struct.pack(">H", len(layouts))
    for epoch, runs in layouts:
        out += struct.pack(">QI", epoch, len(runs))
        for start, partition in runs:
            out += struct.pack(">II", start, partition)
    out += struct.pack(">I", len(leaders))
    for partition, node in leaders:
        out += struct.pack(">II", partition, node)
    return out


def even(partitions):
    """Every run of 65,536 buckets split in order, the first partitions taking one bucket more."""
    base, larger = divmod(1 << 16, partitions)
    runs = []
    for run in range(256):
        start = run << 16
        for index in range(partitions):
            if not runs or runs[-1][1] != index + 1:
                runs.append((start, index + 1))
            start += base + (1 if index < larger else 0)
    return runs


SEEDS = {
    "even_1_node_7": encode([(0, even(1))], [(1, 7)]),
    "even_2_node_1": encode([(0, even(2))], [(1, 1), (2, 1)]),
    "even_3_node_9": encode([(0, even(3))], [(1, 9), (2, 9), (3, 9)]),
    "changed_at_epoch_5": encode([(0, even(1)), (5, [(0, 2), (100, 1)])], [(1, 7)]),
    "three_layouts_no_leader": encode(
        [(0, even(1)), (21, [(0, 3), (21888, 1)]), ((1 << 42) - 1, [(0, 4), (END - 1, 5)])], []
    ),
    "malformed_format_2": bytes([2]) + encode([(0, even(1))], [(1, 7)])[1:],
    "malformed_first_layout_from_1": encode([(1, even(1))], []),
    "malformed_neighbours_of_one_partition": encode([(0, [(0, 1), (100, 1)])], []),
    "malformed_trailing_byte": encode([(0, even(1))], [(1, 7)]) + b"\x00",
    "malformed_truncated": encode([(0, even(1))], [(1, 7)])[:20],
}

os.makedirs(OUT, exist_ok=True)
for name, data in SEEDS.items():
    with open(os.path.join(OUT, name), "wb") as seed:
        seed.write(data)
