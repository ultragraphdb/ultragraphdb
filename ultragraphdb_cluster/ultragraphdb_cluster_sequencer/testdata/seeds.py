#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# Writes the seeds of the fuzz targets `record_decode` and `snapshot_decode` into fuzz_seeds/<target>/, encoded from the format's text in
# SequencerRecord::encode's documentation alone, not by the crate: valid records, whose names say what they hold, and
# malformed ones, named `malformed_*`. The unit tests decode every seed: a valid one re-encodes to its bytes, a
# malformed one is refused. Run with `python3 -I seeds.py` from this directory.

import os
import struct

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fuzz_seeds", "record_decode")
LAST_EPOCH = (1 << 42) - 1


def close(epoch, time_us, catalog, evictions, placement=0):
    out = bytes([1]) + struct.pack(">QQH", epoch, time_us, len(catalog))
    for namespace, version in catalog:
        out += struct.pack(">QQ", namespace, version)
    out += struct.pack(">H", len(evictions))
    for session in evictions:
        out += session.to_bytes(16, "big")
    return out + struct.pack(">Q", placement)


def transaction(epoch, reply_epoch, partitions, body):
    out = bytes([2]) + struct.pack(">QQH", epoch, reply_epoch, len(partitions))
    for partition in partitions:
        out += struct.pack(">I", partition)
    return out + struct.pack(">I", len(body)) + body


SEEDS = {
    "close_1_at_1000_us": close(1, 1000, [(3, 7)], [0x0102030405060708090A0B0C0D0E0F10]),
    "close_empty": close(1, 0, [], []),
    "close_announcing_placement_2": close(9, 5000, [], [], 2),
    "close_last_epoch": close(LAST_EPOCH, (1 << 64) - 1, [(1, 2), (1, 3)], [0, (1 << 128) - 1]),
    "transaction_13_reply_12": transaction(13, 12, [3, 1], b"\xaa\xbb"),
    "transaction_empty_body": transaction(6, 6, [1], b""),
    "malformed_kind_3": bytes([3]) + close(1, 0, [], [])[1:],
    "malformed_epoch_past_42_bits": close(1 << 42, 0, [], []),
    "malformed_partition_0": transaction(2, 1, [0], b""),
    "malformed_trailing_byte": close(1, 0, [], []) + b"\x00",
    "malformed_truncated_body": transaction(13, 12, [1], b"\xaa\xbb")[:-1],
}

os.makedirs(OUT, exist_ok=True)
for name, data in SEEDS.items():
    with open(os.path.join(OUT, name), "wb") as seed:
        seed.write(data)

# The seeds of the fuzz target `snapshot_decode`, from SequencerSnapshot::encode's documentation: the empty snapshot and
# the suite's sample (the closes of 3 and 4 and one transaction at position 5, the snapshot at 6), each record framed
# by its length, and a malformed one.
SNAPSHOT_OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fuzz_seeds", "snapshot_decode")


def snapshot(position, closes, transactions):
    out = bytes([1]) + struct.pack(">QI", position, len(closes))
    for record in closes:
        out += struct.pack(">I", len(record)) + record
    out += struct.pack(">I", len(transactions))
    for at, record in transactions:
        out += struct.pack(">QI", at, len(record)) + record
    return out


SAMPLE = snapshot(6, [close(3, 100, [(1, 2)], []), close(4, 200, [], [7], 1)], [(5, transaction(5, 4, [2], b"\xab"))])
SNAPSHOT_SEEDS = {
    "empty": snapshot(0, [], []),
    "closes_3_4_transaction_at_5": SAMPLE,
    "malformed_trailing_byte": SAMPLE + b"\x00",
}

os.makedirs(SNAPSHOT_OUT, exist_ok=True)
for name, data in SNAPSHOT_SEEDS.items():
    with open(os.path.join(SNAPSHOT_OUT, name), "wb") as seed:
        seed.write(data)
