#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The generator of id_lists.json and posting_lists.json: golden values for the packed ID list and the posting format,
# written from their specifications alone, independent of the Rust codec.
#
# The tests read both with ultragraphdb_tool_bdd::load_golden. Run, from a virtual environment holding python-xxhash
# (the Python binding of the xxHash C library, for the posting format's checksum):
#
#     python3 -m venv venv && venv/bin/pip install xxhash
#     venv/bin/python -I ultragraphdb_core/ultragraphdb_core_codec/testdata/golden/formats.py \
#         ultragraphdb_core/ultragraphdb_core_codec/testdata/golden
#
# The packed ID list (uid-lists, "ID lists pack into the repository's own format"): a strictly increasing list of
# 128-bit system IDs (type prefix in bits 127-112, sequence in 111-48, bucket in 47-24, 24 reserved bits that are zero;
# system-identity) packs into blocks of at most the block size, a block size of 0 packing one ID per block. A block
# stores its first ID whole and each later ID as the difference of its first 104 bits from those of the ID before it,
# in unsigned LEB128 (7 bits a byte, least significant group first, the high bit set on every byte but the last). The
# pack's bytes: the version byte 1; the block size and the block count, u32 little-endian; per block its first ID as
# 16 big-endian bytes, its ID count, u32 little-endian, and its deltas as a u32 little-endian length and the bytes.
#
# The posting format (posting-lists, "Posting lists are stored in the repository's own format", and the layout
# ultragraphdb_core_codec documents): the version byte 2, the flags (bit 0: a pack follows), the commit timestamp as
# u64 little-endian, the pack's bytes as above, the postings, the splits, and the XXH3-64 (seed 0) of every byte
# before it as u64 little-endian. Every 128-bit value is its 16 big-endian bytes, every other number little-endian.

import json
import random
import sys

import xxhash

SEQUENCE_MAX = 2**64 - 1
BUCKET_MAX = 2**24 - 1
PREFIX_MAX = 2**16 - 1


def system_id(prefix, bucket, sequence):
    """The 128-bit value of the ID of type `prefix`, bucket `bucket` and sequence `sequence`."""
    assert 1 <= prefix <= PREFIX_MAX and 0 <= bucket <= BUCKET_MAX and 1 <= sequence <= SEQUENCE_MAX
    return (prefix << 112) | (sequence << 48) | (bucket << 24)


def leb128(value):
    assert value >= 0
    out = bytearray()
    while True:
        low = value & 0x7F
        value >>= 7
        if value:
            out.append(low | 0x80)
        else:
            out.append(low)
            return bytes(out)


def u32(value):
    return value.to_bytes(4, "little")


def u64(value):
    return value.to_bytes(8, "little")


def u128(value):
    return value.to_bytes(16, "big")


def blocks_of(ids, block_size):
    """The blocks of `ids`: per block its first ID, its ID count and its deltas."""
    step = block_size if block_size > 0 else 1
    blocks = []
    for start in range(0, len(ids), step):
        chunk = ids[start : start + step]
        deltas = b"".join(leb128((b >> 24) - (a >> 24)) for a, b in zip(chunk, chunk[1:]))
        blocks.append((chunk[0], len(chunk), deltas))
    return blocks


def pack_bytes(ids, block_size):
    blocks = blocks_of(ids, block_size)
    out = bytes([1]) + u32(block_size) + u32(len(blocks))
    for first, count, deltas in blocks:
        out += u128(first) + u32(count) + u32(len(deltas)) + deltas
    return out


def hex128(value):
    return f"{value:032x}"


def pack_case(name, block_size, ids):
    assert all(a < b for a, b in zip(ids, ids[1:])), name
    case = {"name": name, "block_size": block_size, "ids": [hex128(i) for i in ids]}
    if ids:
        case["pack"] = pack_bytes(ids, block_size).hex()
        case["blocks"] = [
            {"first": hex128(first), "count": count, "deltas": deltas.hex()}
            for first, count, deltas in blocks_of(ids, block_size)
        ]
    else:
        case["pack"] = None
        case["blocks"] = []
    return case


def out_of_order_case(name, block_size, ids, position):
    return {
        "name": name,
        "block_size": block_size,
        "ids": [hex128(i) for i in ids],
        "out_of_order": position,
    }


def random_ids(rng, n):
    """`n` distinct IDs of types 1 and 2 and buckets 0 and 7, sorted."""
    ids = set()
    while len(ids) < n:
        ids.add(system_id(rng.choice([1, 2]), rng.choice([0, 7]), rng.randrange(1, 2000)))
    return sorted(ids)


