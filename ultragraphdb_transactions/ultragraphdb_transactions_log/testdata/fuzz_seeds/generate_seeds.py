#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The seeds of the write-ahead log's fuzz targets, laid out byte by byte from PROTOCOL.md's tables (the frame header,
# the Round command header of section 12.1 with its derivation version at offset 36, the chunked body) and the
# superblock's table in src/types/superblock/mod.rs, independently of the Rust code. Checksums with Python's xxhash
# 3.6.0 (libxxhash 0.8.2). The segments are the suite's records: the rounds of tests/utils/helpers.rs (body byte i of
# round p is p * 31 + i * 7, first version 2p, one transaction, derivation p % 7 + 1) of the scan tests' shapes.
#
#   python3 -I generate_seeds.py <testdata/fuzz_seeds directory>
#
# writes scan/<name>.segment and superblock/<name>.<copy>.

import sys
from pathlib import Path

import xxhash

CLUSTER = 0x0123_4567_89AB_CDEF
BLOCK = 4096


def le(value, width):
    return value.to_bytes(width, "little")


def body(position, length):
    return bytes((position * 31 + i * 7) % 256 for i in range(length))


def round_frame(position, parent, incarnation, length):
    """Round `position` of `incarnation` naming `parent`, its body `length` bytes of the suite's pattern, padded to
    whole blocks; with its checksum."""
    data = body(position, length)
    chunks = [data[:3840]] + [data[i:i + 4096] for i in range(3840, len(data), 4096)]
    leaves = [xxhash.xxh3_64_intdigest(c, seed=i) for i, c in enumerate(chunks)]
    if len(chunks) == 1:
        chunked, checksum_body = data, leaves[0]
    else:
        table = b"".join(le(leaf, 8) for leaf in leaves)
        chunked, checksum_body = data + table, xxhash.xxh3_64_intdigest(table)
    size = 256 + len(chunked)
    command_header = (le(position, 8) + le(parent, 8) + le(incarnation, 8) + le(2 * position, 8) + le(1, 4)
                      + le(position % 7 + 1, 4) + bytes(88))
    assert len(command_header) == 128
    # checksum_body, sequence (the position), cluster, size, reserved, release 0, protocol 1, command 17 (Round),
    # stream 0, reserved and flags 0, 62 reserved bytes, the command header.
    tail = (le(checksum_body, 8) + le(position, 16) + le(CLUSTER, 8) + le(size, 4) + le(0, 4) + le(0, 4) + le(1, 2)
            + le(17, 2) + le(0, 8) + bytes(2) + bytes(62) + command_header)
    assert len(tail) == 248
    checksum = xxhash.xxh3_64_intdigest(tail)
    frame = le(checksum, 8) + tail + chunked
    return frame + bytes(-len(frame) % BLOCK), checksum


def chain(first, last, parent, incarnation, lengths):
    """Rounds `first` to `last` chained from `parent`, round p's body `lengths(p)` bytes; their bytes and the last
    checksum."""
    out = b""
    for position in range(first, last + 1):
        frame, parent = round_frame(position, parent, incarnation, lengths(position))
        out += frame
    return out, parent


def segment(blocks, data):
    assert len(data) <= blocks * BLOCK
    return data + bytes(blocks * BLOCK - len(data))


def superblock(sequence, incarnation, checkpoint, checksum, first):
    fields = (b"UGWL" + le(3, 2) + le(0, 2) + le(sequence, 8) + le(CLUSTER, 8) + le(incarnation, 8)
              + le(checkpoint, 8) + le(checksum, 8) + le(first, 8))
    assert len(fields) == 56
    return fields + le(xxhash.xxh3_64_intdigest(fields), 8)


def main(out):
    scan = out / "scan"
    scan.mkdir(parents=True, exist_ok=True)
    three, _ = chain(1, 3, 0, 1, lambda p: 1000)
    (scan / "three_rounds.segment").write_bytes(segment(4, three))
    # One, two, one (a full block) and two (full) blocks: bodies of 1,000, 5,000, 3,840 and 7,920 bytes.
    sizes = {1: 1000, 2: 5000, 3: 3840, 4: 7920}
    mixed, _ = chain(1, 4, 0, 1, lambda p: sizes[p])
    (scan / "mixed_sizes.segment").write_bytes(segment(8, mixed))
    current, parent = chain(1, 2, 0, 1, lambda p: 1000)
    older, _ = chain(3, 4, parent, 2, lambda p: 1000)
    (scan / "older_incarnation.segment").write_bytes(segment(4, current + older))
    whole, _ = chain(1, 2, 0, 1, lambda p: 1000 if p == 1 else 7920)
    torn = whole[:2 * BLOCK]
    (scan / "torn_tail.segment").write_bytes(segment(4, torn))
    # The fuzz target reads a checkpoint from the first 8 bytes: its position the first byte modulo 8, 3, and its
    # checksum the 8 bytes; rounds 4 and 5 follow it.
    checkpoint = 0x1122_3344_5566_7703
    after, _ = chain(4, 5, checkpoint, 1, lambda p: 1000)
    (scan / "after_checkpoint.segment").write_bytes(le(checkpoint, 8) + segment(3, after))
    copies = out / "superblock"
    copies.mkdir(parents=True, exist_ok=True)
    (copies / "fresh.a").write_bytes(superblock(1, 1, 0, 0, 0))
    (copies / "checkpoint.b").write_bytes(superblock(2, 1, 8, 0x1122_3344_5566_7788, 2))
    top = (1 << 64) - 1
    (copies / "largest.a").write_bytes(superblock(top, top, top, top, top))
    for path in sorted(scan.iterdir()) + sorted(copies.iterdir()):
        if path.suffix != ".py":
            print(f"{path.name}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
