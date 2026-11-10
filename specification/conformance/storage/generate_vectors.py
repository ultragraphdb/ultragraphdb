#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The storage conformance vectors, laid out byte by byte from ../../storage/STORAGE.md alone, independently of any
# implementation: the primitive encodings of section 4, the term, sortable and token encodings and esc of sections 1 and
# 5, tiles of every shape (sections 6 to 10), digests (section 11), the grid's allocation, superblock, derivation version
# and checkpoint identity (sections 3.2, 11.3, 12), tile keys and the manifest (section 13), the pure decisions of
# sections 16 and 17 (cuts, directory lookups, the queue, checkpoint positions, superblock selection), and the refusals
# of section 19.4. Checksums with Python's xxhash 3.6.0 (libxxhash 0.8.2), the generator the protocol's golden bodies
# use; term hashes with hashlib's SHA-256, truncated to 16 bytes as PROTOCOL.md section 10.1 states.
#
#   python3 -I generate_vectors.py <output directory>
#
# writes vectors/*.json, vectors/tiles/*.tile and vectors/tiles/*.block under the output directory, deterministically:
# running it twice gives the same bytes.

import hashlib
import json
import struct
import sys
from pathlib import Path

import xxhash

ABSENT = 0xFFFF_FFFF
BLOCK = 4096
MASK64 = (1 << 64) - 1
MASK128 = (1 << 128) - 1

# Section 3.1, in the table's order.
SETTINGS = [
    ("extent_bytes", 1_048_576),
    ("grid_extents", 16_384),
    ("tile_bytes_max", 65_536),
    ("tile_entries_max", 4_096),
    ("chunk_slots", 4_096),
    ("filter_bits_per_key", 10),
    ("theta_percent", 50),
    ("delta_entries_max", 5_242_880),
    ("delta_watermark_percent", 75),
    ("delta_row_entries_max", 1_024),
    ("delta_age_rounds", 4_096),
    ("beat_factor", 4),
    ("cold_after_rounds", 65_536),
    ("cleaner_free_percent", 15),
    ("checkpoint_bytes", 268_435_456),
    ("retention_ms", 30_000),
    ("lane_entries_max", 65_536),
    ("lane_bytes_max", 262_144),
]
DEFAULTS = dict(SETTINGS)
# The small grid every tile vector is placed on: section 3.1's defaults but two.
SMALL = [(n, 131_072 if n == "extent_bytes" else 64 if n == "grid_extents" else v) for n, v in SETTINGS]


# ---------------------------------------------------------------- section 1


def u16(v):
    return v.to_bytes(2, "little")


def u32(v):
    return v.to_bytes(4, "little")


def u64(v):
    return v.to_bytes(8, "little")


def be(v, width):
    return v.to_bytes(width, "big")


def align8(n):
    return (n + 7) // 8 * 8


def pad8(b):
    return b + bytes(align8(len(b)) - len(b))


def bitlen(x):
    return x.bit_length()


def xxh64(b):
    return xxhash.xxh3_64_intdigest(b)


def xxh128(b):
    return xxhash.xxh3_128_intdigest(b)


def store128(h):
    return u64(h & MASK64) + u64(h >> 64)


def esc(x):
    """Section 1: every 0x00 written as 0x00 0xFF, then 0x00 0x00."""
    return x.replace(b"\x00", b"\x00\xff") + b"\x00\x00"


def version(epoch, sequence):
    return (epoch << 22) | sequence


def bound(epoch):
    return (epoch << 22) | 0x3FFFFF


def system_id(prefix, sequence, bucket):
    """PROTOCOL.md section 1: type prefix (2), sequence (8), bucket (3), reserved (3), big-endian."""
    return be(prefix, 2) + be(sequence, 8) + be(bucket, 3) + bytes(3)


def group_of(sid):
    return int.from_bytes(sid[10:13], "big") >> 6


# ---------------------------------------------------------------- section 4


def pack(fields, width):
    acc = 0
    for i, f in enumerate(fields):
        assert 0 <= f < (1 << width) or (width == 0 and f == 0)
        acc |= f << (i * width)
    nbytes = (len(fields) * width + 7) // 8
    return pad8(acc.to_bytes(nbytes, "little") if nbytes else b"")


def enc_for(values):
    n = len(values)
    if n == 0:
        return u32(0) + bytes(4) + u64(0)
    lo, hi = min(values), max(values)
    width = bitlen(hi - lo)
    return u32(n) + bytes([width]) + bytes(3) + u64(lo) + pack([v - lo for v in values], width)


def delta_allowed(values):
    return len(values) >= 2 and all(values[i] <= values[i + 1] for i in range(len(values) - 1))


def enc_delta(values):
    deltas = [values[i + 1] - values[i] for i in range(len(values) - 1)]
    dlo, dhi = min(deltas), max(deltas)
    width = bitlen(dhi - dlo)
    return u32(len(values)) + bytes([width]) + bytes(3) + u64(values[0]) + u64(dlo) + pack([d - dlo for d in deltas], width)


def enc_ints(values):
    """Section 4.1's choice: DELTA when allowed and strictly shorter, else FOR. Returns (code, bytes)."""
    f = enc_for(values)
    if delta_allowed(values):
        d = enc_delta(values)
        if len(d) < len(f):
            return 2, d
    return 1, f