def id_list_cases():
    rng = random.Random(20261102)
    lowest = system_id(1, 0, 1)
    highest = system_id(PREFIX_MAX, BUCKET_MAX, SEQUENCE_MAX)
    # Deltas of 1, 127, 128, 2^14 - 1, 2^14 and so on to the largest difference two IDs can have: each LEB128 length
    # from 1 byte to 15. The IDs differ in their buckets and, from 2^24 on, in their sequences.
    deltas = [1, 127, 128, 2**14 - 1, 2**14, 2**21, 2**28, 2**35, 2**42, 2**49, 2**56, 2**63, 2**70, 2**77, 2**84]
    stepped = [lowest]
    for delta in deltas:
        stepped.append(((stepped[-1] >> 24) + delta) << 24)
    cases = [
        pack_case(
            "the scenario: 300 increasing IDs in blocks of 256", 256, [system_id(1, 0, s) for s in range(1, 301)]
        ),
        pack_case(
            "the scenario: sequences 1 and 129 of type 1, bucket 0", 256, [system_id(1, 0, 1), system_id(1, 0, 129)]
        ),
        pack_case("one ID", 256, [system_id(3, 0xABCDEF, 42)]),
        pack_case("no ID", 256, []),
        pack_case("deltas of every LEB128 length from 1 to 13 bytes", 256, stepped),
        pack_case("the largest difference, 15 bytes", 256, [lowest, highest]),
        pack_case(
            "across buckets, sequences and types",
            256,
            [
                system_id(1, 7, 5),
                system_id(1, 8, 5),
                system_id(1, 0, 6),
                system_id(1, BUCKET_MAX, SEQUENCE_MAX),
                system_id(2, 0, 1),
                highest,
            ],
        ),
        pack_case("block size 0: one ID per block", 0, [system_id(1, 0, s) for s in (3, 4, 9)]),
        pack_case("block size 1", 1, [system_id(1, 0, s) for s in (3, 4, 9)]),
        pack_case("block size 2", 2, [system_id(1, 0, s) for s in (5, 6, 7)]),
        pack_case("block size 3, an exact multiple", 3, [system_id(1, 0, s) for s in range(1, 7)]),
        pack_case(
            "sequences 10 to 10,000 in steps of 10, blocks of 256",
            256,
            [system_id(1, 0, s) for s in range(10, 10001, 10)],
        ),
    ]
    for n in (5, 17, 64, 100):
        cases.append(pack_case(f"random: {n} IDs of two types and two buckets, blocks of 10", 10, random_ids(rng, n)))
    a, b, c = system_id(1, 0, 1), system_id(1, 0, 2), system_id(1, 0, 3)
    cases += [
        out_of_order_case("the scenario: the third ID equals the second", 256, [a, b, b], 2),
        out_of_order_case("the third ID below the second", 256, [a, c, b], 2),
        out_of_order_case("the second ID below the first", 256, [b, a], 1),
        out_of_order_case("out of order across a block boundary", 2, [a, b, b], 2),
    ]
    return cases


def posting(uid, op, ref, start, commit, value=b"", lang=b"", facets=(), statements=()):
    out = u128(uid) + u32(op) + bytes([0 if ref else 1]) + u64(start) + u64(commit)
    out += u32(len(value)) + value + u32(len(lang)) + lang
    out += u32(len(facets)) + b"".join(u32(len(f)) + f for f in facets)
    out += u32(len(statements)) + b"".join(u128(s) for s in statements)
    return out


def posting_list(commit, pack=None, postings=(), splits=()):
    body = bytes([2, 1 if pack is not None else 0]) + u64(commit)
    if pack is not None:
        body += pack
    body += u32(len(postings)) + b"".join(postings)
    body += u32(len(splits)) + b"".join(u128(s) for s in splits)
    return body + u64(xxhash.xxh3_64_intdigest(body, seed=0))


def posting_list_cases():
    full_pack_ids = [system_id(1, 0, s) for s in (1, 2, 3, 4)] + [system_id(1, 5, 9)]
    full = posting_list(
        0x1122334455667788,
        pack_bytes(full_pack_ids, 4),
        [
            posting(system_id(1, 0, 2), 1, True, 5, 6, facets=[b"since"]),
            posting(
                0x0123456789ABCDEF,
                3,
                False,
                9,
                10,
                value=bytes([0x01]) + b"abc",
                lang=b"en",
                statements=[system_id(2, 0, 11), system_id(2, BUCKET_MAX, SEQUENCE_MAX)],
            ),
        ],
        [1, system_id(1, 0, 3)],
    )
    return [
        {"name": "empty", "note": "no pack, no posting, no split", "bytes": posting_list(0).hex()},
        {
            "name": "empty pack",
            "note": "a pack of block size 0 and no blocks",
            "bytes": posting_list(0, bytes([1]) + u32(0) + u32(0)).hex(),
        },
        {
            "name": "delete-all",
            "note": "a delta of one delete-all: key 2^64 - 1, op 2, a reference, start timestamp 3, "
            "the value _STAR_ALL",
            "bytes": posting_list(0, None, [posting(2**64 - 1, 2, True, 3, 0, value=b"_STAR_ALL")]).hex(),
        },
        {
            "name": "full",
            "note": "commit timestamp 0x1122334455667788; a pack of block size 4 of the IDs (type 1, bucket 0) with "
            "sequences 1 to 4 and (type 1, bucket 5, sequence 9); a reference to (type 1, bucket 0, sequence 2) with "
            "the facet since, op 1, timestamps 5 and 6; a value posting keyed 0x0123456789abcdef, op 3, timestamps 9 "
            "and 10, the value 01 61 62 63, the language en and the statement IDs (type 2, bucket 0, sequence 11) and "
            "(type 2, the largest bucket and sequence); the splits 1 and (type 1, bucket 0, sequence 3)",
            "bytes": full.hex(),
        },
    ]


def write(directory, name, cases):
    golden = {
        "dgraph_commit": "none: the repository's own formats, written from their specifications, not Dgraph",
        "generator": "ultragraphdb_core/ultragraphdb_core_codec/testdata/golden/formats.py with "
        + f"python-xxhash {xxhash.VERSION} over the xxHash C library {xxhash.XXHASH_VERSION}, "
        + f"Python {sys.version.split()[0]}",
        "cases": cases,
    }
    with open(f"{directory}/{name}", "w", encoding="utf-8") as out:
        json.dump(golden, out, indent=2)
        out.write("\n")


def main():
    directory = sys.argv[1]
    write(directory, "id_lists.json", id_list_cases())
    write(directory, "posting_lists.json", posting_list_cases())


main()
