#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# Writes the seeds of the referrer_set_decode fuzz target from the format's text, independent of the Rust encoder:
# the byte 1, the member count as a u32 big-endian, then each member, sorted by group, as its group u32, its epoch u64 and
# its generation u64, all big-endian. A seed named malformed_* is one the decoder refuses. Run from the crate's directory:
#
#     python3 -I testdata/referrer_set_seeds.py

import pathlib
import struct


def member(group, epoch, generation):
    return struct.pack(">IQQ", group, epoch, generation)


def value(members, version=1, count=None):
    return bytes([version]) + struct.pack(">I", len(members) if count is None else count) + b"".join(members)


LAST_GROUP = (1 << 18) - 1
LAST_EPOCH = (1 << 42) - 1

SEEDS = {
    "empty": value([]),
    "one_member": value([member(7, 12, 1)]),
    "two_members": value([member(7, 12, 3), member(9, 14, 1)]),
    "the_bounds": value([member(0, 0, 1), member(LAST_GROUP, LAST_EPOCH, 2**64 - 1)]),
    "malformed_empty": b"",
    "malformed_short_header": bytes([1, 0, 0]),
    "malformed_version": value([member(7, 12, 1)], version=2),
    "malformed_count_above": value([member(7, 12, 1)], count=2),
    "malformed_count_below": value([member(7, 12, 1), member(9, 12, 1)], count=1),
    "malformed_count_huge": value([], count=2**32 - 1),
    "malformed_cut_member": value([member(7, 12, 1)])[:-1],
    "malformed_unsorted": value([member(9, 12, 1), member(7, 12, 1)]),
    "malformed_repeated": value([member(7, 12, 1), member(7, 13, 2)]),
    "malformed_group": value([member(LAST_GROUP + 1, 12, 1)]),
    "malformed_epoch": value([member(7, LAST_EPOCH + 1, 1)]),
    "malformed_generation": value([member(7, 12, 0)]),
}

directory = pathlib.Path(__file__).parent / "fuzz_seeds" / "referrer_set_decode"
directory.mkdir(parents=True, exist_ok=True)
for name, seed in SEEDS.items():
    (directory / name).write_bytes(seed)