def enc_bitmap(bits):
    words = []
    for w in range((len(bits) + 63) // 64):
        word = 0
        for j in range(64):
            i = w * 64 + j
            if i < len(bits) and bits[i]:
                word |= 1 << j
        words.append(u64(word))
    return u32(len(bits)) + u32(0) + b"".join(words)


def enc_bytes(strings):
    ends, total = [], 0
    for s in strings:
        total += len(s)
        ends.append(total)
    return pad8(u32(len(strings)) + u32(total) + b"".join(u32(e) for e in ends) + b"".join(strings))


def enc_fixed(values, width):
    assert all(len(v) == width for v in values)
    return pad8(u32(len(values)) + u32(width) + b"".join(values))


def filter_params(keys, bits_per_key):
    m = max(64, 64 * ((keys * bits_per_key + 63) // 64))
    k = min(16, max(1, (bits_per_key * 693 + 500) // 1000))
    return m, k


def rotl64(x, r):
    return ((x << r) | (x >> (64 - r))) & MASK64


def filter_bits(key, m, k):
    h = xxh64(key)
    g = rotl64(h, 32) | 1
    return [((h + j * g) & MASK64) % m for j in range(k)]


def enc_filter(keys, bits_per_key):
    m, k = filter_params(len(keys), bits_per_key)
    bits = [0] * m
    for x in keys:
        for b in filter_bits(x, m, k):
            bits[b] = 1
    return u32(len(keys)) + bytes([k]) + bytes(3) + u32(m) + u32(0) + enc_bitmap(bits)[8:]


def filter_has(data, key):
    k, m, words = data[4], int.from_bytes(data[8:12], "little"), data[16:]
    return all((words[b // 8] >> (b % 8)) & 1 for b in filter_bits(key, m, k))


# ---------------------------------------------------------------- section 5


def s_int(v, width):
    return be(v + (1 << (8 * width - 1)), width)


def s_uint(v, width):
    return be(v, width)


def s_float(v, width):
    if v != v:
        bits = 0x7FC0_0000 if width == 4 else 0x7FF8_0000_0000_0000
    else:
        if v == 0.0:
            v = 0.0
        bits = int.from_bytes(struct.pack(">f" if width == 4 else ">d", v), "big")
    sign = 1 << (8 * width - 1)
    bits = (~bits & ((1 << (8 * width)) - 1)) if bits & sign else (bits | sign)
    return be(bits, width)


def days_in_month(y, m):
    if m == 2:
        return 29 if (y % 4 == 0 and y % 100 != 0) or y % 400 == 0 else 28
    return 31 if m in (1, 3, 5, 7, 8, 10, 12) else 30


def time_on_timeline(year, month, day, hour, minute, second, tz):
    """PROTOCOL.md section 10.5's T in seconds; a missing year is 1972 (yr 1971), a missing day per its rule."""
    yr = year - 1
    t = 31536000 * yr + 86400 * (yr // 400 - yr // 100 + yr // 4)
    t += 86400 * sum(days_in_month(yr + 1, m) for m in range(1, month))
    t += 86400 * (day - 1) + 3600 * hour + 60 * (minute - tz) + second
    return t


def s_temporal(year, month, day, hour, minute, second, tz):
    t = time_on_timeline(year, month, day, hour, minute, second, 0 if tz is None else tz) * 10**18
    return s_int(t, 16) + s_int(0x7FFF if tz is None else tz, 2)


def s_list(elements):
    return b"".join(be(len(e), 4) + e for e in elements)


def s_set(elements):
    return s_list(sorted(elements))


SORTABLE = [
    ("Bool false", "1", "false", b"\x00"),
    ("Bool true", "1", "true", b"\x01"),
    ("Int8 -128", "2", "-128", s_int(-128, 1)),
    ("Int8 -1", "2", "-1", s_int(-1, 1)),
    ("Int8 0", "2", "0", s_int(0, 1)),
    ("Int8 127", "2", "127", s_int(127, 1)),
    ("Int16 -300", "3", "-300", s_int(-300, 2)),
    ("Int32 -2", "4", "-2", s_int(-2, 4)),
    ("Int32 1000", "4", "1000", s_int(1000, 4)),
    ("Int64 -9223372036854775808", "5", "-9223372036854775808", s_int(-(1 << 63), 8)),
    ("Int64 42", "5", "42", s_int(42, 8)),
    ("Int128 -1", "6", "-1", s_int(-1, 16)),
    ("UInt8 200", "7", "200", s_uint(200, 1)),
    ("UInt16 513", "8", "513", s_uint(513, 2)),
    ("UInt32 4000000000", "9", "4000000000", s_uint(4_000_000_000, 4)),
    ("UInt64 max", "10", "18446744073709551615", s_uint(MASK64, 8)),
    ("Float32 -0.25", "11", "-0.25", s_float(-0.25, 4)),
    ("Float32 -0.0", "11", "-0.0", s_float(-0.0, 4)),
    ("Float32 0.25", "11", "0.25", s_float(0.25, 4)),
    ("Float32 NaN", "11", "NaN", s_float(float("nan"), 4)),
    ("Float64 -inf", "12", "-INF", s_float(float("-inf"), 8)),
    ("Float64 -1.5", "12", "-1.5", s_float(-1.5, 8)),
    ("Float64 -0.0", "12", "-0.0", s_float(-0.0, 8)),
    ("Float64 0.0", "12", "0.0", s_float(0.0, 8)),
    ("Float64 1.5", "12", "1.5", s_float(1.5, 8)),
    ("Float64 inf", "12", "INF", s_float(float("inf"), 8)),
    ("Float64 NaN", "12", "NaN", s_float(float("nan"), 8)),
    ("Decimal(10,2) -12.34", "13 p=10 s=2", "-12.34", s_int(-1234, 16)),
    ("Decimal(10,2) 12.34", "13 p=10 s=2", "12.34", s_int(1234, 16)),
    ("String abc", "14", "abc", b"abc"),
    ("String abd", "14", "abd", b"abd"),
    ("Text with a NUL", "15", "a\\u0000b", b"a\x00b"),
    ("Bytes 00ff", "16", "0x00ff", b"\x00\xff"),
    ("Date 2026-11-10", "17", "2026-11-10", s_temporal(2026, 11, 10, 0, 0, 0, None)),
    ("Time 12:30:00Z", "18", "12:30:00Z", s_temporal(1972, 12, 31, 12, 30, 0, 0)),
    ("DateTime 1970-01-01T00:00:00Z", "19", "1970-01-01T00:00:00Z", s_temporal(1970, 1, 1, 0, 0, 0, 0)),
    ("DateTime 2026-11-10T12:30:00+01:00", "19", "2026-11-10T12:30:00+01:00", s_temporal(2026, 11, 10, 12, 30, 0, 60)),
    ("LocalDateTime 2026-11-10T12:30:00", "20", "2026-11-10T12:30:00", s_temporal(2026, 11, 10, 12, 30, 0, None)),
    ("Duration P1M-shaped", "21", "P1MT1S", s_int(1, 8) + s_int(10**18, 16)),
    ("Iri", "23", "system ID 0003...", system_id(3, 17, 0xABC001)),
    ("Enum member 2", "24", "index 2", s_uint(2, 4)),
    ("List of Int8 [1, -1]", "25 of 2", "[1, -1]", s_list([s_int(1, 1), s_int(-1, 1)])),
    ("Set of Int8 {1, -1}", "26 of 2", "{1, -1}", s_set([s_int(1, 1), s_int(-1, 1)])),
    ("Record (Int8 1, String x)", "27", "{a: 1, b: x}", s_list([s_int(1, 1), b"x"])),
    ("Vector<F32, 2> [1.0, -2.0]", "28 a=1 b=2", "[1.0, -2.0]", struct.pack("<ff", 1.0, -2.0)),
    ("Geo POINT(1 2)", "29", "POINT(1 2)", b"POINT(1 2)"),
    ("Measure 2.5 in unit 3", "30", "2.5, unit number 3", s_float(2.5, 8) + s_uint(3, 4)),
    ("Unit number 7", "31", "unit 7", s_uint(7, 4)),
    ("EntityType prefix 3", "33", "prefix 3", s_uint(3, 2)),
    ("Id128", "36", "0192f3a1b2c3d4e5f60718293a4b5c6d", s_uint(0x0192F3A1B2C3D4E5F60718293A4B5C6D, 16)),
]


def sha16(text):
    return hashlib.sha256(text.encode()).digest()[:16]


def term_vectors():
    """Section 5.1: PROTOCOL.md section 10.1's tags with system IDs in place of hashes for IRIs and datatypes."""
    iri_id = system_id(1, 5, 0xABC010)
    dt_id = system_id(1, 6, 0xABC011)
    long = "a string whose lexical form is longer than fifteen bytes"
    cases = [
        ("an IRI", "<http://ex.org/alice>", b"\x01" + iri_id),
        ("a blank node", "_:b0 named by its skolem IRI", b"\x02" + iri_id),
        ("xsd:boolean true", '"true"^^xsd:boolean', b"\x10\x01"),
        ("xsd:integer 42", '"42"^^xsd:integer', b"\x11" + (42).to_bytes(8, "big", signed=True)),
        ("xsd:integer -1", '"-1"^^xsd:integer', b"\x11" + (-1).to_bytes(8, "big", signed=True)),
        ("xsd:string short", '"abc"', b"\x20" + bytes([3]) + b"abc"),
        ("xsd:string long", f'"{long}"', b"\x21" + sha16(long)),
        ("language-tagged short", '"chat"@fr', b"\x22" + bytes([4]) + b"chat" + bytes([2]) + b"fr"),
        ("another datatype short", '"x"^^<http://ex.org/dt>', b"\x32" + dt_id + bytes([1]) + b"x"),
    ]
    return [{"name": n, "term": t, "hex": b.hex()} for n, t, b in cases]


def token(identifier, body):
    t = bytes([identifier]) + body
    if len(t) > 256:
        return b"\xff" + be(xxh128(t), 16)
    return t


def token_vectors():
    short = token(0x05, s_int(42, 8))
    long_body = b"x" * 300
    return [
        {"name": "range token of an Int64", "identifier": 5, "body_hex": s_int(42, 8).hex(), "hex": short.hex()},
        {"name": "token of 301 bytes", "identifier": 7, "body_hex": long_body.hex(), "hex": token(0x07, long_body).hex()},
        {"name": "token of exactly 256 bytes", "identifier": 7, "body_hex": (b"y" * 255).hex(),
         "hex": token(0x07, b"y" * 255).hex()},
    ]


def esc_vectors():
    pairs = [(b"ab", b"\x01"), (b"abc", b"\x00"), (b"a\x00", b"\x02"), (b"a", b"\xff\xff")]
    joined = [(a.hex(), c.hex(), (esc(a) + c).hex()) for a, c in pairs]
    order = sorted(range(len(pairs)), key=lambda i: (pairs[i][0], pairs[i][1]))
    assert sorted(range(len(pairs)), key=lambda i: esc(pairs[i][0]) + pairs[i][1]) == order
    return {"cases": [{"x_hex": x, "suffix_hex": c, "hex": h} for x, c, h in joined],
            "ascending_indices": order,
            "single": [{"x_hex": x.hex(), "hex": esc(x).hex()} for x in (b"", b"\x00", b"a\x00b", b"\x00\x00")]}


# ---------------------------------------------------------------- section 6


def tile(shape, kind, sections, *, flags=0, tenant=0, graph=bytes(16), subject=bytes(16), type_=0, direction=0,
         order=0, group=ABSENT, built=0, previous=0, horizon=0, entries=0, rows=0, digest=0, span=0, tokenizer=0,
         sort=True):
    sections = sorted(sections, key=lambda s: s[0]) if sort else sections
    offset = 128 + 16 * len(sections)
    directory, body = b"", b""
    for sid, code, data in sections:
        assert len(data) % 8 == 0
        directory += u16(sid) + u16(code) + u32(0) + u32(offset) + u32(len(data))
        body += data
        offset += len(data)
    header = (b"UGTL" + u16(1) + bytes([shape, kind]) + u64(0) + u32(offset) + u16(len(sections)) + u16(flags)
              + u64(tenant) + graph + subject + u16(type_) + bytes([direction, order]) + u32(group)
              + u64(built) + u64(previous) + u64(horizon) + u32(entries) + u32(rows) + store128(digest)
              + u32(span) + bytes([tokenizer]) + bytes(3))
    assert len(header) == 128
    raw = bytearray(header + directory + body)
    raw[8:16] = u64(xxh64(bytes(raw)))
    return bytes(raw)


def refix(raw):
    raw = bytearray(raw)
    raw[8:16] = bytes(8)
    raw[8:16] = u64(xxh64(bytes(raw)))
    return bytes(raw)


def digest_sum(entry_bytes):
    total = 0
    for b in entry_bytes:
        total = (total + xxh128(b)) & MASK128
    return total


def ints_section(sid, values):
    code, data = enc_ints(values)
    return (sid, code, data)


TENANT = 7
GRAPH = bytes(16)
KNOWS = system_id(1, 1001, 0xAB_0102)
AGE = system_id(1, 1003, 0xAB_0104)
PERSON = 3
GROUP = 0xAB_C000 >> 6


def person(seq, bucket):
    return system_id(PERSON, seq, bucket)


# Positions and horizons of the fixtures, horizons never falling as positions rise (HorizonMonotone).
FIXTURE = {"forward": (1200, 900, bound(38)), "reverse": (1300, 0, bound(39)), "column": (1400, 1100, bound(41)),
           "dictionary": (1500, 0, bound(42)), "statistics": (1600, 0, bound(43))}
CHECKPOINT = (1700, bound(43), version(44, 10))


def adjacency_forward():
    """Kind 1: subject slots to object IDs, one inline UInt32 'since', not versioned, ordered by neighbour."""
    built, previous, horizon = FIXTURE["forward"]
    rows = [
        (0, [(person(17, 0xAB_C001), ABSENT, 2019), (person(18, 0xAB_C002), ABSENT, 2021)]),
        (2, [(person(17, 0xAB_C001), ABSENT, 2020)]),
        (5, [(person(20, 0xAB_C004), 9, 2024), (person(21, 0xAB_C001), ABSENT, 2018),
             (person(300, 0xAB_C003), ABSENT, 2017)]),
    ]
    for _, es in rows:
        es.sort(key=lambda e: (e[0], e[1]))
    entries = [(r, e) for r, es in rows for e in es]
    ends = [sum(len(es) for _, es in rows[: i + 1]) for i in range(len(rows))]
    ids = [e[0] for _, e in entries]
    sections = [
        ints_section(1, [r for r, _ in rows]),
        ints_section(2, ends),
        ints_section(3, [int.from_bytes(i[0:2], "big") for i in ids]),
        ints_section(4, [int.from_bytes(i[2:10], "big") for i in ids]),
        ints_section(5, [int.from_bytes(i[10:13], "big") for i in ids]),
        ints_section(8, [e[1] for _, e in entries]),
        (11, 6, enc_filter([be(r, 4) + e[0] for r, e in entries], DEFAULTS["filter_bits_per_key"])),
        (16, 5, enc_fixed([s_uint(e[2], 4) for _, e in entries], 4)),
        (17, 3, enc_bitmap([1] * len(entries))),
    ]
    digest = digest_sum(be(r, 4) + e[0] + be(e[1], 4) + b"\x01" + s_uint(e[2], 4) for r, e in entries)
    raw = tile(1, 1, sections, tenant=TENANT, graph=GRAPH, subject=KNOWS, direction=1, group=GROUP, built=built,
               previous=previous, horizon=horizon, entries=len(entries), rows=len(rows), digest=digest)
    fence = be(rows[0][0], 4) + rows[0][1][0][0]
    return raw, fence, {
        "inline_columns": [{"index": 0, "type": "UInt32", "width": 4}],
        "rows": [{"slot": r, "entries": [{"neighbour": e[0].hex(), "statement": e[1],
                                          "inline": [s_uint(e[2], 4).hex()]} for e in es]} for r, es in rows],
    }


def adjacency_reverse_versioned():
    """Kind 3: object IDs to subject slots, versioned class M, one entry deleted."""
    built, previous, horizon = FIXTURE["reverse"]
    rows = sorted([
        (person(17, 0xAB_C001), [(0, version(40, 2), 0), (2, version(42, 4), 0)]),
        (person(18, 0xAB_C002), [(0, version(41, 6), version(42, 8))]),
    ], key=lambda r: r[0])
    entries = [(k, e) for k, es in rows for e in es]
    ends = [sum(len(es) for _, es in rows[: i + 1]) for i in range(len(rows))]
    sections = [
        (1, 4, enc_bytes([k for k, _ in rows])),
        ints_section(2, ends),
        ints_section(6, [e[0] for _, e in entries]),
        ints_section(9, [e[1] for _, e in entries]),
        ints_section(10, [e[2] for _, e in entries]),
        (11, 6, enc_filter([k + be(e[0], 4) for k, e in entries], DEFAULTS["filter_bits_per_key"])),
    ]
    digest = digest_sum(k + be(e[0], 4) + be(ABSENT, 4) for k, e in entries)
    raw = tile(1, 3, sections, flags=1, tenant=TENANT, graph=GRAPH, subject=KNOWS, direction=2, group=GROUP,
               built=built, previous=previous, horizon=horizon, entries=len(entries), rows=len(rows), digest=digest)
    fence = rows[0][0] + be(rows[0][1][0][0], 4)
    return raw, fence, {
        "rows": [{"key": k.hex(), "entries": [{"slot": e[0], "created": e[1], "deleted": e[2]} for e in es]}
                 for k, es in rows],
    }


def column_chunk():
    """Column kind 1: Int64 'age', span chunk_slots from base 4096, three folded values, two overlay records."""
    built, previous, horizon = FIXTURE["column"]
    base, span = 4096, DEFAULTS["chunk_slots"]
    folded = {0: 31, 1: 45, 4: 22}
    overlay = [(1, version(50, 2), 46), (4, version(50, 4), None)]
    entry_bytes = [be(base + i, 4) + s_int(folded[i], 8) for i in sorted(folded)]
    entry_bytes += [be(base + s, 4) + be(v, 8) + (b"\x01" + s_int(x, 8) if x is not None else b"\x00")
                    for s, v, x in overlay]
    sections = [
        (1, 3, enc_bitmap([1 if i in folded else 0 for i in range(span)])),
        (2, 5, enc_fixed([s_int(folded[i], 8) for i in sorted(folded)], 8)),
        ints_section(3, [s for s, _, _ in overlay]),
        ints_section(4, [v for _, v, _ in overlay]),
        (5, 3, enc_bitmap([1 if x is not None else 0 for _, _, x in overlay])),
        (6, 5, enc_fixed([s_int(x, 8) for _, _, x in overlay if x is not None], 8)),
    ]
    raw = tile(2, 1, sections, flags=1, tenant=TENANT, graph=GRAPH, subject=AGE, type_=PERSON, group=GROUP,
               built=built, previous=previous, horizon=horizon, entries=len(entry_bytes), rows=base,
               digest=digest_sum(entry_bytes), span=span)
    return raw, be(base, 4), {
        "attribute_type": "Int64", "base": base, "span": span,
        "folded": {str(base + i): folded[i] for i in sorted(folded)},
        "overlay": [{"slot": base + s, "version": v, "value": x} for s, v, x in overlay],
    }


def name_bytes(kind, text):
    return bytes([kind]) + text.encode()


def term_key(kind, text):
    nb = name_bytes(kind, text)
    return be(xxh64(nb), 8) + nb


def record_dictionary():
    """Record kind 1, class I: three IRIs to their IDs, all of one bucket group."""
    built, previous, horizon = FIXTURE["dictionary"]
    terms = [("http://ex.org/alice", person(17, 0xAB_C001)), ("http://ex.org/bob", person(18, 0xAB_C002)),
             ("http://ex.org/carol", person(20, 0xAB_C004))]
    recs = sorted((term_key(1, t), i) for t, i in terms)
    sections = [(1, 4, enc_bytes([k for k, _ in recs])), (2, 4, enc_bytes([v for _, v in recs])),
                (5, 6, enc_filter([k for k, _ in recs], DEFAULTS["filter_bits_per_key"]))]
    raw = tile(3, 1, sections, flags=2, tenant=TENANT, group=GROUP, built=built, previous=previous, horizon=horizon,
               entries=len(recs), digest=digest_sum(be(len(k), 4) + k + v for k, v in recs))
    return raw, esc(recs[0][0]), {"records": [{"term_kind": 1, "text": t, "id": i.hex()} for t, i in terms]}


def statistics_value(statements, kinds, subjects, objects):
    regs_s, regs_o = [0] * 64, [0] * 64
    for regs, values in ((regs_s, subjects), (regs_o, objects)):
        for x in values:
            h = xxh64(x)
            y = (h >> 6) | (1 << 58)
            rank = (y & -y).bit_length()
            regs[h % 64] = max(regs[h % 64], rank)
    out = b"\x01" + statements.to_bytes(8, "big", signed=True) + be(len(kinds), 2)
    for name in sorted(kinds, key=lambda s: s.encode()):
        out += be(len(name.encode()), 2) + name.encode() + kinds[name].to_bytes(8, "big", signed=True)
    return out + bytes(regs_s) + bytes(regs_o)


def record_statistics():
    """Record kind 9, class M, versioned: 'knows' after one round of three inserts, in the default graph of tenant 7."""
    built, previous, horizon = FIXTURE["statistics"]
    subjects = [person(1, 0xAB_C001), person(1, 0xAB_C001), person(2, 0xAB_C002)]
    objects = [person(17, 0xAB_C001), person(18, 0xAB_C002), person(17, 0xAB_C001)]
    value = statistics_value(3, {"iri": 3}, subjects, objects)
    created = version(44, 10)
    sections = [(1, 4, enc_bytes([KNOWS])), (2, 4, enc_bytes([value])), ints_section(3, [created]),
                ints_section(4, [0]), (5, 6, enc_filter([KNOWS], DEFAULTS["filter_bits_per_key"]))]
    raw = tile(3, 9, sections, flags=1, tenant=TENANT, graph=GRAPH, group=ABSENT, built=built, previous=previous,
               horizon=horizon, entries=1, digest=digest_sum([be(16, 4) + KNOWS + value]))
    return raw, esc(KNOWS), {"predicate": KNOWS.hex(), "statements": 3, "kinds": {"iri": 3},
                             "subjects": [s.hex() for s in subjects], "objects": [o.hex() for o in objects],
                             "created": created, "value": value.hex()}


def free_set_tile(grid_extents, allocated, built, previous, horizon):
    bits = [1 if e in allocated else 0 for e in range(grid_extents)]
    return tile(4, 1, [(1, 3, enc_bitmap(bits))], group=ABSENT, built=built, previous=previous, horizon=horizon,
                entries=len(allocated), digest=digest_sum(be(e, 4) for e in sorted(allocated)))


# ---------------------------------------------------------------- sections 11 to 13


def reference(address, raw):
    return u64(address) + u32(len(raw)) + u32(0) + raw[8:16]


def family_key(raw):
    h = raw[:128]
    le = lambda a, b: int.from_bytes(h[a:b], "little")
    klass = 1 if le(22, 24) & 2 else 0
    return (bytes([h[6], h[7]]) + be(le(24, 32), 8) + h[32:48] + h[48:64] + be(le(64, 66), 2) + bytes([h[66]])
            + bytes([klass, h[124]]) + be(le(68, 72), 4))


def tile_key(raw, fence):
    fk = family_key(raw)
    assert len(fk) == 51
    return fk + fence


def derivation(settings):
    data = u16(1) + u16(1) + b"".join(u64(v) for _, v in settings)
    assert len(data) == 148
    return xxh64(data)


def identity(fmt, deriv, position, horizon, watermark, manifest_checksum, free_checksum):
    data = u16(fmt) + b"".join(u64(x) for x in (deriv, position, horizon, watermark, manifest_checksum, free_checksum))
    assert len(data) == 50
    return xxh128(data)


def superblock(copy, *, parent, sequence, cluster, partition, replica, deriv, position, horizon, watermark,
               extent_bytes, grid_extents, manifest_ref, free_ref, mapping, log_bytes=0):
    ident = identity(1, deriv, position, horizon, watermark, int.from_bytes(manifest_ref[16:24], "little"),
                     int.from_bytes(free_ref[16:24], "little"))
    body = (b"UGDBGRID" + u16(1) + bytes(6) + u64(0) + u64(parent) + u64(sequence) + u64(cluster) + u32(partition)
            + u32(replica) + u64(deriv) + u64(position) + u64(horizon) + u64(watermark) + u32(extent_bytes)
            + u32(grid_extents) + manifest_ref + free_ref + u64(mapping) + store128(ident) + u64(log_bytes))
    assert len(body) == 176
    raw = bytearray(body + bytes(4096 - len(body)))
    checksum = xxh64(bytes(raw))
    raw[16:24] = u64(checksum)
    raw[10] = copy
    return bytes(raw), checksum, ident


class Allocator:
    """Section 12.4: three streams, each with an open extent and a cursor; a new extent is the lowest free one."""

    def __init__(self, extent_bytes, grid_extents):
        self.blocks_per_extent = extent_bytes // BLOCK
        self.extent_bytes = extent_bytes
        self.free = list(range(1, grid_extents))
        self.used = {0}
        self.open = {}

    def place(self, length, stream):
        b = (length + BLOCK - 1) // BLOCK
        cur = self.open.get(stream)
        if cur is None or cur[1] + b > self.blocks_per_extent:
            extent = self.free.pop(0)
            self.used.add(extent)
            cur = [extent, 0]
            self.open[stream] = cur
        address = cur[0] * self.extent_bytes + cur[1] * BLOCK
        cur[1] += b
        return address


def stream_of(built, previous, cold_after_rounds):
    return 2 if previous != 0 and built - previous >= cold_after_rounds else 1


# ---------------------------------------------------------------- sections 16 and 17, pure decisions


def cut(rows, entries_max):
    """Section 16.3 step 3: greedy cut of rows (their entry counts) into outputs; returns each output's
    (row index, first entry within the row, entries taken) pieces."""
    outputs, current, held = [], [], 0
    for i, n in enumerate(rows):
        if n <= entries_max:
            if held + n > entries_max and current:
                outputs.append(current)
                current, held = [], 0
            current.append((i, 0, n))
            held += n
        else:
            if current:
                outputs.append(current)
            current, held = [], 0
            start = 0
            while n - start > entries_max:
                outputs.append([(i, start, entries_max)])
                start += entries_max
            current, held = [(i, start, n - start)], n - start
    if current:
        outputs.append(current)
    return outputs


def lookup(fences, key):
    """Section 13.2: the tile with the greatest fence at or below the key, else the family's first tile."""
    best = 0
    for i, f in enumerate(fences):
        if f <= key:
            best = i
    return best


def queue_order(tiles):
    order_of = {1: 0, 4: 1, 5: 2, 2: 3, 3: 4}
    return [t["name"] for t in sorted(tiles, key=lambda t: (order_of[t["reason"]], -t["d"], bytes.fromhex(t["key"])))]


def checkpoint_positions(frame_lengths, checkpoint_bytes):
    """Section 17.1: the first position at which the total reaches k × C, for each k reached."""
    out, total, k = [], 0, 1
    for position, length in enumerate(frame_lengths, start=1):
        total += length
        if total >= k * checkpoint_bytes:
            out.append(position)
            while total >= k * checkpoint_bytes:
                k += 1
    return out


def select_superblock(copies):
    """Section 17.2 steps 1 and 2 over the four blocks: the kept copies and the chosen sequence, or a refusal."""
    kept = []
    for block, raw in enumerate(copies):
        if raw[:8] != b"UGDBGRID" or int.from_bytes(raw[8:10], "little") != 1 or raw[10] != block:
            continue
        probe = bytearray(raw)
        probe[10] = 0
        probe[16:24] = bytes(8)
        if xxh64(bytes(probe)) != int.from_bytes(raw[16:24], "little"):
            continue
        kept.append(block)
    if not kept:
        return {"kept": [], "result": "SuperblockLost" if any(any(c) for c in copies) else "new grid"}
    seq = lambda b: int.from_bytes(copies[b][32:40], "little")
    top = max(seq(b) for b in kept)
    tops = [b for b in kept if seq(b) == top]
    strip = lambda b: copies[b][:10] + copies[b][11:]
    if any(strip(b) != strip(tops[0]) for b in tops):
        return {"kept": kept, "result": "SuperblockDiverged"}
    return {"kept": kept, "result": f"sequence {top}"}


# ---------------------------------------------------------------- vectors


def encodings_vectors():
    cases = []

    def ints_case(name, values):
        code, data = enc_ints(values)
        cases.append({"name": name, "kind": "ints", "values": [str(v) for v in values], "encoding": code,
                      "hex": data.hex(), "for_hex": enc_for(values).hex(),
                      "delta_hex": enc_delta(values).hex() if delta_allowed(values) else None})

    ints_case("empty", [])
    ints_case("one value", [7])
    ints_case("all equal", [5, 5, 5, 5])
    ints_case("small unsorted", [3, 1, 4, 1, 5, 9, 2, 6])
    ints_case("sorted slots, FOR shorter", [0, 2, 5])
    ints_case("dense sorted run, DELTA width 0", list(range(1_000_000, 1_000_040)))
    ints_case("sorted with varying gaps, DELTA width 3", [1 << 40] + [(1 << 40) + 3 * i + (i % 5) for i in range(1, 30)])
    ints_case("sorted large gaps", [1, 1 << 40, (1 << 40) + 3, 1 << 50])
    ints_case("versions of one epoch", [version(42, s) for s in range(0, 64, 2)])
    ints_case("unsorted full width, FOR width 64", [MASK64, 0, 1])
    ints_case("two values full range, DELTA shorter", [0, MASK64])
    cases.append({"name": "bitmap of 70", "kind": "bitmap", "bits": [i % 3 == 0 for i in range(70)],
                  "hex": enc_bitmap([1 if i % 3 == 0 else 0 for i in range(70)]).hex()})
    cases.append({"name": "bitmap empty", "kind": "bitmap", "bits": [], "hex": enc_bitmap([]).hex()})
    strs = [b"", b"a", b"bcd", "été".encode()]
    cases.append({"name": "bytes", "kind": "bytes", "strings_hex": [s.hex() for s in strs], "hex": enc_bytes(strs).hex()})
    cases.append({"name": "bytes empty", "kind": "bytes", "strings_hex": [], "hex": enc_bytes([]).hex()})
    fixed = [s_int(-1, 8), s_int(0, 8), s_int(1, 8)]
    cases.append({"name": "fixed width 8", "kind": "fixed", "width": 8, "values_hex": [v.hex() for v in fixed],
                  "hex": enc_fixed(fixed, 8).hex()})
    cases.append({"name": "fixed width 3", "kind": "fixed", "width": 3, "values_hex": ["616263", "78797a"],
                  "hex": enc_fixed([b"abc", b"xyz"], 3).hex()})
    cases.append({"name": "fixed empty", "kind": "fixed", "width": 18, "values_hex": [], "hex": enc_fixed([], 18).hex()})
    for bpk, keys in ((10, [b"alpha", b"beta", b"gamma"]), (10, []), (4, [be(i, 4) for i in range(20)]),
                      (10, [b"dup", b"dup", b"other"]), (32, [b"alpha", b"beta"])):
        m, k = filter_params(len(keys), bpk)
        data = enc_filter(keys, bpk)
        cases.append({"name": f"filter of {len(keys)} keys at {bpk} bits", "kind": "filter", "bits_per_key": bpk,
                      "keys_hex": [x.hex() for x in keys], "m": m, "k": k, "hex": data.hex(),
                      "probes": [{"key_hex": x.hex(), "may_be_present": filter_has(data, x)}
                                 for x in keys[:3] + [b"delta", b"epsilon"]]})
    return {"specification": "STORAGE.md section 4", "cases": cases}


def sortable_vectors():
    by = {n: b for n, _, _, b in SORTABLE}
    orders = [
        ["Int8 -128", "Int8 -1", "Int8 0", "Int8 127"],
        ["Float32 -0.25", "Float32 0.25", "Float32 NaN"],
        ["Float64 -inf", "Float64 -1.5", "Float64 0.0", "Float64 1.5", "Float64 inf", "Float64 NaN"],
        ["Decimal(10,2) -12.34", "Decimal(10,2) 12.34"],
        ["String abc", "String abd"],
        ["DateTime 1970-01-01T00:00:00Z", "DateTime 2026-11-10T12:30:00+01:00"],
    ]
    for o in orders:
        assert all(by[o[i]] < by[o[i + 1]] for i in range(len(o) - 1)), o
    assert by["Float64 -0.0"] == by["Float64 0.0"]
    return {"specification": "STORAGE.md sections 1 and 5", "sortable": [
        {"name": n, "tag": t, "value": v, "hex": b.hex()} for n, t, v, b in SORTABLE],
        "ascending": orders, "equal": [["Float64 -0.0", "Float64 0.0"]],
        "terms": term_vectors(), "tokens": token_vectors(), "esc": esc_vectors()}


def decision_vectors():
    cuts = []
    for name, rows, emax in (("small rows fill outputs", [100, 200, 300, 4000, 50], 4096),
                             ("a hub row cut into full pieces, its tail open", [10, 9000, 20, 4096], 4096),
                             ("a row of exactly the bound", [4096, 1], 4096),
                             ("no rows", [], 4096)):
        cuts.append({"name": name, "row_entries": rows, "tile_entries_max": emax,
                     "outputs": [[{"row": r, "first": f, "entries": n} for r, f, n in o] for o in cut(rows, emax)]})
    fences = [be(0, 4) + person(1, 0xABC001), be(4, 4) + person(1, 0xABC001), be(4, 4) + person(900, 0xABC001)]
    lookups = [{"key_hex": k.hex(), "tile": lookup(fences, k)} for k in (
        be(0, 4) + person(0, 0), be(3, 4) + person(5, 0xABC001), be(4, 4) + person(1, 0xABC001),
        be(4, 4) + person(899, 0xFFFFFF), be(9, 4) + bytes(16))]
    q = [{"name": "a", "reason": 2, "d": 50, "key": "01"}, {"name": "b", "reason": 1, "d": 3, "key": "02"},
         {"name": "c", "reason": 2, "d": 80, "key": "03"}, {"name": "d", "reason": 3, "d": 900, "key": "00"},
         {"name": "e", "reason": 2, "d": 80, "key": "02"}, {"name": "f", "reason": 4, "d": 1, "key": "09"}]
    frames = [4096] * 10 + [12288, 4096, 40960, 4096]
    sb = {"cluster": 1, "partition": 1, "replica": 1, "extent_bytes": 131_072, "grid_extents": 64, "mapping": 1}
    ref = bytes(24)
    old, _, _ = superblock(0, parent=0, sequence=4, deriv=1, position=10, horizon=0, watermark=0, manifest_ref=ref,
                           free_ref=ref, **sb)
    new, cks, _ = superblock(0, parent=1, sequence=5, deriv=1, position=20, horizon=0, watermark=0, manifest_ref=ref,
                             free_ref=ref, **sb)
    other, _, _ = superblock(0, parent=1, sequence=5, deriv=1, position=21, horizon=0, watermark=0, manifest_ref=ref,
                             free_ref=ref, **sb)

    def as_copy(raw, c):
        r = bytearray(raw)
        r[10] = c
        return bytes(r)

    torn = bytearray(as_copy(new, 2))
    torn[200] = 1
    scenarios = {
        "all four of sequence 5": [as_copy(new, c) for c in range(4)],
        "crash during step 4: copies 0 and 1 new": [as_copy(new, 0), as_copy(new, 1), as_copy(old, 2), as_copy(old, 3)],
        "one torn copy": [as_copy(new, 0), as_copy(new, 1), bytes(torn), as_copy(new, 3)],
        "two copies of sequence 5 differ": [as_copy(new, 0), as_copy(other, 1), as_copy(old, 2), as_copy(old, 3)],
        "all zero": [bytes(4096)] * 4,
        "none valid, not zero": [bytes(torn)] * 4,
        "copy field out of place": [as_copy(new, 1), as_copy(new, 1), as_copy(new, 2), as_copy(new, 3)],
    }
    selections = [{"name": n, "copies_hex_xxh3_64": [f"{xxh64(c):016x}" for c in cs], "copies_hex": [c.hex() for c in cs],
                   **select_superblock(cs)} for n, cs in scenarios.items()]
    return {"specification": "STORAGE.md sections 12.4, 13.2, 16.2, 16.3, 17.1 and 17.2",
            "cuts": cuts,
            "lookups": {"fences_hex": [f.hex() for f in fences], "cases": lookups},
            "queue": {"tiles": q, "order": queue_order(q)},
            "streams": [{"built": b, "previous": p, "cold_after_rounds": 65_536,
                         "stream": stream_of(b, p, 65_536)} for b, p in ((1200, 0), (70_000, 4_464), (70_000, 4_465),
                                                                           (70_000, 69_999))],
            "checkpoints": {"frame_lengths": frames, "checkpoint_bytes": 16_384,
                            "positions": checkpoint_positions(frames, 16_384)},
            "superblock_selection": selections}


# ---------------------------------------------------------------- tile kinds beyond the fixtures (sections 7 to 9)


def record_run(kind, recs, *, class_i, tenant=TENANT, graph=GRAPH, group=ABSENT, built=1800, horizon=bound(44),
               versioned_recs=None):
    """A record run of `recs` (key, value) pairs, or of `versioned_recs` (key, value, created, deleted) when given."""
    if versioned_recs is not None:
        rows = sorted(versioned_recs, key=lambda r: (r[0], r[2]))
        keys, values = [r[0] for r in rows], [r[1] for r in rows]
        sections = [(1, 4, enc_bytes(keys)), (2, 4, enc_bytes(values)), ints_section(3, [r[2] for r in rows])]
        if not class_i:
            sections.append(ints_section(4, [r[3] for r in rows]))
        flags = 1 | (2 if class_i else 0)
    else:
        rows = sorted(recs)
        keys, values = [r[0] for r in rows], [r[1] for r in rows]
        sections = [(1, 4, enc_bytes(keys)), (2, 4, enc_bytes(values))]
        flags = 2 if class_i else 0
    sections.append((5, 6, enc_filter(keys, DEFAULTS["filter_bits_per_key"])))
    raw = tile(3, kind, sections, flags=flags, tenant=tenant, graph=graph, group=group, built=built, horizon=horizon,
               entries=len(rows), digest=digest_sum(be(len(k), 4) + k + v for k, v in zip(keys, values)))
    return raw, {"records": [{"key_hex": k.hex(), "value_hex": v.hex()} for k, v in zip(keys, values)]}


def adjacency_values(kind, rows, *, subject, direction, tokenizer=0, slot_rows=True):
    """Adjacency kinds 2, 4 and 5: rows of (row key, [neighbour bytes or slot]), not versioned."""
    rows = sorted(rows, key=lambda r: r[0])
    entries = [(r, n) for r, ns in rows for n in sorted(ns)]
    ends = [sum(len(ns) for _, ns in rows[: i + 1]) for i in range(len(rows))]
    if slot_rows:
        rows_sec = ints_section(1, [int.from_bytes(r, "big") for r, _ in rows])
    else:
        rows_sec = (1, 4, enc_bytes([r for r, _ in rows]))
    if kind == 4:
        nbr_sec = ints_section(6, [n for _, n in entries])
        nbr_bytes = [be(n, 4) for _, n in entries]
    else:
        nbr_sec = (7, 4, enc_bytes([n for _, n in entries]))
        nbr_bytes = [n for _, n in entries]
    join = lambda r, nb: (r if len(r) in (4, 16) else esc(r)) + (nb if kind == 4 else esc(nb))
    sections = [rows_sec, ints_section(2, ends), nbr_sec,
                (11, 6, enc_filter([r + nb for (r, _), nb in zip(entries, nbr_bytes)], DEFAULTS["filter_bits_per_key"]))]
    digest = digest_sum(r + nb + be(ABSENT, 4) for (r, _), nb in zip(entries, nbr_bytes))
    raw = tile(1, kind, sections, tenant=TENANT, graph=GRAPH, subject=subject, direction=direction, group=GROUP,
               built=1800, horizon=bound(44), entries=len(entries), rows=len(rows), digest=digest, tokenizer=tokenizer)
    fence = join(rows[0][0], nbr_bytes[0])
    return raw, fence, {"rows": [{"key_hex": r.hex(), "neighbours": [n if isinstance(n, int) else n.hex()
                                                                      for n in sorted(ns)]} for r, ns in rows]}


def column_variable():
    """Column kind 2: a String attribute, not versioned, three values from base 0."""
    folded = {0: b"Ada", 3: b"", 7: "Gréta".encode()}
    span = DEFAULTS["chunk_slots"]
    sections = [(1, 3, enc_bitmap([1 if i in folded else 0 for i in range(span)])),
                (2, 4, enc_bytes([folded[i] for i in sorted(folded)]))]
    name = system_id(1, 1005, 0xAB_0106)
    raw = tile(2, 2, sections, tenant=TENANT, graph=GRAPH, subject=name, type_=PERSON, group=GROUP, built=1800,
               horizon=bound(44), entries=len(folded), rows=0, span=span,
               digest=digest_sum(be(i, 4) + folded[i] for i in sorted(folded)))
    return raw, be(0, 4), {"attribute_type": "String", "base": 0, "span": span,
                           "folded": {str(i): folded[i].hex() for i in sorted(folded)}}


def column_identity():
    """Column kind 3: the identity column of persons in the group, slots 0 to 2."""
    ids = {0: (17, 0xAB_C001), 1: (18, 0xAB_C002), 2: (20, 0xAB_C004)}
    span = DEFAULTS["chunk_slots"]
    values = [be(ids[i][0], 8) + be(ids[i][1], 3) for i in sorted(ids)]
    raw = tile(2, 3, [(1, 3, enc_bitmap([1 if i in ids else 0 for i in range(span)])), (2, 5, enc_fixed(values, 11))],
               tenant=TENANT, type_=PERSON, group=GROUP, built=1800, horizon=bound(44), entries=3, rows=0, span=span,
               digest=digest_sum(be(i, 4) + v for i, v in zip(sorted(ids), values)))
    return raw, be(0, 4), {"slots": {str(i): {"sequence": s, "bucket": b} for i, (s, b) in ids.items()}}


def more_tiles():
    """Every kind sections 7 to 9 define that the fixtures above leave out."""
    alice, bob = person(17, 0xAB_C001), person(18, 0xAB_C002)
    out = {}
    out["adjacency_forward_values"] = adjacency_values(
        2, [(be(0, 4), [b"\x20\x03Ada", b"\x11" + (42).to_bytes(8, "big", signed=True)]), (be(2, 4), [b"\x20\x00"])],
        subject=system_id(1, 1006, 0xAB_0107), direction=1)
    out["adjacency_index"] = adjacency_values(
        4, [(token(0x05, s_int(31, 8)), [0, 4]), (token(0x05, s_int(45, 8)), [1])],
        subject=AGE, direction=0, tokenizer=5, slot_rows=False)
    out["adjacency_version_run"] = adjacency_values(
        5, [(be(0, 4), [be(version(40, 2), 8) + b"draft", be(version(42, 6), 8) + b"final"])],
        subject=system_id(1, 1007, 0xAB_0108), direction=1)
    out["column_variable"] = column_variable()
    out["column_identity"] = column_identity()
    dict_id_to_term = record_run(2, [(alice, name_bytes(1, "http://ex.org/alice")), (bob, name_bytes(1, "http://ex.org/bob"))],
                                 class_i=True, group=GROUP)
    long = "a string whose lexical form is longer than fifteen bytes"
    runs = {
        "record_id_to_term": dict_id_to_term,
        "record_hash_to_text": record_run(3, [(sha16(long), long.encode())], class_i=True),
        "record_key_claim": record_run(4, [(be(PERSON, 2) + b"\x01alice", alice)], class_i=False, group=GROUP),
        "record_unique_claim": record_run(5, [(AGE + s_int(31, 8), alice)], class_i=False, group=ABSENT),
        "record_mark": record_run(6, [(alice + be(GROUP, 3), be(2, 8))], class_i=False, group=GROUP),
        "record_referrer_set": record_run(7, [(alice, be(GROUP, 3) + be(40, 8) + be(1, 8)
                                                 + be(GROUP + 1, 3) + be(41, 8) + be(3, 8))], class_i=False, group=GROUP),
        "record_tombstone": record_run(8, [(bob, u64(version(43, 2)))], class_i=False, group=GROUP),
        "record_session": record_run(10, [(bytes(range(16)) + be(0, 4),
                                           u64(5) + u64(version(44, 10)) + u64(1600) + u32(4) + u32(0) + b"\x01\x02\x03\x04")],
                                     class_i=False, tenant=0, group=ABSENT),
        "record_slot_map": record_run(11, [(be(17, 8), u32(0)), (be(18, 8), u32(1)), (be(20, 8), u32(2))], class_i=True,
                                      group=GROUP),
        "record_set_combination": record_run(12, [(be(9, 2) + alice + bob, system_id(9, 3, 0xAB_C001))], class_i=False,
                                             group=GROUP),
        "record_idempotency_claim": record_run(13, [(be(PERSON, 2) + be(0x0192F3A1B2C3D4E5F60718293A4B5C6D, 16), alice)],
                                               class_i=True, group=GROUP),
        "record_drop_marker": record_run(14, [(be(version(45, 0), 8), u16(2) + bytes(6) + system_id(2, 1, 0xAB_0109))],
                                         class_i=False, group=ABSENT),
        "record_build": record_run(15, [(be(1650, 8), u64(3) + AGE + be(5, 4) + alice)], class_i=False, group=ABSENT),
        "record_statement": record_run(18, [(system_id(9, 4, 0xAB_C001),
                                             GRAPH + alice + KNOWS + b"\x01" + bob
                                             + enc_bytes([AGE + b"\x11" + (2019).to_bytes(8, "big", signed=True)]))],
                                       class_i=False, group=GROUP),
        "record_mark_versioned": record_run(6, [], class_i=False, group=GROUP, versioned_recs=[
            (alice + be(GROUP, 3), be(1, 8), version(43, 2), version(45, 2)),
            (alice + be(GROUP, 3), be(2, 8), version(45, 2), 0)]),
    }
    for name, (raw, desc) in runs.items():
        keys = [bytes.fromhex(r["key_hex"]) for r in desc["records"]]
        out[name] = (raw, esc(min(keys)), desc)
    return out


def halving_vector(settings):
    """Section 16.3 step 4: one row of tile_entries_max entries whose encoding exceeds tile_bytes_max, cut in two at
    entry n div 2 and again until each half fits."""
    emax, bmax = settings["tile_entries_max"], settings["tile_bytes_max"]
    fam = {"kind": 1, "direction": 1, "subject": KNOWS}
    # Neighbours with scattered sequences and buckets, so packing gains little.
    entries = sorted((be(0, 4), person((i * 2654435761) % (1 << 40) + 1, (i * 40503) % (1 << 24)), 0, 0)
                     for i in range(emax))
    pieces = halve(fam, entries, 1900, 0, bound(44), settings)
    return {"settings": {"tile_entries_max": emax, "tile_bytes_max": bmax},
            "entries": len(entries), "neighbours_hex": [e[1].hex() for e in entries],
            "pieces": [{"entries": len(es), "length": len(raw), "checksum": f"{int.from_bytes(raw[8:16], 'little'):016x}"}
                       for es, raw in pieces]}


def column_fold_vector():
    """Section 11.2: folding the column fixture's overlay at H = bound(50): the records fold into folded values."""
    raw, _, _ = column_chunk()
    base, span = 4096, DEFAULTS["chunk_slots"]
    folded_in = {0: 31, 1: 45, 4: 22}
    overlay = [(1, version(50, 2), 46), (4, version(50, 4), None)]
    h = bound(50)
    folded = dict(folded_in)
    for s, v, x in overlay:
        if v <= h:
            if x is None:
                folded.pop(s, None)
            else:
                folded[s] = x
    ent_in = [be(base + i, 4) + s_int(folded_in[i], 8) for i in sorted(folded_in)]
    ent_in += [be(base + s, 4) + be(v, 8) + (b"\x01" + s_int(x, 8) if x is not None else b"\x00") for s, v, x in overlay]
    ent_out = [be(base + i, 4) + s_int(folded[i], 8) for i in sorted(folded)]
    removed = [e for e in ent_in if e not in ent_out]
    added = [e for e in ent_out if e not in ent_in]
    out = tile(2, 1, [(1, 3, enc_bitmap([1 if i in folded else 0 for i in range(span)])),
                      (2, 5, enc_fixed([s_int(folded[i], 8) for i in sorted(folded)], 8))],
               tenant=TENANT, graph=GRAPH, subject=AGE, type_=PERSON, group=GROUP, built=1900, previous=1400,
               horizon=h, entries=len(ent_out), rows=base, span=span, digest=digest_sum(ent_out))
    lhs = digest_sum(ent_out)
    rhs = (digest_sum(ent_in) + digest_sum(added) - digest_sum(removed)) & MASK128
    assert lhs == rhs
    return out, {"input_file": "tiles/column_chunk_overlay.tile", "horizon": h,
                 "added_hex": [e.hex() for e in added], "removed_hex": [e.hex() for e in removed],
                 "input_digest": raw[104:120].hex(), "output_digest": out[104:120].hex(),
                 "conserved": lhs == rhs}


# ---------------------------------------------------------------- the reference engine (sections 12 to 17)
#
# A replica holding one relation, knows, in tenant 7's default graph and one bucket group: its forward family (kind 1)
# and its reverse family (kind 3), class M, no statements and no inline column. The rounds name subject slots
# directly (slot maps are record runs outside this model). Everything else follows sections 12 to 17 as written: apply,
# due rules, the queue, units, rewrites with merge, fold, cut, halving and the next-tile merge, placement and streams,
# the cleaner, checkpoints with the drain, the manifest, the free set and the superblock, and recovery.

ENGINE_SETTINGS = [
    ("extent_bytes", 131_072), ("grid_extents", 64), ("tile_bytes_max", 16_384), ("tile_entries_max", 256),
    ("chunk_slots", 4_096), ("filter_bits_per_key", 10), ("theta_percent", 50), ("delta_entries_max", 65_536),
    ("delta_watermark_percent", 75), ("delta_row_entries_max", 64), ("delta_age_rounds", 64), ("beat_factor", 4),
    ("cold_after_rounds", 1_024), ("cleaner_free_percent", 15), ("checkpoint_bytes", 16_777_216),
    ("retention_ms", 30_000), ("lane_entries_max", 65_536), ("lane_bytes_max", 262_144),
]
FAMILIES = {
    "forward": {"kind": 1, "direction": 1, "subject": KNOWS},
    "reverse": {"kind": 3, "direction": 2, "subject": KNOWS},
}
REASON_RANK = {1: 0, 4: 1, 5: 2, 2: 3, 3: 4}


def fam_key(fam):
    return (bytes([1, fam["kind"]]) + be(TENANT, 8) + GRAPH + fam["subject"] + be(0, 2) + bytes([fam["direction"]])
            + bytes([0, 0]) + be(GROUP, 4))


def encode_adjacency(fam, entries, built, previous, horizon, settings):
    """Sections 7 and 14.5: entries (row, neighbour, created, deleted), sorted, created already 0 at or below H."""
    rows = []
    for r, n, c, d in entries:
        if rows and rows[-1][0] == r:
            rows[-1][1].append((n, c, d))
        else:
            rows.append((r, [(n, c, d)]))
    versioned = any(c != 0 or d != 0 for _, _, c, d in entries)
    ends = [sum(len(es) for _, es in rows[: i + 1]) for i in range(len(rows))]
    if fam["kind"] == 1:
        sections = [ints_section(1, [int.from_bytes(r, "big") for r, _ in rows]), ints_section(2, ends),
                    ints_section(3, [int.from_bytes(n[0:2], "big") for _, n, _, _ in entries]),
                    ints_section(4, [int.from_bytes(n[2:10], "big") for _, n, _, _ in entries]),
                    ints_section(5, [int.from_bytes(n[10:13], "big") for _, n, _, _ in entries])]
    else:
        sections = [(1, 4, enc_bytes([r for r, _ in rows])), ints_section(2, ends),
                    ints_section(6, [int.from_bytes(n, "big") for _, n, _, _ in entries])]
    if versioned:
        sections += [ints_section(9, [c for _, _, c, _ in entries]), ints_section(10, [d for _, _, _, d in entries])]
    if settings["filter_bits_per_key"] > 0:
        sections.append((11, 6, enc_filter([r + n for r, n, _, _ in entries], settings["filter_bits_per_key"])))
    digest = digest_sum(r + n + be(ABSENT, 4) for r, n, _, _ in entries)
    return tile(1, fam["kind"], sections, flags=1 if versioned else 0, tenant=TENANT, graph=GRAPH,
                subject=fam["subject"], direction=fam["direction"], group=GROUP, built=built, previous=previous,
                horizon=horizon, entries=len(entries), rows=len(rows), digest=digest)


def halve(fam, entries, built, previous, horizon, settings):
    """Section 16.3 step 4, returning [(entries, raw)] in order."""
    raw = encode_adjacency(fam, entries, built, previous, horizon, settings)
    if len(raw) <= settings["tile_bytes_max"]:
        return [(entries, raw)]
    n = len(entries)
    m = n // 2
    boundaries = [i for i in range(1, n) if entries[i][0] != entries[i - 1][0]]
    if boundaries:
        cut_at = min(boundaries, key=lambda b: (abs(b - m), b))
    else:
        cut_at = m
    return halve(fam, entries[:cut_at], built, previous, horizon, settings) + \
        halve(fam, entries[cut_at:], built, previous, horizon, settings)


class Tile:
    def __init__(self, fam_name, entries, raw):
        self.fam = fam_name
        self.entries = entries
        self.raw = raw
        self.address = None
        self.built = int.from_bytes(raw[72:80], "little")

    @property
    def fence(self):
        return self.entries[0][0] + self.entries[0][1]

    @property
    def key(self):
        return fam_key(FAMILIES[self.fam]) + self.fence

    @property
    def blocks(self):
        return (len(self.raw) + BLOCK - 1) // BLOCK


class Engine:
    def __init__(self, settings, name):
        self.s = dict(settings)
        self.name = name
        self.E = self.s["extent_bytes"]
        self.G = self.s["grid_extents"]
        self.grid = bytearray(self.E * self.G)
        self.state = ["closed"] + ["free"] * (self.G - 1)      # extent 0 holds the superblock
        self.stream_of_extent = [None] * self.G
        self.cursor = [0] * self.G
        self.live = [0] * self.G
        self.open = {}
        self.directory = {f: [] for f in FAMILIES}
        self.delta = []                                       # dicts: fam, row, nbr, op, version, position, taken
        self.queue = []                                       # items: (rank, -d, key, target)
        self.in_progress = []
        self.units = 0
        self.position = 0
        self.horizon = 0
        self.watermark = 0
        self.log_bytes = 0
        self.next_checkpoint = self.s["checkpoint_bytes"]
        self.sequence = 0
        self.parent = 0
        self.leaves = []                                      # [(records, Tile-like dict)]
        self.root = None
        self.free_set = None
        self.events = []
        self.checkpoints = []

    # -- allocation (section 12.4)
    def place(self, raw, stream):
        b = (len(raw) + BLOCK - 1) // BLOCK
        e = self.open.get(stream)
        if e is None or self.cursor[e] + b > self.E // BLOCK:
            if e is not None:
                self.state[e] = "closed"
            e = next(x for x in range(1, self.G) if self.state[x] == "free")
            self.state[e], self.stream_of_extent[e], self.cursor[e] = "open", stream, 0
            self.open[stream] = e
        address = e * self.E + self.cursor[e] * BLOCK
        self.cursor[e] += b
        self.grid[address:address + len(raw)] = raw
        self.grid[address + len(raw):address + b * BLOCK] = bytes(b * BLOCK - len(raw))
        self.live[e] += b
        return address

    def release(self, address, raw):
        e = address // self.E
        self.live[e] -= (len(raw) + BLOCK - 1) // BLOCK
        assert self.live[e] >= 0

    # -- directory (section 13.2)
    def tile_of(self, fam, row, nbr):
        tiles = self.directory[fam]
        if not tiles:
            return ("new", fam)
        k = row + nbr
        best = tiles[0]
        for t in tiles:
            if t.fence <= k:
                best = t
        return best

    # -- apply (section 15)
    def apply(self, rnd):
        self.position = rnd["position"]
        self.horizon = rnd["horizon"]
        assert self.horizon >= 0
        changes = 0
        for commit in rnd["commits"]:
            v = commit["version"]
            self.watermark = v
            for op, items in (("insert", commit["inserts"]), ("delete", commit["deletes"])):
                for slot, obj_hex in items:
                    obj = bytes.fromhex(obj_hex)
                    self.delta.append({"fam": "forward", "row": be(slot, 4), "nbr": obj, "op": op, "version": v,
                                       "position": self.position, "taken": False})
                    self.delta.append({"fam": "reverse", "row": obj, "nbr": be(slot, 4), "op": op, "version": v,
                                       "position": self.position, "taken": False})
                    changes += 2
        self.log_bytes += rnd["frame_length"]
        self.boundary(changes)

    def groups(self):
        groups = {}
        for c in self.delta:
            if c["taken"]:
                continue
            t = self.tile_of(c["fam"], c["row"], c["nbr"])
            groups.setdefault(id(t) if not isinstance(t, tuple) else t, (t, []))[1].append(c)
        return groups

    def target_key(self, t):
        return fam_key(FAMILIES[t[1]]) if isinstance(t, tuple) else t.key

    def busy(self, t):
        for rw in self.in_progress:
            if any(x is t or x == t for x in rw["inputs"]):
                return True
        return any(q[3] is t or q[3] == t for q in self.queue)

    # -- due rules and the queue (sections 16.1 and 16.2)
    def evaluate(self):
        for t, cs in self.groups().values():
            if self.busy(t):
                continue
            d = len(cs)
            b = 0 if isinstance(t, tuple) else len(t.entries)
            per_row = {}
            for c in cs:
                per_row[c["row"]] = per_row.get(c["row"], 0) + 1
            reasons = []
            if max(per_row.values()) >= self.s["delta_row_entries_max"]:
                reasons.append(1)
            if d * 100 >= self.s["theta_percent"] * max(b, 1):
                reasons.append(2)
            if min(c["position"] for c in cs) <= self.position - self.s["delta_age_rounds"]:
                reasons.append(3)
            if not reasons:
                continue
            reason = min(reasons, key=lambda r: REASON_RANK[r])
            self.queue.append((REASON_RANK[reason], -d, self.target_key(t), t))
        self.queue.sort(key=lambda q: (q[0], q[1], q[2]))

    # -- a rewrite (section 16.3)
    def start(self, t):
        fam = t[1] if isinstance(t, tuple) else t.fam
        tiles = [] if isinstance(t, tuple) else [t]
        famtiles = self.directory[fam]
        if tiles:
            i = famtiles.index(t)
            emax = self.s["tile_entries_max"]
            if i + 1 < len(famtiles):
                nxt = famtiles[i + 1]
                nxt_busy = any(any(x is nxt for x in rw["inputs"]) for rw in self.in_progress)
                if len(t.entries) < emax // 4 and len(t.entries) + len(nxt.entries) <= emax and not nxt_busy:
                    tiles.append(nxt)
                    self.queue = [q for q in self.queue if q[3] is not nxt]
        groups = self.groups()
        taken = []
        for key in ([id(x) for x in tiles] if tiles else [t]):
            if key in groups:
                taken.extend(groups[key][1])
        taken.sort(key=lambda c: self.delta.index(c))
        for c in taken:
            c["taken"] = True
        entries = [list(e) for x in tiles for e in x.entries]
        before = [e[0] + e[1] for e in entries]
        for c in taken:
            if c["op"] == "insert":
                entries.append([c["row"], c["nbr"], c["version"], 0])
            else:
                match = [e for e in entries if e[0] == c["row"] and e[1] == c["nbr"] and e[3] == 0]
                assert len(match) == 1, "a delete names exactly one visible entry"
                match[0][3] = c["version"]
        h = self.horizon
        kept = []
        for e in entries:
            if e[3] != 0 and e[3] <= h:
                continue
            if e[2] <= h:
                e[2] = 0
            kept.append(tuple(e))
        kept.sort(key=lambda e: (e[0], e[1], e[2]))
        after = [e[0] + e[1] for e in kept]
        outputs = []
        famd = FAMILIES[fam]
        previous = tiles[0].built if tiles else 0
        for piece in self.cut(kept):
            for es, raw in halve(famd, piece, self.position, previous, h, self.s):
                outputs.append(Tile(fam, es, raw))
        # Conservation (section 11.2), entries identified by their digest bytes.
        multiset = lambda xs: {x: xs.count(x) for x in set(xs)}
        mb, ma = multiset(before), multiset(after)
        added = [x for x in ma for _ in range(max(0, ma[x] - mb.get(x, 0)))]
        removed = [x for x in mb for _ in range(max(0, mb[x] - ma.get(x, 0)))]
        hashes = lambda xs: digest_sum(x + be(ABSENT, 4) for x in xs)
        lhs = sum(int.from_bytes(o.raw[104:112], "little") | (int.from_bytes(o.raw[112:120], "little") << 64)
                  for o in outputs) & MASK128
        rhs = (sum(int.from_bytes(x.raw[104:112], "little") | (int.from_bytes(x.raw[112:120], "little") << 64)
                   for x in tiles) + hashes(added) - hashes(removed)) & MASK128
        assert lhs == rhs, "DigestConserved"
        cost = sum(len(x.entries) for x in tiles) + len(taken) + sum(len(o.entries) for o in outputs)
        return {"target": t, "inputs": tiles if tiles else [t], "tiles": tiles, "taken": taken, "outputs": outputs,
                "remaining": cost, "decided": self.position, "fam": fam}

    def cut(self, kept):
        rows, counts = [], []
        for e in kept:
            if rows and rows[-1][0][0] == e[0]:
                rows[-1].append(e)
            else:
                rows.append([e])
        pieces = []
        for out in cut([len(r) for r in rows], self.s["tile_entries_max"]):
            pieces.append([e for (ri, first, n) in out for e in rows[ri][first:first + n]])
        return [p for p in pieces if p]

    def complete(self, rw):
        fam = rw["fam"]
        for x in rw["tiles"]:
            self.directory[fam].remove(x)
            self.release(x.address, x.raw)
        self.delta = [c for c in self.delta if not (c["taken"] and any(c is t for t in rw["taken"]))]
        for o in sorted(rw["outputs"], key=lambda o: o.key):
            b, p = o.built, int.from_bytes(o.raw[80:88], "little")
            stream = 2 if p != 0 and b - p >= self.s["cold_after_rounds"] else 1
            o.address = self.place(o.raw, stream)
            self.directory[fam].append(o)
        self.directory[fam].sort(key=lambda o: o.fence)
        self.events.append({"position": self.position, "decided": rw["decided"], "family": fam,
                            "inputs": [x.key.hex() for x in rw["tiles"]],
                            "outputs": [{"key": o.key.hex(), "address": o.address, "length": len(o.raw),
                                         "checksum": f"{int.from_bytes(o.raw[8:16], 'little'):016x}"}
                                        for o in sorted(rw["outputs"], key=lambda o: o.key)]})

    # -- the beat (section 16.4)
    def beat(self, earned, drain=False):
        pool = self.units + earned
        for rw in list(self.in_progress):
            pay = rw["remaining"] if drain else min(pool, rw["remaining"])
            rw["remaining"] -= pay
            pool -= 0 if drain else pay
            if rw["remaining"] == 0:
                self.in_progress.remove(rw)
                self.complete(rw)
        while self.queue and (pool > 0 or drain):
            _, _, _, t = self.queue.pop(0)
            rw = self.start(t)
            self.in_progress.append(rw)
            pay = rw["remaining"] if drain else min(pool, rw["remaining"])
            rw["remaining"] -= pay
            pool -= 0 if drain else pay
            if rw["remaining"] == 0:
                self.in_progress.remove(rw)
                self.complete(rw)
        if not self.queue and not self.in_progress:
            pool = 0
        self.units = pool

    # -- the cleaner (section 16.6)
    def free_count(self):
        return sum(1 for e in range(1, self.G)
                   if self.state[e] == "free" or (self.state[e] == "closed" and self.live[e] == 0))

    def clean(self):
        if self.free_count() * 100 >= self.s["cleaner_free_percent"] * self.G:
            return
        busy = {x.address // self.E for rw in self.in_progress for x in rw["tiles"]}
        cands = [e for e in range(1, self.G) if self.state[e] == "closed" and self.live[e] > 0
                 and self.stream_of_extent[e] != 0 and e not in busy]
        if not cands:
            return
        e = min(cands, key=lambda x: (self.live[x], x))
        moving = sorted((t for f in self.directory for t in self.directory[f] if t.address // self.E == e),
                        key=lambda t: t.address)
        for t in moving:
            self.release(t.address, t.raw)
            t.address = self.place(t.raw, 2)
        self.events.append({"position": self.position, "cleaner": e, "moved": len(moving)})

    # -- a round boundary
    def boundary(self, changes):
        self.evaluate()
        self.beat(self.s["beat_factor"] * changes)
        self.clean()
        if self.log_bytes >= self.next_checkpoint:
            self.checkpoint()
            self.next_checkpoint = (self.log_bytes // self.s["checkpoint_bytes"] + 1) * self.s["checkpoint_bytes"]
        self.check_relation_digests()

    def check_relation_digests(self):
        def live(fam):
            state = {}
            for t in self.directory[fam]:
                for r, n, c, d in t.entries:
                    if d == 0:
                        state[(r, n)] = state.get((r, n), 0) + 1
            for c in self.delta:
                if c["fam"] == fam:
                    k = (c["row"], c["nbr"])
                    state[k] = state.get(k, 0) + (1 if c["op"] == "insert" else -1)
            return {k for k, v in state.items() if v > 0}
        fwd = {(r, n) for r, n in live("forward")}
        rev = {(n, r) for r, n in live("reverse")}
        assert fwd == rev, "RelationDigests"

    # -- checkpoints (section 17.1)
    def checkpoint(self):
        # Step 0: the drain.
        self.beat(0, drain=True)
        while any(not c["taken"] for c in self.delta):
            groups = sorted(self.groups().values(), key=lambda g: self.target_key(g[0]))
            rw = self.start(groups[0][0])
            self.complete(rw)
        assert not self.delta and not self.queue and not self.in_progress
        self.units = 0
        self.clean()
        # Step 1.
        for s, e in list(self.open.items()):
            self.state[e] = "closed"
        self.open = {}
        # Step 2: the manifest.
        records = sorted((t.key, reference(t.address, t.raw) + t.raw[96:100] + u32(0))
                         for f in self.directory for t in self.directory[f])
        leaves = []
        current = []
        for rec in records:
            trial = current + [rec]
            raw = self.leaf_raw(trial, 0, 0)
            if current and (len(trial) > self.s["tile_entries_max"] or len(raw) > self.s["tile_bytes_max"]):
                leaves.append(current)
                current = [rec]
            else:
                current = trial
        if current:
            leaves.append(current)
        new_leaves = []
        for i, recs in enumerate(leaves):
            prev = self.leaves[i] if i < len(self.leaves) else None
            if prev is not None and prev["records"] == recs:
                new_leaves.append(prev)
                continue
            raw = self.leaf_raw(recs, self.position, prev["built"] if prev else 0)
            new_leaves.append({"records": recs, "raw": raw, "built": self.position, "address": None})
        for lf in new_leaves:
            if lf["address"] is None:
                lf["address"] = self.place(lf["raw"], 0)
        root_recs = [(lf["records"][0][0], reference(lf["address"], lf["raw"])) for lf in new_leaves]
        root_raw = self.run_raw(17, root_recs, self.position, self.root["built"] if self.root else 0)
        root_at = self.place(root_raw, 0)
        fs_probe = free_set_tile(self.G, set(), self.position, 0, self.horizon)
        fs_at = self.place(fs_probe, 0)
        marked = {0} | {e for e in range(1, self.G) if self.state[e] != "free"}
        fs_raw = free_set_tile(self.G, marked, self.position, self.free_set["built"] if self.free_set else 0,
                               self.horizon)
        self.grid[fs_at:fs_at + len(fs_raw)] = fs_raw
        # The previous checkpoint's stream 0 tiles stop being live.
        kept_leaves = {id(lf) for lf in new_leaves}
        for lf in self.leaves:
            if id(lf) not in kept_leaves:
                self.release(lf["address"], lf["raw"])
        for old in (self.root, self.free_set):
            if old is not None:
                self.release(old["address"], old["raw"])
        self.leaves = new_leaves
        self.root = {"raw": root_raw, "address": root_at, "built": self.position}
        self.free_set = {"raw": fs_raw, "address": fs_at, "built": self.position}
        for e in list(self.open.values()):
            self.state[e] = "closed"
        self.open = {}
        # Step 4: the superblock.
        self.sequence += 1
        watermark = self.watermark
        copies = []
        for c in range(4):
            raw, checksum, ident = superblock(
                c, parent=self.parent, sequence=self.sequence, cluster=0x0123_4567_89AB_CDEF, partition=1, replica=1,
                deriv=derivation(ENGINE_SETTINGS), position=self.position, horizon=self.horizon, watermark=watermark,
                extent_bytes=self.E, grid_extents=self.G, manifest_ref=reference(root_at, root_raw),
                free_ref=reference(fs_at, fs_raw), mapping=1, log_bytes=self.log_bytes)
            self.grid[c * BLOCK:(c + 1) * BLOCK] = raw
            copies.append(raw)
        self.parent = checksum
        # Step 6: free and punch.
        for e in range(1, self.G):
            if self.state[e] == "closed" and self.live[e] == 0:
                self.state[e] = "free"
                self.stream_of_extent[e] = None
                self.grid[e * self.E:(e + 1) * self.E] = bytes(self.E)
        self.checkpoints.append({"position": self.position, "log_bytes": self.log_bytes, "sequence": self.sequence,
                                 "superblock_checksum": f"{checksum:016x}", "identity": store128(ident).hex(),
                                 "tiles": sum(len(v) for v in self.directory.values()), "leaves": len(new_leaves),
                                 "extents_allocated": sorted(marked),
                                 "grid_xxh3_64": f"{xxh64(bytes(self.grid)):016x}"})

    def run_raw(self, kind, recs, built, previous):
        keys, values = [k for k, _ in recs], [v for _, v in recs]
        return tile(3, kind, [(1, 4, enc_bytes(keys)), (2, 4, enc_bytes(values)),
                              (5, 6, enc_filter(keys, self.s["filter_bits_per_key"]))],
                    flags=2, group=ABSENT, built=built, previous=previous, horizon=self.horizon, entries=len(recs),
                    digest=digest_sum(be(len(k), 4) + k + v for k, v in recs))

    def leaf_raw(self, recs, built, previous):
        return self.run_raw(16, recs, built, previous)

    # -- recovery (section 17.2)
    def recover(self, grid):
        self.__init__(ENGINE_SETTINGS, self.name)
        self.grid = bytearray(grid)
        sel = select_superblock([bytes(grid[c * BLOCK:(c + 1) * BLOCK]) for c in range(4)])
        assert sel["result"].startswith("sequence"), sel
        sb = bytes(grid[sel["kept"][0] * BLOCK:(sel["kept"][0] + 1) * BLOCK])
        f = lambda a, b: int.from_bytes(sb[a:b], "little")
        self.sequence, self.position, self.horizon, self.watermark = f(32, 40), f(64, 72), f(72, 80), f(80, 88)
        self.log_bytes = f(168, 176)
        self.next_checkpoint = (self.log_bytes // self.s["checkpoint_bytes"] + 1) * self.s["checkpoint_bytes"]
        self.parent = f(16, 24)
        read = lambda ref: bytes(grid[int.from_bytes(ref[0:8], "little"):
                                      int.from_bytes(ref[0:8], "little") + int.from_bytes(ref[8:12], "little")])
        root_ref, fs_ref = sb[96:120], sb[120:144]
        root_raw, fs_raw = read(root_ref), read(fs_ref)
        for raw, ref in ((root_raw, root_ref), (fs_raw, fs_ref)):
            assert raw[8:16] == ref[16:24], "Misdirected"
        self.root = {"raw": root_raw, "address": int.from_bytes(root_ref[0:8], "little"),
                     "built": int.from_bytes(root_raw[72:80], "little")}
        self.free_set = {"raw": fs_raw, "address": int.from_bytes(fs_ref[0:8], "little"),
                         "built": int.from_bytes(fs_raw[72:80], "little")}
        marked = decode_bitmap(section_of(fs_raw, 1))
        self.leaves = []
        for lkey, lref in run_records(root_raw):
            leaf_raw = read(lref)
            assert leaf_raw[8:16] == lref[16:24], "Misdirected"
            recs = run_records(leaf_raw)
            self.leaves.append({"records": recs, "raw": leaf_raw, "built": int.from_bytes(leaf_raw[72:80], "little"),
                                "address": int.from_bytes(lref[0:8], "little")})
            for tkey, val in recs:
                ref = val[:24]
                raw = read(ref)
                assert raw[8:16] == ref[16:24], "Misdirected"
                fam = "forward" if raw[7] == 1 else "reverse"
                t = Tile(fam, decode_adjacency(raw), raw)
                t.address = int.from_bytes(ref[0:8], "little")
                self.directory[fam].append(t)
        for fam in self.directory:
            self.directory[fam].sort(key=lambda o: o.fence)
        for t in [t for f in self.directory for t in self.directory[f]]:
            self.live[t.address // self.E] += t.blocks
        for x in self.leaves + [self.root, self.free_set]:
            self.live[x["address"] // self.E] += (len(x["raw"]) + BLOCK - 1) // BLOCK
            self.stream_of_extent[x["address"] // self.E] = 0
        for e in range(1, self.G):
            self.state[e] = "closed" if marked[e] and self.live[e] > 0 else "free"
            if self.state[e] == "free":
                self.stream_of_extent[e] = None


def section_of(raw, sid):
    n = int.from_bytes(raw[20:22], "little")
    for i in range(n):
        d = raw[128 + 16 * i:144 + 16 * i]
        if int.from_bytes(d[0:2], "little") == sid:
            off, ln = int.from_bytes(d[8:12], "little"), int.from_bytes(d[12:16], "little")
            return raw[off:off + ln]
    return None


def decode_bitmap(data):
    n = int.from_bytes(data[0:4], "little")
    return [(data[8 + i // 8] >> (i % 8)) & 1 for i in range(n)]


def decode_bytes(data):
    n = int.from_bytes(data[0:4], "little")
    ends = [int.from_bytes(data[8 + 4 * i:12 + 4 * i], "little") for i in range(n)]
    base = 8 + 4 * n
    out, start = [], 0
    for e in ends:
        out.append(data[base + start:base + e])
        start = e
    return out


def unpack(data, offset, n, width):
    acc = int.from_bytes(data[offset:], "little")
    mask = (1 << width) - 1
    return [(acc >> (i * width)) & mask for i in range(n)]


def decode_vector(raw, sid):
    """A FOR or DELTA section's values; the directory names which (section 6.3)."""
    n_sec = int.from_bytes(raw[20:22], "little")
    for i in range(n_sec):
        d = raw[128 + 16 * i:144 + 16 * i]
        if int.from_bytes(d[0:2], "little") != sid:
            continue
        code = int.from_bytes(d[2:4], "little")
        off, ln = int.from_bytes(d[8:12], "little"), int.from_bytes(d[12:16], "little")
        data = raw[off:off + ln]
        n, width = int.from_bytes(data[0:4], "little"), data[4]
        if code == 1:
            base = int.from_bytes(data[8:16], "little")
            return [base + x for x in unpack(data, 16, n, width)] if n else []
        first, dbase = int.from_bytes(data[8:16], "little"), int.from_bytes(data[16:24], "little")
        vals = [first]
        for x in unpack(data, 24, n - 1, width):
            vals.append(vals[-1] + dbase + x)
        return vals
    return None


def decode_adjacency(raw):
    kind = raw[7]
    ends = decode_vector(raw, 2)
    if kind == 1:
        rows = [be(x, 4) for x in decode_vector(raw, 1)]
        pre, seq, buk = decode_vector(raw, 3), decode_vector(raw, 4), decode_vector(raw, 5)
        nbrs = [system_id(p, s, b) for p, s, b in zip(pre, seq, buk)]
    else:
        rows = decode_bytes(section_of(raw, 1))
        nbrs = [be(x, 4) for x in decode_vector(raw, 6)]
    created = decode_vector(raw, 9) or [0] * len(nbrs)
    deleted = decode_vector(raw, 10) or [0] * len(nbrs)
    out, start = [], 0
    for r, end in zip(rows, ends):
        for i in range(start, end):
            out.append((r, nbrs[i], created[i], deleted[i]))
        start = end
    return out


def run_records(raw):
    return list(zip(decode_bytes(section_of(raw, 1)), decode_bytes(section_of(raw, 2))))


class Lcg:
    """A 64-bit linear congruential generator; the rounds it draws are written into the vector, so no implementation
    needs to reproduce it."""

    def __init__(self, seed):
        self.x = seed

    def next(self, n):
        self.x = (self.x * 6364136223846793005 + 1442695040888963407) & MASK64
        return (self.x >> 33) % n


def engine_rounds(count=220):
    rng = Lcg(2026_11_10)
    objects = [person(seq, 0xAB_C000 + (seq % 64)) for seq in range(1, 401)]
    linked = set()
    rounds = []
    for r in range(count):
        position, epoch = r + 1, 100 + r
        commits = []
        for k in range(1 + rng.next(3)):
            inserts, deletes = [], []
            for _ in range(6 + rng.next(9)):
                slot = 0 if rng.next(4) == 0 else 1 + rng.next(39)
                obj = objects[rng.next(len(objects))]
                if (slot, obj) not in linked and (slot, obj.hex()) not in [(s, o) for s, o in inserts]:
                    inserts.append((slot, obj.hex()))
            for _ in range(rng.next(4)):
                if linked:
                    pick = sorted(linked)[rng.next(len(linked))]
                    if (pick[0], pick[1].hex()) not in deletes:
                        deletes.append((pick[0], pick[1].hex()))
            for s, o in deletes:
                linked.discard((s, bytes.fromhex(o)))
            for s, o in inserts:
                linked.add((s, bytes.fromhex(o)))
            commits.append({"version": version(epoch, 2 * k), "inserts": inserts, "deletes": deletes})
        ops = sum(len(c["inserts"]) + len(c["deletes"]) for c in commits)
        rounds.append({"position": position, "horizon": bound(epoch - 5) if r >= 5 else 0,
                       "frame_length": 1_048_576 + 4096 * ops, "commits": commits})
    return rounds


def engine_vector(vec):
    rounds = engine_rounds()
    a = Engine(ENGINE_SETTINGS, "a")
    for rnd in rounds:
        a.apply(rnd)
    # A second replica crashes between its second and third checkpoints, recovers from the grid alone and replays.
    second = a.checkpoints[1]["position"]
    crash_at = second + 5
    b = Engine(ENGINE_SETTINGS, "b")
    for rnd in rounds[:crash_at]:
        b.apply(rnd)
    survived = bytes(b.grid)
    b.recover(survived)
    assert b.position == second
    for rnd in rounds[second:]:
        b.apply(rnd)
    same_checkpoints = [x for x in a.checkpoints if x["position"] > second] == \
        [x for x in b.checkpoints if x["position"] > second]
    same_grid = bytes(a.grid) == bytes(b.grid)
    assert same_checkpoints and same_grid, "replay after recovery differs"
    blocks = bytearray()
    for i in range(len(a.grid) // BLOCK):
        blk = a.grid[i * BLOCK:(i + 1) * BLOCK]
        if any(blk):
            blocks += u32(i) + blk
    (vec / "tiles" / "engine_grid.blocks").write_bytes(bytes(blocks))
    return {
        "specification": "STORAGE.md sections 12 to 17, over one relation's forward and reverse families",
        "settings": dict(ENGINE_SETTINGS), "derivation": f"{derivation(ENGINE_SETTINGS):016x}",
        "families": {n: {"kind": f["kind"], "direction": f["direction"], "subject": f["subject"].hex(),
                         "tenant": TENANT, "graph": GRAPH.hex(), "group": GROUP, "family_key": fam_key(f).hex()}
                     for n, f in FAMILIES.items()},
        "superblock_fields": {"cluster": 0x0123_4567_89AB_CDEF, "partition": 1, "replica": 1, "mapping": 1,
                              "watermark": "the last commit version applied"},
        "rounds": rounds,
        "events": a.events,
        "checkpoints": a.checkpoints,
        "final": {"position": a.position, "grid_xxh3_64": f"{xxh64(bytes(a.grid)):016x}",
                  "grid_blocks_file": "tiles/engine_grid.blocks",
                  "grid_blocks_format": "per nonzero block: its index as a u32 then its 4,096 bytes, ascending"},
        "crash": {"after_position": crash_at, "recovered_to": second,
                  "replayed_checkpoints_equal": same_checkpoints, "final_grid_equal": same_grid},
    }


def main(out):
    vec = Path(out) / "vectors"
    (vec / "tiles").mkdir(parents=True, exist_ok=True)

    def write_json(name, obj):
        (vec / name).write_text(json.dumps(obj, indent=2) + "\n")

    write_json("encodings.json", encodings_vectors())
    write_json("sortable.json", sortable_vectors())
    write_json("decisions.json", decision_vectors())

    small = dict(SMALL)
    data = {
        "adjacency_forward": adjacency_forward(),
        "adjacency_reverse_versioned": adjacency_reverse_versioned(),
        "column_chunk_overlay": column_chunk(),
        "record_dictionary": record_dictionary(),
        "record_statistics": record_statistics(),
    }
    # Place the data tiles as compaction would at their boundaries, in built order, then the checkpoint's stream 0.
    alloc = Allocator(small["extent_bytes"], small["grid_extents"])
    placed = {}
    for name, (raw, fence, _) in sorted(data.items(), key=lambda kv: int.from_bytes(kv[1][0][72:80], "little")):
        b, p = int.from_bytes(raw[72:80], "little"), int.from_bytes(raw[80:88], "little")
        placed[name] = alloc.place(len(raw), stream_of(b, p, small["cold_after_rounds"]))
    for s in list(alloc.open):
        del alloc.open[s]
    position, horizon, watermark = CHECKPOINT
    leaf_recs = sorted((tile_key(raw, fence), reference(placed[name], raw) + raw[96:100] + u32(0))
                       for name, (raw, fence, _) in data.items())
    leaf = tile(3, 16, [(1, 4, enc_bytes([k for k, _ in leaf_recs])), (2, 4, enc_bytes([v for _, v in leaf_recs])),
                        (5, 6, enc_filter([k for k, _ in leaf_recs], 10))],
                flags=2, group=ABSENT, built=position, horizon=horizon, entries=len(leaf_recs),
                digest=digest_sum(be(len(k), 4) + k + v for k, v in leaf_recs))
    leaf_at = alloc.place(len(leaf), 0)
    root_recs = [(leaf_recs[0][0], reference(leaf_at, leaf))]
    root = tile(3, 17, [(1, 4, enc_bytes([k for k, _ in root_recs])), (2, 4, enc_bytes([v for _, v in root_recs])),
                        (5, 6, enc_filter([k for k, _ in root_recs], 10))],
                flags=2, group=ABSENT, built=position, horizon=horizon, entries=1,
                digest=digest_sum(be(len(k), 4) + k + v for k, v in root_recs))
    root_at = alloc.place(len(root), 0)
    probe = free_set_tile(small["grid_extents"], set(), position, 0, horizon)
    free_at = alloc.place(len(probe), 0)
    fs = free_set_tile(small["grid_extents"], set(alloc.used), position, 0, horizon)
    assert len(fs) == len(probe)

    tiles = {}
    for name, (raw, fence, desc) in data.items():
        tiles[name] = {"input": desc, "header": header_fields(raw), "fence_hex": fence.hex(),
                       "tile_key_hex": tile_key(raw, fence).hex(), "address": placed[name],
                       "file": f"tiles/{name}.tile", "length": len(raw),
                       "checksum": f"{int.from_bytes(raw[8:16], 'little'):016x}", "digest": raw[104:120].hex()}
    for name, raw, at in (("manifest_leaf", leaf, leaf_at), ("manifest_root", root, root_at), ("free_set", fs, free_at)):
        tiles[name] = {"header": header_fields(raw), "address": at, "file": f"tiles/{name}.tile", "length": len(raw),
                       "checksum": f"{int.from_bytes(raw[8:16], 'little'):016x}", "digest": raw[104:120].hex()}
    tiles["free_set"]["input"] = {"grid_extents": small["grid_extents"], "allocated": sorted(alloc.used)}
    for name, entry in tiles.items():
        raw = {**{n: d[0] for n, d in data.items()}, "manifest_leaf": leaf, "manifest_root": root, "free_set": fs}[name]
        (vec / "tiles" / f"{name}.tile").write_bytes(raw)
    write_json("tiles.json", {"specification": "STORAGE.md sections 6 to 13",
                              "settings": "section 3.1's defaults, extent_bytes 131072 and grid_extents 64",
                              "tiles": tiles})

    deriv_small = derivation(SMALL)
    sb_args = dict(parent=0, sequence=1, cluster=0x0123_4567_89AB_CDEF, partition=1, replica=1, deriv=deriv_small,
                   position=position, horizon=horizon, watermark=watermark, extent_bytes=small["extent_bytes"],
                   grid_extents=small["grid_extents"], manifest_ref=reference(root_at, root),
                   free_ref=reference(free_at, fs), mapping=1)
    copies = []
    for c in range(4):
        raw, checksum, ident = superblock(c, **sb_args)
        (vec / "tiles" / f"superblock_copy{c}.block").write_bytes(raw)
        copies.append(f"tiles/superblock_copy{c}.block")

    alloc2 = Allocator(131_072, 64)
    script = [(20_000, 1200, 900), (60_000, 1201, 0), (50_000, 1202, 1100), (65_536, 70_000, 1), (8, 70_001, 0),
              (65_536, 70_002, 70_000), (12_288, 70_003, 0)]
    placements = []
    for length, b, p in script:
        s = stream_of(b, p, 65_536)
        placements.append({"length": length, "built": b, "previous": p, "stream": s, "address": alloc2.place(length, s)})

    write_json("grid.json", {
        "specification": "STORAGE.md sections 3.2, 6.4, 11.3, 12 and 13",
        "derivation": {"defaults": f"{derivation(SETTINGS):016x}",
                       "small_grid": {"settings": dict(SMALL), "value": f"{deriv_small:016x}"}},
        "allocation": {"extent_bytes": 131_072, "grid_extents": 64, "cold_after_rounds": 65_536,
                       "placements": placements},
        "checkpoint": {"note": "the tiles of tiles.json placed in built order, then the leaf, the root and the free set",
                       "extents_allocated": sorted(alloc.used)},
        "superblock": {"input": {k: (v.hex() if isinstance(v, bytes) else v) for k, v in sb_args.items()},
                       "copies": copies, "checksum": f"{checksum:016x}", "identity": store128(ident).hex()},
    })

    fwd = data["adjacency_forward"][0]
    refusals = []

    def tile_case(name, raw, refusal, note=""):
        refusals.append({"name": name, "structure": "tile", "hex": raw.hex(), "refusal": refusal, "note": note})

    bad = bytearray(fwd)
    bad[0] = ord("X")
    tile_case("magic", bytes(bad), "Magic")
    bad = bytearray(fwd)
    bad[4:6] = u16(2)
    tile_case("format 2", refix(bad), "Format")
    tile_case("length past tile_bytes_max", refix(fwd[:16] + u32(70_000) + fwd[20:]), "Length")
    bad = bytearray(fwd)
    bad[-1] ^= 0x01
    tile_case("a flipped bit", bytes(bad), "Checksum")
    tile_case("checksum holds, reference names another", fwd, "Misdirected",
              "read through a reference whose checksum is 0000000000000001")
    bad = bytearray(fwd)
    bad[6] = 9
    tile_case("shape 9", refix(bad), "Shape")
    bad = bytearray(fwd)
    bad[66] = 2
    tile_case("forward block with direction 2", refix(bad), "Shape", "section 7.1: kind 1 gives direction 1")
    bad = bytearray(fwd)
    bad[22:24] = u16(4)
    tile_case("flags bit 2", refix(bad), "Flags")
    bad = bytearray(fwd)
    bad[126] = 1
    tile_case("header zero byte", refix(bad), "Padding")
    # Directory and section rules, built afresh so each breaks one rule.
    rows_s, ends_s = ints_section(1, [0, 2, 5]), ints_section(2, [2, 3, 6])
    nb = [person(i, 0xABC001) for i in range(6)]
    entry_rows = [0, 0, 2, 5, 5, 5]
    base = [rows_s, ends_s, ints_section(3, [3] * 6), ints_section(4, list(range(6))), ints_section(5, [0xABC001] * 6),
            (11, 6, enc_filter([be(r, 4) + nb[i] for i, r in enumerate(entry_rows)], 10))]
    hdr = dict(tenant=TENANT, subject=KNOWS, direction=1, group=GROUP, built=1, horizon=0, entries=6, rows=3)
    good_digest = digest_sum(be(r, 4) + nb[i] + be(ABSENT, 4) for r, i in ((0, 0), (0, 1), (2, 2), (5, 3), (5, 4), (5, 5)))
    tile_case("unknown section id 12", tile(1, 1, base + [(12, 1, enc_for([0]))], digest=good_digest, **hdr), "Section")
    tile_case("prefix section missing", tile(1, 1, base[:2] + base[3:], digest=good_digest, **hdr), "Section")
    tile_case("rows as BYTES in kind 1", tile(1, 1, [(1, 4, enc_bytes([be(0, 4), be(2, 4), be(5, 4)]))] + base[1:],
                                              digest=good_digest, **hdr), "Encoding")
    tile_case("prefix count 5 of 6 entries", tile(1, 1, base[:2] + [ints_section(3, [3] * 5)] + base[3:],
                                                  digest=good_digest, **hdr), "Count")
    tile_case("prefix past 2^16", tile(1, 1, base[:2] + [ints_section(3, [3] * 5 + [1 << 16])] + base[3:],
                                       digest=good_digest, **hdr), "Range")
    tile_case("rows descending", tile(1, 1, [ints_section(1, [5, 2, 0])] + base[1:], digest=good_digest, **hdr), "Order")
    tile_case("digest of other entries", tile(1, 1, base, digest=good_digest ^ 1, **hdr), "Digest")
    tile_case("valid control for the cases above", tile(1, 1, base, digest=good_digest, **hdr), "none")
    sec = []

    def section_case(name, encoding, raw, refusal):
        sec.append({"name": name, "structure": f"section of encoding {encoding}", "hex": raw.hex(), "refusal": refusal})

    section_case("FOR not minimal", 1, u32(3) + bytes([4]) + bytes(3) + u64(0) + pack([0, 2, 5], 4), "NotMinimal")
    section_case("FOR where DELTA is shorter", 1, enc_for(list(range(1_000_000, 1_000_040))), "NotChosen")
    section_case("DELTA where FOR is shorter", 2, enc_delta([0, 2, 5]), "NotChosen")
    section_case("FOR packed bits past the last field", 1,
                 u32(3) + bytes([3]) + bytes(3) + u64(0) + (int.from_bytes(pack([0, 2, 5], 3)[:8], "little")
                                                           | (1 << 20)).to_bytes(8, "little"), "Padding")
    section_case("BITMAP bit past count", 3, u32(3) + u32(0) + u64(0b1001), "Padding")
    section_case("BYTES ends falling", 4, pad8(u32(2) + u32(1) + u32(2) + u32(1) + b"a"), "Order")
    section_case("BYTES total other than the last end", 4, pad8(u32(2) + u32(4) + u32(1) + u32(3) + b"abc" + b"\x00"),
                 "Count")
    good_filter = bytearray(enc_filter([b"alpha", b"beta", b"gamma"], 10))
    good_filter[4] = 6
    section_case("FILTER k other than the formula", 6, bytes(good_filter), "Parameter")
    refusals.extend(sec)
    raw0 = bytearray((vec / "tiles" / "superblock_copy0.block").read_bytes())
    raw0[64] ^= 0x01
    refusals.append({"name": "superblock with a flipped bit", "structure": "superblock in block 0",
                     "hex": bytes(raw0).hex(), "refusal": "Checksum"})
    raw1 = bytearray((vec / "tiles" / "superblock_copy1.block").read_bytes())
    raw1[10] = 2
    refusals.append({"name": "superblock copy field other than its block", "structure": "superblock in block 1",
                     "hex": bytes(raw1).hex(), "refusal": "Copy"})
    write_json("refusals.json", {"specification": "STORAGE.md sections 4 and 19.4", "cases": refusals})

    kinds = {}
    for name, (raw, fence, desc) in more_tiles().items():
        (vec / "tiles" / f"{name}.tile").write_bytes(raw)
        kinds[name] = {"input": desc, "header": header_fields(raw), "fence_hex": fence.hex(),
                       "tile_key_hex": tile_key(raw, fence).hex(), "file": f"tiles/{name}.tile", "length": len(raw),
                       "checksum": f"{int.from_bytes(raw[8:16], 'little'):016x}", "digest": raw[104:120].hex()}
    write_json("kinds.json", {"specification": "STORAGE.md sections 7 to 9: every kind tiles.json leaves out",
                              "settings": "defaults of section 3.1", "tiles": kinds})

    folded_raw, fold = column_fold_vector()
    (vec / "tiles" / "column_chunk_folded.tile").write_bytes(folded_raw)
    fold.update({"output_file": "tiles/column_chunk_folded.tile", "header": header_fields(folded_raw)})
    halving_settings = {**DEFAULTS, "tile_bytes_max": 16_384, "tile_entries_max": 4_096}
    write_json("compaction.json", {"specification": "STORAGE.md sections 11.2 and 16.3",
                                   "halving": halving_vector(halving_settings), "column_fold": fold})

    write_json("engine.json", engine_vector(vec))


def header_fields(raw):
    le = lambda a, b: int.from_bytes(raw[a:b], "little")
    return {"shape": raw[6], "kind": raw[7], "flags": le(22, 24), "tenant": le(24, 32), "graph": raw[32:48].hex(),
            "subject": raw[48:64].hex(), "type": le(64, 66), "direction": raw[66], "order": raw[67],
            "group": le(68, 72), "built": le(72, 80), "previous": le(80, 88), "horizon": le(88, 96),
            "entries": le(96, 100), "rows": le(100, 104), "span": le(120, 124), "tokenizer": raw[124]}


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("usage: generate_vectors.py <output directory>")
    main(sys.argv[1])
