#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# Writes the seeds of the fuzz targets `command_decode` and `snapshot_decode` into fuzz_seeds/<target>/, encoded from
# the formats' text in MetadataCommand::encode's and MetadataSnapshot::encode's documentation alone, not by the crate:
# the suite's commands of every kind and its empty and small snapshots, whose names say what they hold, and malformed
# ones, named `malformed_*`. The unit tests decode every seed, a valid one re-encoding to its bytes, and check that the
# valid seeds are the crate's encodings of the suite's values. Run with `python3 -I seeds.py` from this directory.

import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))


def text(value, length_format):
    data = value.encode("utf-8")
    return struct.pack(length_format, len(data)) + data


def verifier(salt, iterations, stored, server):
    return struct.pack(">B", len(salt)) + salt + struct.pack(">I", iterations) + stored + server


COMMANDS = {
    "offer_catalog_3_on_2": bytes([1]) + struct.pack(">QQI", 3, 2, 2) + b"v3",
    "record_default_score": bytes([2]) + struct.pack(">Q", 4) + text("score", ">H") + struct.pack(">I", 2) + b"\x09\x08",
    "lease_block_1001_2001": bytes([3]) + struct.pack(">HIQQ", 2, 5, 1001, 2001),
    "init_placement_3": bytes([4]) + struct.pack(">II", 3, 6),
    "change_placement_2_ranges": bytes([5]) + struct.pack(">I", 2) + struct.pack(">III", 0, 100, 1)
    + struct.pack(">III", 100, 1 << 24, 2),
    "set_leader_7_8": bytes([6]) + struct.pack(">II", 7, 8),
    "create_principal_ann": bytes([7]) + struct.pack(">Q", 9) + text("ann", ">B")
    + verifier(b"\x01\x02\x03", 4096, bytes([10] * 32), bytes([11] * 32)) + b"\x01",
    "remove_principal_bob": bytes([8]) + text("bob", ">B") + struct.pack(">I", 1) + (16).to_bytes(16, "big"),
    "register_session_12": bytes([9]) + (12).to_bytes(16, "big") + struct.pack(">QI", 13, 2)
    + (14).to_bytes(16, "big") + (15).to_bytes(16, "big"),
    "learn_close_39": bytes([10]) + struct.pack(">QI", 39, 1) + struct.pack(">QQ", 3, 3) + struct.pack(">I", 1)
    + (14).to_bytes(16, "big") + struct.pack(">Q", 2),
    "report_untrimmed_2_at_30": bytes([11]) + struct.pack(">IQ", 2, 30),
}
COMMANDS["chunk_at_16"] = bytes([12]) + struct.pack(">QI", 16, 3) + b"\x05\x06\x07"
COMMANDS["commit_of_19_bytes"] = bytes([13]) + struct.pack(">QQ", 19, 0x0102030405060708)
COMMANDS["malformed_kind_14"] = bytes([14])
COMMANDS["malformed_partition_0"] = bytes([6]) + struct.pack(">II", 0, 1)
COMMANDS["malformed_trailing_byte"] = COMMANDS["set_leader_7_8"] + b"\x00"


def snapshot_bytes(position, learned, catalogs, high_water):
    out = bytes([1]) + struct.pack(">QQI", position, learned, len(catalogs))
    for tenant, current, released, versions in catalogs:
        out += struct.pack(">QQQI", tenant, current, released, len(versions))
        for version, effective, data in versions:
            out += struct.pack(">Q", version)
            out += b"\x00" if effective is None else b"\x01" + struct.pack(">Q", effective)
            out += struct.pack(">I", len(data)) + data
    out += struct.pack(">I", 0)
    out += struct.pack(">I", len(high_water))
    for prefix, sequence in high_water:
        out += struct.pack(">HQ", prefix, sequence)
    out += b"\x00" + struct.pack(">I", 0) + struct.pack(">QI", 1, 0) + struct.pack(">II", 0, 0)
    # The last placement version's number and no placement version pending, as no map is held.
    out += struct.pack(">QI", 0, 0)
    return out


SMALL = snapshot_bytes(3, 5, [(2, 1, 0, [(1, 6, b"ab")])], [(1, 11)])
SNAPSHOTS = {
    "empty": snapshot_bytes(0, 0, [], []),
    "tenant_2_version_1_at_6": SMALL,
    "malformed_format_2": bytes([2]) + SMALL[1:],
    "malformed_truncated": SMALL[:-1],
}

for target, seeds in (("command_decode", COMMANDS), ("snapshot_decode", SNAPSHOTS)):
    out = os.path.join(HERE, "fuzz_seeds", target)
    os.makedirs(out, exist_ok=True)
    for name, data in seeds.items():
        with open(os.path.join(out, name), "wb") as seed:
            seed.write(data)
