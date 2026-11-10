#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The storage conformance vectors, laid out byte by byte from ../../storage/STORAGE.md alone, independently of any
# implementation: the primitive encodings of section 4, the term, sortable and token encodings, slot pairs and esc of
# sections 1 and 5, tiles of every shape and kind (sections 6 to 10), digests (section 11), the grid's allocation,
# superblock, derivation version and checkpoint identity (sections 3.2, 11.3, 12), tile keys and the manifest (section
# 13), the pure decisions of sections 12, 16 and 17 (the reserve, the cleaner's due rule, cuts, directory lookups, the
# queue, checkpoint positions with flagged rounds, superblock selection), the refusals of section 19.4, and a reference
# engine of sections 12 to 17. Checksums with Python's xxhash 3.6.0 (libxxhash 0.8.2), the generator the protocol's
# golden bodies use; term hashes with hashlib's SHA-256, truncated to 16 bytes as PROTOCOL.md section 10.1 states; Geo
# values canonicalised by an RFC 8785 serialiser written here for strings and integers.
#
#   python3 -I generate_vectors.py <output directory>
#
# writes vectors/*.json, vectors/tiles/*.tile and vectors/tiles/*.block under the output directory, deterministically:
# running it twice gives the same bytes.

import hashlib
import json
import re
import struct
import sys
from fractions import Fraction
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


def slot_pair(prefix, slot):
    """Section 5.4: a slot pair's canonical form, the type prefix as a big-endian u16, then the slot as a big-endian u32."""
    assert 0 <= prefix < (1 << 16) and 0 <= slot <= ABSENT
    return be(prefix, 2) + be(slot, 4)


# Section 11.1: the statement's slot pair of an entry holding none, prefix 0 and slot ABSENT.
NO_STATEMENT = be(0, 2) + be(ABSENT, 4)


def term_of_id(sid):
    """Section 5.1: an IRI's term encoding in the store, tag 0x01 and its system ID."""
    return b"\x01" + sid


def bucket_of_value(tenant, predicate, value):
    """Bucket::of_value (system-identity; STORAGE.md section 9.2, kind 5): the top 8 bits of XXH3-64 of the tenant as 8
    big-endian bytes, then the top 16 bits of XXH3-64 of the predicate's 16-byte system ID followed by `value`, which is
    the comparison form of the value's sortable encoding in a compiled layout (section 5.2) and its term encoding in the
    generic layout (section 5.1): the same bytes kind 5's key holds after the predicate."""
    return ((xxh64(be(tenant, 8)) >> 56) << 16) | (xxh64(predicate + value) >> 48)


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
    """Section 4.4: widths 1 to value_bytes_max, tile_bytes_max / 4 at the defaults (section 3.3)."""
    assert 1 <= width <= DEFAULTS["tile_bytes_max"] // 4
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


def s_text(text, language):
    """Tag 15: esc of the text's UTF-8 bytes, then its BCP 47 language tag lowercased, empty for none."""
    return esc(text.encode()) + (language or "").lower().encode()


def jcs_string(s):
    """RFC 8785 section 3.2.2.2: ECMAScript's JSON string serialisation."""
    short = {'"': '\\"', "\\": "\\\\", "\b": "\\b", "\t": "\\t", "\n": "\\n", "\f": "\\f", "\r": "\\r"}
    return '"' + "".join(short.get(c, f"\\u{ord(c):04x}" if ord(c) < 0x20 else c) for c in s) + '"'


def jcs(v):
    """RFC 8785 (JCS) for the values a GeoJSON sample here holds: objects, arrays, strings, integers, booleans and null.
    Members sort by their names' UTF-16 code units; integers are limited to the exactly representable range, so ECMAScript
    serialises them as their decimal digits. Fractional numbers would need ECMAScript's number formatting and are not
    used."""
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        assert abs(v) < (1 << 53)
        return str(v)
    if isinstance(v, str):
        return jcs_string(v)
    if isinstance(v, list):
        return "[" + ",".join(jcs(x) for x in v) + "]"
    assert isinstance(v, dict)
    return "{" + ",".join(jcs_string(k) + ":" + jcs(v[k]) for k in sorted(v, key=lambda k: k.encode("utf-16-be"))) + "}"


def s_geo(text):
    return jcs(json.loads(text)).encode()


TEMPORAL_TAGS = (17, 18, 19, 20)
VARIABLE_TAGS = (14, 15, 16, 23, 25, 26, 27, 29)


def comparison_form(tag, encoded):
    """Section 5.2: the first 16 bytes, the instant, for tags 17 to 20; the whole encoding otherwise."""
    return encoded[:16] if tag in TEMPORAL_TAGS else encoded


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
    ("Text with a NUL", "15", "a\\u0000b, no language", s_text("a\x00b", None)),
    ("Text a", "15", "a, no language", s_text("a", None)),
    ("Text a@en-GB", "15", "a@en-GB", s_text("a", "en-GB")),
    ("Text ab", "15", "ab, no language", s_text("ab", None)),
    ("Text chat@FR", "15", "chat@FR", s_text("chat", "FR")),
    ("Bytes 00ff", "16", "0x00ff", b"\x00\xff"),
    ("Date 2026-11-10", "17", "2026-11-10", s_temporal(2026, 11, 10, 0, 0, 0, None)),
    ("Time 12:30:00Z", "18", "12:30:00Z", s_temporal(1972, 12, 31, 12, 30, 0, 0)),
    ("DateTime 1970-01-01T00:00:00Z", "19", "1970-01-01T00:00:00Z", s_temporal(1970, 1, 1, 0, 0, 0, 0)),
    ("DateTime 2026-11-10T12:30:00+01:00", "19", "2026-11-10T12:30:00+01:00", s_temporal(2026, 11, 10, 12, 30, 0, 60)),
    ("DateTime 2026-11-10T12:00:00Z", "19", "2026-11-10T12:00:00Z", s_temporal(2026, 11, 10, 12, 0, 0, 0)),
    ("DateTime 2026-11-10T13:00:00+01:00", "19", "2026-11-10T13:00:00+01:00", s_temporal(2026, 11, 10, 13, 0, 0, 60)),
    ("LocalDateTime 2026-11-10T12:30:00", "20", "2026-11-10T12:30:00", s_temporal(2026, 11, 10, 12, 30, 0, None)),
    ("Duration P1M-shaped", "21", "P1MT1S", s_int(1, 8) + s_int(10**18, 16)),
    ("Iri http://ex.org/alice", "23", "http://ex.org/alice", "http://ex.org/alice".encode()),
    ("Iri http://ex.org/été", "23", "http://ex.org/été", "http://ex.org/été".encode()),
    ("Enum member 2", "24", "index 2", s_uint(2, 4)),
    ("List of Int8 [1, -1]", "25 of 2", "[1, -1]", s_list([s_int(1, 1), s_int(-1, 1)])),
    ("Set of Int8 {1, -1}", "26 of 2", "{1, -1}", s_set([s_int(1, 1), s_int(-1, 1)])),
    ("Record (Int8 1, String x)", "27", "{a: 1, b: x}", s_list([s_int(1, 1), b"x"])),
    ("Vector<F32, 2> [1.0, -2.0]", "28 a=1 b=2", "[1.0, -2.0]", struct.pack("<ff", 1.0, -2.0)),
    ("Geo Point (1, 2)", "29", '{ "type": "Point", "coordinates": [1, 2] }',
     s_geo('{ "type": "Point", "coordinates": [1, 2] }')),
    ("Geo LineString", "29", '{"coordinates": [[-180, 0], [3, 45]], "type": "LineString", "bbox": [-180, 0, 3, 45]}',
     s_geo('{"coordinates": [[-180, 0], [3, 45]], "type": "LineString", "bbox": [-180, 0, 3, 45]}')),
    ("Measure 2.5 in unit 3", "30", "2.5, unit number 3", s_float(2.5, 8) + s_uint(3, 4)),
    ("Unit number 7", "31", "unit 7", s_uint(7, 4)),
    ("EntityType prefix 3", "33", "prefix 3", s_uint(3, 2)),
    ("Id128", "36", "0192f3a1b2c3d4e5f60718293a4b5c6d", s_uint(0x0192F3A1B2C3D4E5F60718293A4B5C6D, 16)),
    ("Precision year", "37", "year", bytes([4])),
    ("Precision day", "37", "day", bytes([6])),
    ("Precision nanosecond", "37", "nanosecond", bytes([12])),
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


def range_token(identifier, tag, encoded):
    """Section 5.3: a range, interval or count token, the identifier byte and the value's comparison form."""
    return token(identifier, comparison_form(tag, encoded))


def token_vectors():
    short = range_token(0x06, 5, s_int(42, 8))
    long_body = b"x" * 300
    noon_z, one_plus = s_temporal(2026, 11, 10, 12, 0, 0, 0), s_temporal(2026, 11, 10, 13, 0, 0, 60)
    assert noon_z != one_plus and range_token(0x20, 19, noon_z) == range_token(0x20, 19, one_plus)
    km = 2.5
    canonical = km * 1000.0
    return [
        {"name": "range token of an Int64, tokenizer int", "identifier": 6, "body_hex": s_int(42, 8).hex(),
         "hex": short.hex()},
        {"name": "range token of DateTime 2026-11-10T12:00:00Z", "identifier": 0x20, "value_hex": noon_z.hex(),
         "body_hex": noon_z[:16].hex(), "hex": range_token(0x20, 19, noon_z).hex(),
         "note": "the identifier byte is this vector's input; the token holds the value's comparison form"},
        {"name": "range token of DateTime 2026-11-10T13:00:00+01:00, equal to the one above", "identifier": 0x20,
         "value_hex": one_plus.hex(), "body_hex": one_plus[:16].hex(), "hex": range_token(0x20, 19, one_plus).hex(),
         "note": "the identifier byte is this vector's input; the token holds the value's comparison form"},
        {"name": "token of Measure<Length> 2.5 km, canonical unit m", "identifier": 0x21,
         "value": "2.5 km, 1 km = 1000 m", "body_hex": s_float(canonical, 8).hex(),
         "hex": token(0x21, s_float(canonical, 8)).hex(),
         "note": "the identifier byte is this vector's input; the token holds 2500.0 m as a Float64's sortable encoding"},
        {"name": "token of 301 bytes", "identifier": 7, "body_hex": long_body.hex(), "hex": token(0x07, long_body).hex()},
        {"name": "token of exactly 256 bytes", "identifier": 7, "body_hex": (b"y" * 255).hex(),
         "hex": token(0x07, b"y" * 255).hex()},
    ]


INT64_MIN, INT64_MAX = -(1 << 63), (1 << 63) - 1
# xsd:integer and the integer types derived from it (PROTOCOL.md section 10.4).
XSD_INTEGERS = ("integer", "long", "int", "short", "byte", "nonNegativeInteger", "positiveInteger",
                "nonPositiveInteger", "negativeInteger", "unsignedLong", "unsignedInt", "unsignedShort", "unsignedByte")
LEXICAL_INSTANT = re.compile(r"(\d{4})-(\d\d)-(\d\d)(?:T(\d\d):(\d\d):(\d\d(?:\.\d+)?))?(Z|[+-]\d\d:\d\d)?")


def generic_comparison_form(datatype, lexical):
    """Section 5.3: the comparison form a generic-layout literal's datatype picks for its range token, or None for an
    integer outside Int64's range, which takes no range token. Samples are exactly representable in their type."""
    if datatype in XSD_INTEGERS:
        v = int(lexical)
        return s_int(v, 8) if INT64_MIN <= v <= INT64_MAX else None
    if datatype == "double":
        return s_float(float(lexical), 8)
    if datatype == "float":
        f32 = struct.unpack(">f", struct.pack(">f", float(lexical)))[0]
        assert f32 == float(lexical) or f32 != f32, "a float sample is exact in binary32"
        return s_float(f32, 8)
    if datatype == "decimal":
        scaled = Fraction(lexical) * 10**18
        assert scaled.denominator == 1 and -(1 << 127) <= scaled < (1 << 127), "exact in Int128 at scale 18"
        return s_int(int(scaled), 16)
    if datatype in ("dateTime", "date"):
        m = LEXICAL_INSTANT.fullmatch(lexical)
        assert m and (m.group(4) is None) == (datatype == "date")
        z = m.group(7)
        tz = 0 if z in (None, "Z") else (1 if z[0] == "+" else -1) * (int(z[1:3]) * 60 + int(z[4:6]))
        t = time_on_timeline(int(m.group(1)), int(m.group(2)), int(m.group(3)), int(m.group(4) or 0),
                             int(m.group(5) or 0), Fraction(m.group(6) or 0), tz) * 10**18
        assert t.denominator == 1
        return s_int(int(t), 16)
    assert datatype == "boolean"
    return {"false": b"\x00", "0": b"\x00", "true": b"\x01", "1": b"\x01"}[lexical]


GENERIC_SORTABLE = {**{d: "Int64" for d in XSD_INTEGERS}, "double": "Float64", "float": "Float64",
                    "decimal": "Int128, the value × 10^18", "dateTime": "the UTC instant of tag 19",
                    "date": "the UTC instant of tag 17", "boolean": "Bool"}
# The identifier bytes of the generic layout's range tokenizer and of the count tokenizer: inputs of the vectors, which
# the tokenizers capability's registry owns.
GENERIC_RANGE_ID, COUNT_ID = 0x22, 0x23
GENERIC_TOKENS = [
    ("integer", "-9223372036854775808"), ("long", "-4294967296"), ("byte", "-1"), ("nonNegativeInteger", "0"),
    ("integer", "+007"), ("int", "7"), ("unsignedByte", "200"), ("int", "2147483647"),
    ("positiveInteger", "1099511627776"), ("unsignedLong", "9223372036854775807"),
    ("integer", "9223372036854775808"), ("unsignedLong", "18446744073709551615"),
    ("negativeInteger", "-9223372036854775809"),
    ("double", "-INF"), ("double", "-1.0E300"), ("float", "-3.5"), ("double", "-1.5"), ("double", "-0.0E0"),
    ("float", "0.0"), ("double", "2.5E-3"), ("float", "0.25"), ("double", "1.0E0"), ("float", "1"),
    ("double", "1.0E300"), ("double", "INF"), ("double", "NaN"),
    ("decimal", "-1000000000000.5"), ("decimal", "-12.34"), ("decimal", "-0.0"), ("decimal", "0"),
    ("decimal", "0.000000000000000001"), ("decimal", "12.34"), ("decimal", "12.340"),
    ("decimal", "100000000000000000000"),
    ("dateTime", "0001-01-01T00:00:00Z"), ("dateTime", "1969-12-31T23:59:59.5Z"), ("dateTime", "1970-01-01T00:00:00Z"),
    ("date", "2026-11-10+14:00"), ("date", "2026-11-10"), ("dateTime", "2026-11-10T00:00:00Z"),
    ("dateTime", "2026-11-09T23:00:00-02:00"), ("dateTime", "2026-11-10T12:00:00Z"),
    ("dateTime", "2026-11-10T13:00:00+01:00"), ("dateTime", "9999-12-31T23:59:59Z"),
    ("boolean", "false"), ("boolean", "0"), ("boolean", "true"), ("boolean", "1"),
]
COUNT_TOKENS = [(1, 1), (1, 2), (1, 300), (1, 1 << 40), (2, 1), (2, 7)]


def generic_token_name(datatype, lexical):
    return f'generic range token of "{lexical}"^^xsd:{datatype}'


def count_token_name(direction, count):
    return f"count token, {'a subject' if direction == 1 else 'an object'}'s count {count}"


def generic_token_vectors():
    """Section 5.3: generic-layout range tokens and count tokens, with the orders and equalities their bytes keep."""
    cases, by = [], {}
    for datatype, lexical in GENERIC_TOKENS:
        name = generic_token_name(datatype, lexical)
        body = generic_comparison_form(datatype, lexical)
        hexed = None if body is None else token(GENERIC_RANGE_ID, body)
        by[name] = hexed
        cases.append({"name": name, "identifier": GENERIC_RANGE_ID, "datatype": f"xsd:{datatype}", "lexical": lexical,
                      "sortable": GENERIC_SORTABLE[datatype], "body_hex": None if body is None else body.hex(),
                      "hex": None if hexed is None else hexed.hex(),
                      "note": "the identifier byte is this vector's input" + (
                          "; outside Int64's range, so no range token" if body is None else "")})
    assert generic_comparison_form("dateTime", "2026-11-10T13:00:00+01:00") == \
        s_temporal(2026, 11, 10, 13, 0, 0, 60)[:16]
    assert generic_comparison_form("date", "2026-11-10") == s_temporal(2026, 11, 10, 0, 0, 0, None)[:16]
    for direction, count in COUNT_TOKENS:
        name = count_token_name(direction, count)
        by[name] = token(COUNT_ID, bytes([direction]) + be(count, 8))
        cases.append({"name": name, "identifier": COUNT_ID, "direction": direction, "count": count,
                      "hex": by[name].hex(), "note": "the identifier byte is this vector's input"})
    g = lambda *pairs: [generic_token_name(d, x) for d, x in pairs]
    ascending = [
        g(("integer", "-9223372036854775808"), ("long", "-4294967296"), ("byte", "-1"), ("nonNegativeInteger", "0"),
          ("int", "7"), ("unsignedByte", "200"), ("int", "2147483647"), ("positiveInteger", "1099511627776"),
          ("unsignedLong", "9223372036854775807")),
        g(("double", "-INF"), ("double", "-1.0E300"), ("float", "-3.5"), ("double", "-1.5"), ("double", "-0.0E0"),
          ("double", "2.5E-3"), ("float", "0.25"), ("double", "1.0E0"), ("double", "1.0E300"), ("double", "INF"),
          ("double", "NaN")),
        g(("decimal", "-1000000000000.5"), ("decimal", "-12.34"), ("decimal", "0"), ("decimal", "0.000000000000000001"),
          ("decimal", "12.34"), ("decimal", "100000000000000000000")),
        g(("dateTime", "0001-01-01T00:00:00Z"), ("dateTime", "1969-12-31T23:59:59.5Z"),
          ("dateTime", "1970-01-01T00:00:00Z"), ("date", "2026-11-10+14:00"), ("date", "2026-11-10"),
          ("dateTime", "2026-11-09T23:00:00-02:00"), ("dateTime", "2026-11-10T12:00:00Z"),
          ("dateTime", "9999-12-31T23:59:59Z")),
        g(("boolean", "false"), ("boolean", "true")),
        [count_token_name(d, c) for d, c in COUNT_TOKENS],
    ]
    equal = [
        g(("integer", "+007"), ("int", "7")),
        g(("double", "-0.0E0"), ("float", "0.0")),
        g(("double", "1.0E0"), ("float", "1")),
        g(("decimal", "-0.0"), ("decimal", "0")),
        g(("decimal", "12.34"), ("decimal", "12.340")),
        g(("date", "2026-11-10"), ("dateTime", "2026-11-10T00:00:00Z")),
        g(("dateTime", "2026-11-10T12:00:00Z"), ("dateTime", "2026-11-10T13:00:00+01:00")),
        g(("boolean", "false"), ("boolean", "0")),
        g(("boolean", "true"), ("boolean", "1")),
    ]
    for o in ascending:
        assert all(by[o[i]] is not None and by[o[i]] < by[o[i + 1]] for i in range(len(o) - 1)), o
    for e in equal:
        assert by[e[0]] is not None and all(by[e[0]] == by[x] for x in e), e
    return cases, ascending, equal


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


def join_key(kind, row_fixed, row, neighbour):
    """Sections 5.4 and 13.1: a row key and a neighbour joined, each component not of fixed width written as esc of it.
    Kind 1's neighbour is a system ID and kinds 3 and 4's a slot pair; kinds 2 and 5's are a value's bytes."""
    return (row if row_fixed else esc(row)) + (neighbour if kind in (1, 3, 4) else esc(neighbour))


def encode_block(kind, rows, *, row_fixed, subject, direction, built, horizon, previous=0, inline_tags=(), order=0,
                 class_i=False, tenant=TENANT, graph=GRAPH, group=GROUP, tokenizer=0, settings=DEFAULTS):
    """Section 7: an adjacency block of `rows`, [(row key, [entry])], each entry (neighbour, statement slot pair or None,
    [inline value or None per column], created, deleted), row keys and neighbours in their canonical bytes (section
    5.4). Rows and entries are put in section 7.4's order here. Returns (raw, fence, entries in order)."""
    inline_widths = []
    for c, tag in enumerate(inline_tags):
        widths = {len(e[2][c]) for _, es in rows for e in es if e[2][c] is not None}
        assert len(widths) == 1 and 1 <= next(iter(widths)) <= 18, "an inline column is small (section 5.2)"
        inline_widths.append(widths.pop())

    def entry_key(e):
        if order == 0:
            col = b""
        else:
            c = (order & 0x7F) - 1
            cf = comparison_form(inline_tags[c], e[2][c])
            col = cf if order < 0x80 else bytes(255 - x for x in cf)
        return (col, e[0], e[1] is None, e[1] or b"", e[3])

    rows = sorted(((r, sorted(es, key=entry_key)) for r, es in rows), key=lambda x: x[0])
    assert all(rows[i][0] < rows[i + 1][0] for i in range(len(rows) - 1)), "rows distinct"
    entries = [(r, e) for r, es in rows for e in es]
    assert len({(r, e[0], e[1], e[3]) for r, e in entries}) == len(entries), "section 7.2: no two entries agree"
    versioned = any(e[3] != 0 or e[4] != 0 for _, e in entries)
    ends = [sum(len(es) for _, es in rows[: i + 1]) for i in range(len(rows))]
    nbrs = [e[0] for _, e in entries]
    sections = [ints_section(2, ends)]
    if kind in (1, 2, 5):
        assert all(len(r) == 6 for r, _ in rows)
        sections += [ints_section(1, [int.from_bytes(r[2:6], "big") for r, _ in rows]),
                     ints_section(12, [int.from_bytes(r[0:2], "big") for r, _ in rows])]
    else:
        sections.append((1, 4, enc_bytes([r for r, _ in rows])))
    if kind == 1:
        sections += [ints_section(3, [int.from_bytes(n[0:2], "big") for n in nbrs]),
                     ints_section(4, [int.from_bytes(n[2:10], "big") for n in nbrs]),
                     ints_section(5, [int.from_bytes(n[10:13], "big") for n in nbrs])]
    elif kind in (3, 4):
        assert all(len(n) == 6 for n in nbrs)
        sections += [ints_section(6, [int.from_bytes(n[2:6], "big") for n in nbrs]),
                     ints_section(13, [int.from_bytes(n[0:2], "big") for n in nbrs])]
    else:
        sections.append((7, 4, enc_bytes(nbrs)))
    statements = [e[1] or NO_STATEMENT for _, e in entries]
    if any(e[1] is not None for _, e in entries):
        sections += [ints_section(8, [int.from_bytes(s[2:6], "big") for s in statements]),
                     ints_section(14, [int.from_bytes(s[0:2], "big") for s in statements])]
    if versioned:
        sections.append(ints_section(9, [e[3] for _, e in entries]))
        if not class_i:
            sections.append(ints_section(10, [e[4] for _, e in entries]))
    if settings["filter_bits_per_key"] > 0:
        sections.append((11, 6, enc_filter([r + e[0] for r, e in entries], settings["filter_bits_per_key"])))
    for c, w in enumerate(inline_widths):
        values = [e[2][c] for _, e in entries]
        sections += [(16 + 2 * c, 5, enc_fixed([v if v is not None else bytes(w) for v in values], w)),
                     (17 + 2 * c, 3, enc_bitmap([1 if v is not None else 0 for v in values]))]
    digest = digest_sum(r + e[0] + s + b"".join(b"\x00" if v is None else b"\x01" + v for v in e[2])
                        for (r, e), s in zip(entries, statements))
    raw = tile(1, kind, sections, flags=(1 if versioned else 0) | (2 if class_i else 0), tenant=tenant, graph=graph,
               subject=subject, direction=direction, order=order, group=group, built=built, previous=previous,
               horizon=horizon, entries=len(entries), rows=len(rows), digest=digest, tokenizer=tokenizer)
    return raw, join_key(kind, row_fixed, rows[0][0], rows[0][1][0][0]), entries


# Positions and horizons of the fixtures, horizons never falling as positions rise (HorizonMonotone).
FIXTURE = {"forward": (1200, 900, bound(38)), "reverse": (1300, 0, bound(39)), "column": (1400, 1100, bound(41)),
           "dictionary": (1500, 0, bound(42)), "statistics": (1600, 0, bound(43))}
CHECKPOINT = (1700, bound(43), version(44, 10))


ORG = 4                     # a second entity type, so a family holds two types' slots (section 5.4)
STATEMENT = 2               # the built-in Statement type's prefix (SCHEMA.md section 7.1)


def describe_entries(entries, inline=True):
    return [{"row_hex": r.hex(), "neighbour_hex": e[0].hex(), "statement_hex": e[1].hex() if e[1] else None,
             **({"inline_hex": [v.hex() if v is not None else None for v in e[2]]} if inline else {}),
             "created": e[3], "deleted": e[4]} for r, e in entries]


def adjacency_forward():
    """Kind 1: subject slot pairs to object IDs, one inline UInt32 'since', one statement, not versioned, ordered by
    neighbour; rows of two types, so (type prefix, slot) and the slot alone order them differently."""
    built, previous, horizon = FIXTURE["forward"]
    since = lambda y: [s_uint(y, 4)]
    rows = [
        (slot_pair(PERSON, 0), [(person(18, 0xAB_C002), None, since(2021), 0, 0),
                                (person(17, 0xAB_C001), None, since(2019), 0, 0)]),
        (slot_pair(PERSON, 2), [(person(17, 0xAB_C001), None, since(2020), 0, 0)]),
        (slot_pair(PERSON, 5), [(person(300, 0xAB_C003), None, since(2017), 0, 0),
                                (person(20, 0xAB_C004), slot_pair(STATEMENT, 9), since(2024), 0, 0),
                                (person(21, 0xAB_C001), None, since(2018), 0, 0)]),
        (slot_pair(ORG, 1), [(person(17, 0xAB_C001), None, since(2015), 0, 0)]),
    ]
    raw, fence, entries = encode_block(1, rows, row_fixed=True, subject=KNOWS, direction=1, built=built,
                                       previous=previous, horizon=horizon, inline_tags=(9,))
    return raw, fence, {"inline_columns": [{"index": 0, "type": "UInt32", "tag": 9, "width": 4}], "order": 0,
                        "entries": describe_entries(entries)}


def adjacency_reverse_versioned():
    """Kind 3 of the generic layout: rows the objects' term encodings (tag 0x01 and the system ID), neighbours subject
    slot pairs of two types, versioned class M, one entry deleted."""
    built, previous, horizon = FIXTURE["reverse"]
    rows = [
        (term_of_id(person(17, 0xAB_C001)), [(slot_pair(ORG, 0), None, [], version(42, 4), 0),
                                             (slot_pair(PERSON, 2), None, [], version(42, 4), 0),
                                             (slot_pair(PERSON, 0), None, [], version(40, 2), 0)]),
        (term_of_id(person(18, 0xAB_C002)), [(slot_pair(PERSON, 0), None, [], version(41, 6), version(42, 8))]),
    ]
    raw, fence, entries = encode_block(3, rows, row_fixed=False, subject=KNOWS, direction=2, built=built,
                                       previous=previous, horizon=horizon)
    return raw, fence, {"layout": "generic", "entries": describe_entries(entries, inline=False)}


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
    """Record kind 1, class M (section 9.2), not versioned: three IRIs to their IDs, all of one bucket group."""
    built, previous, horizon = FIXTURE["dictionary"]
    terms = [("http://ex.org/alice", person(17, 0xAB_C001)), ("http://ex.org/bob", person(18, 0xAB_C002)),
             ("http://ex.org/carol", person(20, 0xAB_C004))]
    recs = sorted((term_key(1, t), i) for t, i in terms)
    sections = [(1, 4, enc_bytes([k for k, _ in recs])), (2, 4, enc_bytes([v for _, v in recs])),
                (5, 6, enc_filter([k for k, _ in recs], DEFAULTS["filter_bits_per_key"]))]
    raw = tile(3, 1, sections, flags=0, tenant=TENANT, group=GROUP, built=built, previous=previous, horizon=horizon,
               entries=len(recs), digest=digest_sum(be(len(k), 4) + k + v for k, v in recs))
    return raw, esc(recs[0][0]), {"records": [{"term_kind": 1, "text": t, "id": i.hex()} for t, i in terms]}


def statistics_registers(values):
    regs = [0] * 64
    for x in values:
        h = xxh64(x)
        y = (h >> 6) | (1 << 58)
        rank = (y & -y).bit_length()
        regs[h % 64] = max(regs[h % 64], rank)
    return regs


def statistics_bytes(statements, kinds, regs_s, regs_o):
    """Section 9.4's format; no kind's count is zero."""
    assert all(kinds.values())
    out = b"\x01" + statements.to_bytes(8, "big", signed=True) + be(len(kinds), 2)
    for name in sorted(kinds, key=lambda s: s.encode()):
        out += be(len(name.encode()), 2) + name.encode() + kinds[name].to_bytes(8, "big", signed=True)
    return out + bytes(regs_s) + bytes(regs_o)


def statistics_value(statements, kinds, subjects, objects):
    return statistics_bytes(statements, kinds, statistics_registers(subjects), statistics_registers(objects))


def statistics_merge(parts):
    """Section 9.4: add the statement counts and each kind's count, drop kinds at zero, take each register's maximum.
    `parts` are (statements, kinds, subject registers, object registers)."""
    statements, kinds, regs_s, regs_o = 0, {}, [0] * 64, [0] * 64
    for st, ks, rs, ro in parts:
        statements += st
        for k, v in ks.items():
            kinds[k] = kinds.get(k, 0) + v
        regs_s = [max(a, b) for a, b in zip(regs_s, rs)]
        regs_o = [max(a, b) for a, b in zip(regs_o, ro)]
    return statistics_bytes(statements, {k: v for k, v in kinds.items() if v != 0}, regs_s, regs_o)


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


def identity(fmt, deriv, position, horizon, committed, manifest_checksum, free_checksum):
    data = u16(fmt) + b"".join(u64(x) for x in (deriv, position, horizon, committed, manifest_checksum, free_checksum))
    assert len(data) == 50
    return xxh128(data)


def superblock(copy, *, parent, sequence, cluster, partition, replica, deriv, position, horizon, committed,
               extent_bytes, grid_extents, manifest_ref, free_ref, mapping, log_bytes=0):
    """Section 12.3; `committed` is the field at offset 80 (T48, C48)."""
    ident = identity(1, deriv, position, horizon, committed, int.from_bytes(manifest_ref[16:24], "little"),
                     int.from_bytes(free_ref[16:24], "little"))
    body = (b"UGDBGRID" + u16(1) + bytes(6) + u64(0) + u64(parent) + u64(sequence) + u64(cluster) + u32(partition)
            + u32(replica) + u64(deriv) + u64(position) + u64(horizon) + u64(committed) + u32(extent_bytes)
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


def checkpoint_positions(frame_lengths, flags, checkpoint_bytes):
    """Section 17.1: the first boundary p with T(p) >= C, after a checkpoint at q the first with
    T(p) >= (T(q) div C + 1) x C, and every round whose `flags` bit 0 (checkpoint) is set."""
    out, total, due = [], 0, checkpoint_bytes
    for position, (length, flag) in enumerate(zip(frame_lengths, flags), start=1):
        total += length
        if total >= due or flag & 1:
            out.append(position)
            due = (total // checkpoint_bytes + 1) * checkpoint_bytes
    return out


def blocks(n):
    return (n + BLOCK - 1) // BLOCK


def reserve_of(settings, tiles, manifest_blocks, free_set_blocks):
    """Section 12.4: the reserve, from the tiles holding delta changes, each (length, [e(c) of its changes]) with a
    family's new tile at length 0, the latest checkpoint's manifest leaves' and root's blocks, and the free set's."""
    w = sum(2 * blocks(length + sum(es)) for length, es in tiles) + 2 * manifest_blocks + free_set_blocks
    per_extent = settings["extent_bytes"] // BLOCK - blocks(settings["tile_bytes_max"]) + 1
    return (w + per_extent - 1) // per_extent + 3


def e_of(row, neighbour, value=b""):
    """Section 12.4: e(c), 256 plus the bytes of the change's row key or key, its neighbour and its value."""
    return 256 + len(row) + len(neighbour) + len(value)


def cleaner_due(settings, free, reserve):
    """Section 12.4: due while free extents are below cleaner_free_percent of the grid or below twice the reserve."""
    return free * 100 < settings["cleaner_free_percent"] * settings["grid_extents"] or free < 2 * reserve


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
    wide = [struct.pack("<75f", *[float(i - j) for i in range(75)]) for j in range(2)]
    cases.append({"name": "fixed width 300, two Vector<F32, 75>", "kind": "fixed", "width": 300,
                  "values_hex": [v.hex() for v in wide], "hex": enc_fixed(wide, 300).hex()})
    widest = [bytes([0xA5]) * (DEFAULTS["tile_bytes_max"] // 4)]
    cases.append({"name": "fixed width value_bytes_max, 16384 at the defaults", "kind": "fixed",
                  "width": DEFAULTS["tile_bytes_max"] // 4, "values_hex": [v.hex() for v in widest],
                  "hex": enc_fixed(widest, DEFAULTS["tile_bytes_max"] // 4).hex()})
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
    tag_of = {n: int(t.split()[0]) for n, t, _, _ in SORTABLE}
    by = {n: comparison_form(tag_of[n], b) for n, _, _, b in SORTABLE}
    orders = [
        ["Int8 -128", "Int8 -1", "Int8 0", "Int8 127"],
        ["Float32 -0.25", "Float32 0.25", "Float32 NaN"],
        ["Float64 -inf", "Float64 -1.5", "Float64 0.0", "Float64 1.5", "Float64 inf", "Float64 NaN"],
        ["Decimal(10,2) -12.34", "Decimal(10,2) 12.34"],
        ["String abc", "String abd"],
        ["Text a", "Text a@en-GB", "Text with a NUL", "Text ab"],
        ["DateTime 1970-01-01T00:00:00Z", "DateTime 2026-11-10T12:30:00+01:00", "DateTime 2026-11-10T12:00:00Z"],
        ["Precision year", "Precision day", "Precision nanosecond"],
    ]
    for o in orders:
        assert all(by[o[i]] < by[o[i + 1]] for i in range(len(o) - 1)), o
    equal = [["Float64 -0.0", "Float64 0.0"], ["DateTime 2026-11-10T12:00:00Z", "DateTime 2026-11-10T13:00:00+01:00"]]
    for e in equal:
        assert all(by[e[0]] == by[x] for x in e), e
    stored = {n: b for n, _, _, b in SORTABLE}
    assert stored["DateTime 2026-11-10T12:00:00Z"] != stored["DateTime 2026-11-10T13:00:00+01:00"]
    assert stored["Geo Point (1, 2)"] == b'{"coordinates":[1,2],"type":"Point"}'

    def small(n, b):
        return tag_of[n] not in VARIABLE_TAGS and len(b) <= 18

    generic, token_ascending, token_equal = generic_token_vectors()
    return {"specification": "STORAGE.md sections 1 and 5", "sortable": [
        {"name": n, "tag": t, "value": v, "hex": b.hex(), "comparison_hex": comparison_form(tag_of[n], b).hex(),
         "small": small(n, b)} for n, t, v, b in SORTABLE],
        "ascending": orders, "equal": equal,
        "terms": term_vectors(), "tokens": token_vectors() + generic, "token_ascending": token_ascending,
        "token_equal": token_equal, "esc": esc_vectors()}


def decision_vectors():
    cuts = []
    for name, rows, emax in (("small rows fill outputs", [100, 200, 300, 4000, 50], 4096),
                             ("a hub row cut into full pieces, its tail open", [10, 9000, 20, 4096], 4096),
                             ("a row of exactly the bound", [4096, 1], 4096),
                             ("no rows", [], 4096)):
        cuts.append({"name": name, "row_entries": rows, "tile_entries_max": emax,
                     "outputs": [[{"row": r, "first": f, "entries": n} for r, f, n in o] for o in cut(rows, emax)]})
    sp = slot_pair
    fences = [sp(PERSON, 0) + person(1, 0xABC001), sp(PERSON, 4) + person(1, 0xABC001),
              sp(PERSON, 4) + person(900, 0xABC001), sp(ORG, 0) + person(1, 0xABC001)]
    lookups = [{"key_hex": k.hex(), "tile": lookup(fences, k)} for k in (
        sp(PERSON, 0) + person(0, 0), sp(PERSON, 3) + person(5, 0xABC001), sp(PERSON, 4) + person(1, 0xABC001),
        sp(PERSON, 4) + person(899, 0xFFFFFF), sp(PERSON, 9) + bytes(16), sp(ORG, 0) + bytes(16),
        sp(ORG, 7) + bytes(16))]
    q = [{"name": "a", "reason": 2, "d": 50, "key": "01"}, {"name": "b", "reason": 1, "d": 3, "key": "02"},
         {"name": "c", "reason": 2, "d": 80, "key": "03"}, {"name": "d", "reason": 3, "d": 900, "key": "00"},
         {"name": "e", "reason": 2, "d": 80, "key": "02"}, {"name": "f", "reason": 4, "d": 1, "key": "09"}]
    frames = [4096] * 10 + [12288, 4096, 40960, 4096]
    checkpoint_cases = []
    for name, flags in (("no round flagged", [0] * len(frames)),
                        ("rounds 2 and 12 flagged checkpoint", [0, 1] + [0] * 9 + [1, 0, 0]),
                        ("round 4 flagged, already a checkpoint by its bytes", [0, 0, 0, 1] + [0] * 10)):
        checkpoint_cases.append({"name": name, "frame_lengths": frames, "flags": flags, "checkpoint_bytes": 16_384,
                                 "positions": checkpoint_positions(frames, flags, 16_384)})
    engine_like = {"extent_bytes": 131_072, "grid_extents": 64, "tile_bytes_max": 16_384, "cleaner_free_percent": 15}
    reserve_cases = []
    for name, settings, tiles, manifest_blocks, free_set_blocks in (
            ("no delta change and no checkpoint yet", engine_like, [], 0, 1),
            ("a family's new tile and an existing tile", engine_like,
             [(0, [[6, 16, 0, 100]]), (9_000, [[17, 6, 0, 3]])], 3, 1),
            ("forty full tiles with ten changes each", engine_like, [(16_384, [[6, 16, 4, 10]])] * 40, 5, 1),
            ("the defaults, a hub tile and a value change", DEFAULTS,
             [(65_536, [[6, 16, 0, 4096]]), (300, [[6, 0, 16_384, 1]])], 40, 1)):
        flat = [(length, [e_of(bytes(r), bytes(n), bytes(v)) for r, n, v, c in chs for _ in range(c)])
                for length, chs in tiles]
        reserve_cases.append({"name": name,
                              "settings": {k: settings[k] for k in ("extent_bytes", "tile_bytes_max")},
                              "tiles": [{"length": length, "changes": [{"row_bytes": r, "neighbour_bytes": n,
                                                                        "value_bytes": v, "count": c}
                                                                       for r, n, v, c in chs]}
                                        for length, chs in tiles],
                              "manifest_blocks": manifest_blocks, "free_set_blocks": free_set_blocks,
                              "reserve": reserve_of(settings, flat, manifest_blocks, free_set_blocks)})
    cleaner_cases = [{"grid_extents": 64, "cleaner_free_percent": 15, "free": f, "reserve": r,
                      "due": cleaner_due(engine_like, f, r),
                      "moves": cleaner_due(engine_like, f, r) and f >= r + 2}
                     for f, r in ((10, 4), (9, 4), (12, 7), (20, 11), (5, 4), (30, 4))]
    sb = {"cluster": 1, "partition": 1, "replica": 1, "extent_bytes": 131_072, "grid_extents": 64, "mapping": 1}
    ref = bytes(24)
    old, _, _ = superblock(0, parent=0, sequence=4, deriv=1, position=10, horizon=0, committed=0, manifest_ref=ref,
                           free_ref=ref, **sb)
    new, cks, _ = superblock(0, parent=1, sequence=5, deriv=1, position=20, horizon=0, committed=0, manifest_ref=ref,
                             free_ref=ref, **sb)
    other, _, _ = superblock(0, parent=1, sequence=5, deriv=1, position=21, horizon=0, committed=0, manifest_ref=ref,
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
    return {"specification": "STORAGE.md sections 12.4, 13.2, 16.2, 16.3, 16.6, 17.1 and 17.2",
            "cuts": cuts,
            "lookups": {"fences_hex": [f.hex() for f in fences], "cases": lookups},
            "queue": {"tiles": q, "order": queue_order(q)},
            "streams": [{"built": b, "previous": p, "cold_after_rounds": 65_536,
                         "stream": stream_of(b, p, 65_536)} for b, p in ((1200, 0), (70_000, 4_464), (70_000, 4_465),
                                                                           (70_000, 69_999))],
            "checkpoints": checkpoint_cases,
            "reserve": reserve_cases,
            "cleaner": cleaner_cases,
            "superblock_selection": selections}


# ---------------------------------------------------------------- tile kinds beyond the fixtures (sections 7 to 9)


def record_run(kind, recs, *, class_i, tenant=TENANT, graph=GRAPH, group=ABSENT, type_=0, built=1800,
               horizon=bound(44), versioned_recs=None):
    """A record run of `recs` (key, value) pairs, or of `versioned_recs` (key, value, created, deleted) when given."""
    if versioned_recs is not None:
        rows = sorted(versioned_recs, key=lambda r: (r[0], r[2]))
        assert all((rows[i][0], rows[i][2]) < (rows[i + 1][0], rows[i + 1][2]) for i in range(len(rows) - 1))
        assert all(r[2] == 0 or r[2] > horizon for r in rows), "section 14.5: created at or below H is 0"
        assert all(r[3] == 0 or r[3] > horizon for r in rows), "section 14.5: deleted at or below H is removed"
        keys, values = [r[0] for r in rows], [r[1] for r in rows]
        sections = [(1, 4, enc_bytes(keys)), (2, 4, enc_bytes(values)), ints_section(3, [r[2] for r in rows])]
        if not class_i:
            sections.append(ints_section(4, [r[3] for r in rows]))
        flags = 1 | (2 if class_i else 0)
    else:
        rows = sorted(recs)
        assert all(rows[i][0] < rows[i + 1][0] for i in range(len(rows) - 1)), "a key once"
        keys, values = [r[0] for r in rows], [r[1] for r in rows]
        sections = [(1, 4, enc_bytes(keys)), (2, 4, enc_bytes(values))]
        flags = 2 if class_i else 0
    sections.append((5, 6, enc_filter(keys, DEFAULTS["filter_bits_per_key"])))
    raw = tile(3, kind, sections, flags=flags, tenant=tenant, graph=graph, type_=type_, group=group, built=built,
               horizon=horizon, entries=len(rows), digest=digest_sum(be(len(k), 4) + k + v for k, v in zip(keys, values)))
    if versioned_recs is not None:
        desc = [{"key_hex": r[0].hex(), "value_hex": r[1].hex(), "created": r[2], "deleted": r[3]} for r in rows]
    else:
        desc = [{"key_hex": k.hex(), "value_hex": v.hex()} for k, v in zip(keys, values)]
    return raw, {"records": desc}


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


G2 = system_id(1, 2001, 0xAB_0110)     # a named graph's system ID
EMAIL = system_id(1, 1008, 0xAB_0111)  # a generic-layout predicate holding @unique values
BORN_AT = system_id(1, 1009, 0xAB_0112)  # a compiled @unique DateTime attribute's predicate


def referrer_members(members):
    """Kind 7's value: each member (group, epoch added, generation), in group order, as 3 + 8 + 8 big-endian bytes."""
    return b"".join(be(g, 3) + be(e, 8) + be(n, 8) for g, e, n in sorted(members))


def referrer_fold(records):
    """Kind 7's fold: the union by group, each group's greatest generation and earliest epoch."""
    out = {}
    for members in records:
        for g, e, n in members:
            pe, pn = out.get(g, (e, n))
            out[g] = (min(pe, e), max(pn, n))
    return referrer_members([(g, e, n) for g, (e, n) in out.items()])


def limit_value(count, total):
    """Kind 19's value for a `sum` over an integer attribute: the count's change as an i64, the sum's as an i128."""
    return count.to_bytes(8, "little", signed=True) + total.to_bytes(16, "little", signed=True)


def session_value(request, commit_version, position, share, command, header, body):
    """Kind 10's value, section 9.3's session table."""
    assert len(header) == 128
    return (u64(request) + u64(commit_version) + u64(position) + u32(share) + u16(command) + bytes(2) + header
            + u32(len(body)) + bytes(4) + body)


def more_tiles():
    """Every kind sections 7 to 9 define that tiles.json leaves out, record kinds 16 and 17 aside (tiles.json's manifest
    leaf and root), with the additive kinds' folds."""
    alice, bob, carol = person(17, 0xAB_C001), person(18, 0xAB_C002), person(20, 0xAB_C004)
    out = {}
    raw, fence, entries = encode_block(
        2, [(slot_pair(PERSON, 0), [(b"\x20\x03Ada", None, [], 0, 0),
                                    (b"\x11" + (42).to_bytes(8, "big", signed=True), None, [], 0, 0)]),
            (slot_pair(ORG, 0), [(b"\x20\x00", None, [], 0, 0)])],
        row_fixed=True, subject=system_id(1, 1006, 0xAB_0107), direction=1, built=1800, horizon=bound(44))
    out["adjacency_forward_values"] = (raw, fence, {"layout": "generic", "entries": describe_entries(entries, False)})
    raw, fence, entries = encode_block(
        4, [(range_token(0x06, 5, s_int(31, 8)), [(slot_pair(PERSON, 4), None, [], 0, 0),
                                                  (slot_pair(PERSON, 0), None, [], 0, 0)]),
            (range_token(0x06, 5, s_int(45, 8)), [(slot_pair(PERSON, 1), None, [], 0, 0)])],
        row_fixed=False, subject=AGE, direction=0, built=1800, horizon=bound(44), tokenizer=6)
    out["adjacency_index"] = (raw, fence, {"tokenizer": "int, 0x06, range tokens of Int64 'age'",
                                           "entries": describe_entries(entries, False)})
    raw, fence, entries = encode_block(
        5, [(slot_pair(PERSON, 0), [(be(version(40, 2), 8) + b"draft", None, [], 0, 0),
                                    (be(version(42, 6), 8) + b"final", None, [], 0, 0)])],
        row_fixed=True, subject=system_id(1, 1007, 0xAB_0108), direction=1, built=1800, horizon=bound(44),
        class_i=True)
    out["adjacency_version_run"] = (raw, fence, {"class": "I", "entries": describe_entries(entries, False)})
    # A compiled reverse family ordered newest first by the local subject's denormalised DateTime 'born' (sections 7.4,
    # 13.4): equal instants of different offsets tie, and the neighbour breaks the tie.
    born = {(PERSON, 0): s_temporal(1990, 5, 1, 12, 0, 0, 0), (PERSON, 2): s_temporal(1990, 5, 1, 13, 0, 0, 60),
            (PERSON, 5): s_temporal(2001, 1, 1, 0, 0, 0, 0), (ORG, 1): s_temporal(1985, 3, 2, 9, 30, 0, -300)}
    raw, fence, entries = encode_block(
        3, [(bob, [(slot_pair(*k), None, [v], 0, 0) for k, v in born.items()]),
            (alice, [(slot_pair(PERSON, 2), slot_pair(STATEMENT, 4), [born[(PERSON, 2)]], 0, 0)])],
        row_fixed=True, subject=KNOWS, direction=2, built=1800, horizon=bound(44), inline_tags=(19,), order=0x81)
    assert [e[0] for r, e in entries if r == bob] == [slot_pair(PERSON, 5), slot_pair(PERSON, 0), slot_pair(PERSON, 2),
                                                      slot_pair(ORG, 1)]
    out["adjacency_reverse_ordered"] = (raw, fence, {
        "layout": "compiled", "inline_columns": [{"index": 0, "type": "DateTime", "tag": 19, "width": 18}],
        "order": 0x81, "entries": describe_entries(entries)})
    out["column_variable"] = column_variable()
    out["column_identity"] = column_identity()
    long = "a string whose lexical form is longer than fifteen bytes"
    dave_old, dave_new = person(30, 0xAB_C005), person(31, 0xAB_C005)
    email = b"\x20\x08a@ex.org"
    unique_group = bucket_of_value(TENANT, EMAIL, email) >> 6
    born_cf = comparison_form(19, s_temporal(2026, 11, 10, 12, 0, 0, 0))
    assert born_cf == comparison_form(19, s_temporal(2026, 11, 10, 13, 0, 0, 60))
    born_group = bucket_of_value(TENANT, BORN_AT, born_cf) >> 6
    commit_header = (bytes(range(16)) + u64(5) + u64(version(44, 10)) + u32(1) + bytes(4) + u64(1600) + u64(44)
                     + bytes(128 - 56))
    commit_body = u32(24) + u32(0) + u32(24) + u32(0) + u32(24) + u32(0)
    ref_v1 = [(GROUP, 40, 1)]
    ref_v2 = [(GROUP, 41, 3), (GROUP + 1, 41, 1)]
    stats_v1 = (3, {"iri": 3}, statistics_registers([person(1, 0xAB_C001), person(1, 0xAB_C001),
                                                     person(2, 0xAB_C002)]),
                statistics_registers([alice, bob, alice]))
    stats_v2 = (-1, {"iri": -1}, [0] * 64, [0] * 64)
    limits = [(1, 31), (1, 45), (-1, -31)]
    limit_key = be(PERSON, 2) + be(0, 2) + alice + s_int(2026, 8)
    runs = {
        "record_dictionary_recreated": (record_run(1, [], class_i=False, group=GROUP, versioned_recs=[
            (term_key(1, "http://ex.org/dave"), dave_old, 0, version(45, 4)),
            (term_key(1, "http://ex.org/dave"), dave_new, version(45, 4), 0)]),
            "kind 1, class M: dave deleted and re-created at version(45, 4); a read below it resolves the old ID"),
        "record_id_to_term": (record_run(2, [(alice, bytes(16) + name_bytes(1, "http://ex.org/alice")),
                                             (dave_new, dave_old + name_bytes(1, "http://ex.org/dave"))],
                                         class_i=True, group=GROUP),
                              "kind 2, class I: alice replaced none; dave's new ID names the ID it replaced"),
        "record_hash_to_text": (record_run(3, [(sha16(long), long.encode())], class_i=True), "kind 3"),
        "record_key_claim": (record_run(4, [(be(PERSON, 2) + G2 + be(5, 4) + b"alice", alice)], class_i=False,
                                        graph=G2, group=group_of(alice)),
                             "kind 4: key space prefix 3, graph G2, the canonical key of the String key 'alice'"),
        "record_unique_claim": (record_run(5, [(EMAIL + email, alice)], class_i=False, graph=G2, group=unique_group),
                                f"kind 5, generic layout: the value's term encoding; group {unique_group} is the "
                                f"bucket group of Bucket::of_value(7, EMAIL, term), bucket "
                                f"{bucket_of_value(TENANT, EMAIL, email):06x}"),
        "record_unique_claim_compiled": (record_run(5, [(BORN_AT + born_cf, alice)], class_i=False, graph=G2,
                                                    group=born_group),
                                         f"kind 5, compiled layout: a @unique DateTime 2026-11-10T12:00:00Z, the key "
                                         f"and Bucket::of_value both over its comparison form, so "
                                         f"2026-11-10T13:00:00+01:00 is the same claim; group {born_group}, bucket "
                                         f"{bucket_of_value(TENANT, BORN_AT, born_cf):06x}"),
        "record_mark": (record_run(6, [(alice + be(GROUP, 3), be(2, 8))], class_i=False, group=GROUP), "kind 6"),
        "record_mark_versioned": (record_run(6, [], class_i=False, group=GROUP, versioned_recs=[
            (alice + be(GROUP, 3), be(1, 8), 0, version(45, 2)),
            (alice + be(GROUP, 3), be(2, 8), version(45, 2), 0)]), "kind 6, versioned"),
        "record_referrer_set": (record_run(7, [], class_i=False, group=GROUP, versioned_recs=[
            (alice, referrer_members(ref_v1), version(45, 2), 0),
            (alice, referrer_members(ref_v2), version(46, 4), 0)]),
            "kind 7, additive: two records of one key; fold_hex is their fold"),
        "record_tombstone": (record_run(8, [(bob, u64(version(43, 2)))], class_i=False, group=GROUP), "kind 8"),
        "record_statistics_additive": (record_run(9, [], class_i=False, group=ABSENT, versioned_recs=[
            (KNOWS, statistics_bytes(*stats_v1), version(45, 10), 0),
            (KNOWS, statistics_bytes(*stats_v2), version(46, 2), 0)]),
            "kind 9, additive: a round of three inserts, then a round of one delete; fold_hex is their merge"),
        "record_session": (record_run(10, [(bytes(range(16)), session_value(5, version(44, 10), 1600, 0, 5,
                                                                             commit_header, commit_body))],
                                      class_i=False, tenant=0, group=ABSENT),
                           "kind 10: a Commit reply (command 5) with no results, token partition 1 position 1600"),
        "record_slot_map": (record_run(11, [(be(17, 8), u32(0)), (be(18, 8), u32(1)), (be(20, 8), u32(2))],
                                       class_i=True, group=GROUP, type_=PERSON), "kind 11, type 3"),
        "record_set_combination": (record_run(12, [(be(9, 2) + be(1, 2) + alice + be(2, 2) + min(bob, carol)
                                                    + max(bob, carol) + be(0, 2), system_id(9, 3, 0xAB_C001))],
                                              class_i=False, group=group_of(system_id(9, 3, 0xAB_C001))),
                                   "kind 12: root prefix 9; roles in order: one, two ascending, an empty optional"),
        "record_idempotency_claim": (record_run(13, [], class_i=False, group=GROUP, versioned_recs=[
            (be(PERSON, 2) + be(0x0192F3A1B2C3D4E5F60718293A4B5C6D, 16), alice, 0, version(45, 2)),
            (be(PERSON, 2) + be(0x0192F3A1B2C3D4E5F60718293A4B5C6D, 16), bob, version(45, 4), 0)]),
            "kind 13 of a deletable type, class M: alice's delete released the value, which bob then claimed"),
        "record_idempotency_claim_append_only": (record_run(13, [(be(5, 2) + be(0x0192F3A1B2C3D4E5F6071829AAAAAAAA, 16),
                                                                  system_id(5, 7, 0xAB_C001))],
                                                            class_i=True, group=GROUP),
                                                 "kind 13 of an @append_only type (prefix 5), class I"),
        "record_drop_marker": (record_run(14, [(be(version(44, 4), 8), u16(2) + bytes(6) + system_id(2, 1, 0xAB_0109))],
                                          class_i=False, group=ABSENT), "kind 14"),
        "record_drop_marker_catalog_version": (record_run(14, [(be(version(46, 0), 8),
                                                                u16(6) + bytes(6) + u64(4) + bytes(8))],
                                                          class_i=False, group=ABSENT),
                                               "kind 14 naming operation 6: catalog version 4, taking effect at "
                                               "version(46, 0), removes a tokenizer of an index and covers that "
                                               "tokenizer's index families; the argument holds the version as a u64 "
                                               "in its first 8 bytes and 8 zero bytes, as PROTOCOL.md section 12.2's "
                                               "operation 6 lays it out"),
        "record_build": (record_run(15, [(be(1650, 8), u64(3) + u64(version(44, 2)) + AGE + slot_pair(PERSON, 5)
                                          + alice)], class_i=False, group=ABSENT),
                         "kind 15: catalog version 3, start version(44, 2), the predicate, then the last key reached, "
                         "a slot pair and a system ID"),
        "record_statement": (record_run(18, [(system_id(9, 4, 0xAB_C001),
                                              GRAPH + alice + KNOWS + b"\x01" + bob
                                              + enc_bytes([AGE + b"\x11" + (2019).to_bytes(8, "big", signed=True)]))],
                                        class_i=False, group=GROUP), "kind 18"),
        "record_limit_aggregate": (record_run(19, [], class_i=False, group=group_of(alice), versioned_recs=[
            (limit_key, limit_value(c, t), version(45, 2 * i), 0) for i, (c, t) in enumerate(limits)]),
            "kind 19, additive: declaring prefix 3, constraint 0, per alice, grouping Int64 2026; fold_hex sums them"),
        "record_purge_marker": (record_run(20, [(carol, u64(version(44, 6)))], class_i=False, group=group_of(carol)),
                                "kind 20: carol purged at version(44, 6)"),
    }
    folds = {"record_referrer_set": referrer_fold([ref_v1, ref_v2]),
             "record_statistics_additive": statistics_merge([stats_v1, stats_v2]),
             "record_limit_aggregate": limit_value(sum(c for c, _ in limits), sum(t for _, t in limits))}
    for name, ((raw, desc), note) in runs.items():
        keys = [bytes.fromhex(r["key_hex"]) for r in desc["records"]]
        desc["note"] = note
        if name in folds:
            desc["fold_hex"] = folds[name].hex()
        out[name] = (raw, esc(min(keys)), desc)
    return out


def halving_vector(settings):
    """Section 16.3 step 4: one row of tile_entries_max entries whose encoding exceeds tile_bytes_max, cut in two at
    entry n div 2 and again until each half fits."""
    emax, bmax = settings["tile_entries_max"], settings["tile_bytes_max"]
    fam = {"kind": 1, "direction": 1, "subject": KNOWS, "row_fixed": True}
    # Neighbours with scattered sequences and buckets, so packing gains little.
    entries = sorted((slot_pair(PERSON, 0), person((i * 2654435761) % (1 << 40) + 1, (i * 40503) % (1 << 24)), 0, 0)
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
# A replica holding one relation, knows, in tenant 7's default graph and one bucket group, in the generic layout: its
# forward family (kind 1, rows subject slot pairs) and its reverse family (kind 3, rows the objects' term encodings),
# class M, no statements and no inline column. The rounds name subject slot pairs directly (slot maps are record runs
# outside this model). Admission (section 15.6) is checked as the primary would, and the script never meets GridFull.
# Everything else follows sections 12 to 17 as written: apply, due rules, the queue, units, rewrites with merge, fold,
# cut, halving and the next-tile merge, placement and streams, the reserve and the cleaner, checkpoints at byte
# thresholds and at flagged rounds with the drain, the manifest, the free set, the superblock and frees taking effect at
# the next boundary, and recovery.

ENGINE_SETTINGS = [
    ("extent_bytes", 131_072), ("grid_extents", 64), ("tile_bytes_max", 16_384), ("tile_entries_max", 256),
    ("chunk_slots", 4_096), ("filter_bits_per_key", 10), ("theta_percent", 50), ("delta_entries_max", 65_536),
    ("delta_watermark_percent", 75), ("delta_row_entries_max", 64), ("delta_age_rounds", 64), ("beat_factor", 4),
    ("cold_after_rounds", 1_024), ("cleaner_free_percent", 15), ("checkpoint_bytes", 16_777_216),
    ("retention_ms", 30_000), ("lane_entries_max", 65_536), ("lane_bytes_max", 262_144),
]
# The generic layout's families of one predicate (section 13.4): rows of the forward family are subject slot pairs, rows
# of the reverse family the objects' term encodings (tag 0x01 and the system ID), never of fixed width (section 5.4).
FAMILIES = {
    "forward": {"kind": 1, "direction": 1, "subject": KNOWS, "row_fixed": True},
    "reverse": {"kind": 3, "direction": 2, "subject": KNOWS, "row_fixed": False},
}
REASON_RANK = {1: 0, 4: 1, 5: 2, 2: 3, 3: 4}


def fam_key(fam):
    return (bytes([1, fam["kind"]]) + be(TENANT, 8) + GRAPH + fam["subject"] + be(0, 2) + bytes([fam["direction"]])
            + bytes([0, 0]) + be(GROUP, 4))


def encode_adjacency(fam, entries, built, previous, horizon, settings):
    """Sections 7 and 14.5: entries (row, neighbour, created, deleted) in section 7.4's order, created already 0 at or
    below H, no statement and no inline column."""
    rows = []
    for r, n, c, d in entries:
        if rows and rows[-1][0] == r:
            rows[-1][1].append((n, None, [], c, d))
        else:
            rows.append((r, [(n, None, [], c, d)]))
    raw, _, ordered = encode_block(fam["kind"], rows, row_fixed=fam["row_fixed"], subject=fam["subject"],
                                   direction=fam["direction"], built=built, previous=previous, horizon=horizon,
                                   settings=settings)
    assert [(r, e[0], e[3], e[4]) for r, e in ordered] == list(entries), "entries in section 7.4's order"
    return raw


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
        f = FAMILIES[self.fam]
        return join_key(f["kind"], f["row_fixed"], self.entries[0][0], self.entries[0][1])

    @property
    def key(self):
        return fam_key(FAMILIES[self.fam]) + self.fence

    @property
    def blocks(self):
        return blocks(len(self.raw))


class Engine:
    def __init__(self, settings, name, check_admission=False):
        self.s = dict(settings)
        self.name = name
        self.check_admission = check_admission
        self.E = self.s["extent_bytes"]
        self.G = self.s["grid_extents"]
        self.grid = bytearray(self.E * self.G)
        # Extent 0 holds the superblock; "closed" with no live block is released (section 12.4).
        self.state = ["closed"] + ["free"] * (self.G - 1)
        self.stream_of_extent = [None] * self.G
        self.cursor = [0] * self.G
        self.live = [0] * self.G
        self.open = {}
        self.pending_free = []                                # freed by the last checkpoint, free at the next boundary
        self.directory = {f: [] for f in FAMILIES}
        self.delta = []                                       # dicts: fam, row, nbr, op, version, position, taken
        self.queue = []                                       # items: (rank, -d, key, target)
        self.in_progress = []
        self.units = 0
        self.position = 0
        self.horizon = 0
        self.committed = 0
        self.log_bytes = 0
        self.next_checkpoint = self.s["checkpoint_bytes"]
        self.sequence = 0
        self.parent = 0
        self.leaves = []                                      # [(records, Tile-like dict)]
        self.root = None
        self.free_set = None
        self.events = []
        self.checkpoints = []
        self.admission = []

    # -- allocation (section 12.4)
    def place(self, raw, stream):
        b = blocks(len(raw))
        e = self.open.get(stream)
        if e is None or self.cursor[e] + b > self.E // BLOCK:
            if e is not None:
                self.state[e] = "closed"
            e = next((x for x in range(1, self.G) if self.state[x] == "free"), None)
            assert e is not None, "PlacementFound"
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
        self.live[e] -= blocks(len(raw))
        assert self.live[e] >= 0

    def free_count(self):
        """Section 12.4: free extents only; a released extent counts once it is freed."""
        return sum(1 for e in range(1, self.G) if self.state[e] == "free")

    def reserve(self, extra=()):
        """Section 12.4's reserve over the delta's changes and `extra`, each change grouped to the tile it belongs to."""
        tiles = {}
        for c in list(self.delta) + list(extra):
            t = self.tile_of(c["fam"], c["row"], c["nbr"])
            k = id(t) if not isinstance(t, tuple) else t
            length = 0 if isinstance(t, tuple) else len(t.raw)
            tiles.setdefault(k, (length, []))[1].append(e_of(c["row"], c["nbr"]))
        manifest = sum(blocks(len(lf["raw"])) for lf in self.leaves) + (blocks(len(self.root["raw"])) if self.root else 0)
        free_set = blocks(len(free_set_tile(self.G, set(), 0, 0, 0)))
        return reserve_of(self.s, list(tiles.values()), manifest, free_set)

    # -- directory (section 13.2)
    def tile_of(self, fam, row, nbr):
        tiles = self.directory[fam]
        if not tiles:
            return ("new", fam)
        f = FAMILIES[fam]
        k = join_key(f["kind"], f["row_fixed"], row, nbr)
        best = tiles[0]
        for t in tiles:
            if t.fence <= k:
                best = t
        return best

    # -- apply (section 15)
    def changes_of(self, rnd):
        out = []
        for commit in rnd["commits"]:
            v = commit["version"]
            for op, items in (("insert", commit["inserts"]), ("delete", commit["deletes"])):
                for prefix, slot, obj_hex in items:
                    obj, subject = bytes.fromhex(obj_hex), slot_pair(prefix, slot)
                    out.append({"fam": "forward", "row": subject, "nbr": obj, "op": op, "version": v,
                                "position": rnd["position"], "taken": False})
                    out.append({"fam": "reverse", "row": term_of_id(obj), "nbr": subject, "op": op, "version": v,
                                "position": rnd["position"], "taken": False})
        return out

    def admit(self, changes):
        """Section 15.6, as the primary builds the round: the delta with the round's changes stays at or below the
        watermark, and free extents stay at or above the reserve with those changes counted (section 12.4)."""
        free, reserve = self.free_count(), self.reserve(changes)
        limit = self.s["delta_entries_max"] * self.s["delta_watermark_percent"] // 100
        self.admission.append({"position": self.position + 1, "delta": len(self.delta) + len(changes), "free": free,
                               "reserve": reserve})
        assert len(self.delta) + len(changes) <= limit, "the script exceeds delta admission"
        assert free >= reserve, "the script exceeds grid admission (GridFull)"

    def apply(self, rnd):
        changes = self.changes_of(rnd)
        if self.check_admission:
            self.admit(changes)
        self.position = rnd["position"]
        assert rnd["horizon"] >= self.horizon, "HorizonMonotone"
        self.horizon = rnd["horizon"]
        for commit in rnd["commits"]:
            self.committed = commit["version"]
        self.delta.extend(changes)
        self.log_bytes += rnd["frame_length"]
        self.boundary(len(changes), rnd["flags"])

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
        # Conservation (section 11.2), entries identified by their digest bytes: no statement, no inline column.
        multiset = lambda xs: {x: xs.count(x) for x in set(xs)}
        mb, ma = multiset(before), multiset(after)
        added = [x for x in ma for _ in range(max(0, ma[x] - mb.get(x, 0)))]
        removed = [x for x in mb for _ in range(max(0, mb[x] - ma.get(x, 0)))]
        hashes = lambda xs: digest_sum(x + NO_STATEMENT for x in xs)
        lhs = sum(int.from_bytes(o.raw[104:112], "little") | (int.from_bytes(o.raw[112:120], "little") << 64)
                  for o in outputs) & MASK128
        rhs = (sum(int.from_bytes(x.raw[104:112], "little") | (int.from_bytes(x.raw[112:120], "little") << 64)
                   for x in tiles) + hashes(added) - hashes(removed)) & MASK128
        assert lhs == rhs, "DigestConserved"
        for o in outputs:
            assert decode_adjacency(o.raw) == list(o.entries), "RoundTrip"
        cost = sum(len(x.entries) for x in tiles) + len(taken) + sum(len(o.entries) for o in outputs)
        return {"target": t, "inputs": tiles if tiles else [t], "tiles": tiles, "taken": taken, "outputs": outputs,
                "remaining": cost, "decided": self.position, "fam": fam}

    def cut(self, kept):
        rows = []
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
    def clean(self):
        free, reserve = self.free_count(), self.reserve()
        if not cleaner_due(self.s, free, reserve) or free < reserve + 2:
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
        assert self.live[e] == 0
        self.events.append({"position": self.position, "cleaner": e, "moved": len(moving), "free": free,
                            "reserve": reserve})

    # -- a round boundary
    def boundary(self, changes, flags):
        # A checkpoint's frees take effect here, before this boundary's placements (sections 12.4 and 17.1, step 6).
        for e in self.pending_free:
            self.state[e] = "free"
        self.pending_free = []
        self.evaluate()
        self.beat(self.s["beat_factor"] * changes)
        at_checkpoint = self.log_bytes >= self.next_checkpoint or flags & 1
        if not at_checkpoint:
            self.clean()  # at a checkpoint boundary the cleaner runs once, after the drain (section 16.6)
        else:
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
        rev = {(n, r[1:]) for r, n in live("reverse")}
        assert fwd == rev, "RelationDigests"

    # -- checkpoints (section 17.1)
    def checkpoint(self):
        # Step 0: the drain, then the cleaner once.
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
        assert len(fs_raw) == len(fs_probe)
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
        copies = []
        for c in range(4):
            raw, checksum, ident = superblock(
                c, parent=self.parent, sequence=self.sequence, cluster=0x0123_4567_89AB_CDEF, partition=1, replica=1,
                deriv=derivation(ENGINE_SETTINGS), position=self.position, horizon=self.horizon,
                committed=self.committed, extent_bytes=self.E, grid_extents=self.G,
                manifest_ref=reference(root_at, root_raw), free_ref=reference(fs_at, fs_raw), mapping=1,
                log_bytes=self.log_bytes)
            self.grid[c * BLOCK:(c + 1) * BLOCK] = raw
            copies.append(raw)
        self.parent = checksum
        # Step 6: free and punch; the frees take effect at the next round's boundary.
        freed = []
        for e in range(1, self.G):
            if self.state[e] == "closed" and self.live[e] == 0:
                self.state[e] = "released"
                self.stream_of_extent[e] = None
                self.grid[e * self.E:(e + 1) * self.E] = bytes(self.E)
                freed.append(e)
        self.pending_free = freed
        self.checkpoints.append({"position": self.position, "log_bytes": self.log_bytes, "sequence": self.sequence,
                                 "superblock_checksum": f"{checksum:016x}", "identity": store128(ident).hex(),
                                 "tiles": sum(len(v) for v in self.directory.values()), "leaves": len(new_leaves),
                                 "extents_allocated": sorted(marked), "extents_freed": freed,
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
        self.sequence, self.position, self.horizon, self.committed = f(32, 40), f(64, 72), f(72, 80), f(80, 88)
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
                assert t.key == tkey
                self.directory[fam].append(t)
        for fam in self.directory:
            self.directory[fam].sort(key=lambda o: o.fence)
        for t in [t for f in self.directory for t in self.directory[f]]:
            self.live[t.address // self.E] += t.blocks
        for x in self.leaves + [self.root, self.free_set]:
            self.live[x["address"] // self.E] += blocks(len(x["raw"]))
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
        rows = [slot_pair(p, x) for p, x in zip(decode_vector(raw, 12), decode_vector(raw, 1))]
        pre, seq, buk = decode_vector(raw, 3), decode_vector(raw, 4), decode_vector(raw, 5)
        nbrs = [system_id(p, s, b) for p, s, b in zip(pre, seq, buk)]
    else:
        rows = decode_bytes(section_of(raw, 1))
        nbrs = [slot_pair(p, x) for p, x in zip(decode_vector(raw, 13), decode_vector(raw, 6))]
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


# An empty round flagged `checkpoint` (PROTOCOL.md section 12.1, `flags` bit 0), and its frame's length: a 256-byte
# frame and command header, then a body of the eight sections' table and no section bytes.
FLAGGED_POSITIONS = (25, 150)
EMPTY_ROUND_BYTES = 256 + 8 * 8


def engine_rounds(count=220, flagged=FLAGGED_POSITIONS):
    """The scripted rounds: subjects are slot pairs of two types (PERSON and ORG), objects 400 persons of the bucket
    group. A position in `flagged` is an empty round flagged `checkpoint`; every other round carries commits."""
    rng = Lcg(2026_11_10)
    objects = [person(seq, 0xAB_C000 + (seq % 64)) for seq in range(1, 401)]
    linked = set()
    rounds = []
    for r in range(count):
        position, epoch = r + 1, 100 + r
        horizon = bound(epoch - 5) if r >= 5 else 0
        if position in flagged:
            rounds.append({"position": position, "horizon": horizon, "flags": 1, "frame_length": EMPTY_ROUND_BYTES,
                           "commits": []})
            continue
        commits = []
        for k in range(1 + rng.next(3)):
            inserts, deletes = [], []
            for _ in range(6 + rng.next(9)):
                prefix = ORG if rng.next(5) == 0 else PERSON
                slot = 0 if rng.next(4) == 0 else 1 + rng.next(39)
                obj = objects[rng.next(len(objects))]
                item = (prefix, slot, obj.hex())
                if (prefix, slot, obj) not in linked and item not in inserts:
                    inserts.append(item)
            for _ in range(rng.next(4)):
                if linked:
                    pick = sorted(linked)[rng.next(len(linked))]
                    item = (pick[0], pick[1], pick[2].hex())
                    if item not in deletes:
                        deletes.append(item)
            for p_, s_, o in deletes:
                linked.discard((p_, s_, bytes.fromhex(o)))
            for p_, s_, o in inserts:
                linked.add((p_, s_, bytes.fromhex(o)))
            commits.append({"version": version(epoch, 2 * k), "inserts": [list(i) for i in inserts],
                            "deletes": [list(d) for d in deletes]})
        ops = sum(len(c["inserts"]) + len(c["deletes"]) for c in commits)
        rounds.append({"position": position, "horizon": horizon, "flags": 0, "frame_length": 1_048_576 + 4096 * ops,
                       "commits": commits})
    return rounds


def engine_vector(vec):
    rounds = engine_rounds()
    a = Engine(ENGINE_SETTINGS, "a", check_admission=True)
    for rnd in rounds:
        a.apply(rnd)
    flagged = [r["position"] for r in rounds if r["flags"] & 1]
    assert all(any(c["position"] == p for c in a.checkpoints) for p in flagged), "a flagged round is a checkpoint"
    # A second replica crashes between its second and third checkpoints, recovers from the grid alone and replays.
    second, third = a.checkpoints[1]["position"], a.checkpoints[2]["position"]
    crash_at = min(second + 5, third - 1)
    assert crash_at > second
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
    grid_blocks = bytearray()
    for i in range(len(a.grid) // BLOCK):
        blk = a.grid[i * BLOCK:(i + 1) * BLOCK]
        if any(blk):
            grid_blocks += u32(i) + blk
    (vec / "tiles" / "engine_grid.blocks").write_bytes(bytes(grid_blocks))
    return {
        "specification": "STORAGE.md sections 12 to 17, over one relation's forward and reverse families",
        "settings": dict(ENGINE_SETTINGS), "derivation": f"{derivation(ENGINE_SETTINGS):016x}",
        "families": {n: {"kind": f["kind"], "direction": f["direction"], "subject": f["subject"].hex(),
                         "tenant": TENANT, "graph": GRAPH.hex(), "group": GROUP, "family_key": fam_key(f).hex(),
                         "row_key": "the subject's slot pair" if f["row_fixed"] else "the object's term encoding"}
                     for n, f in FAMILIES.items()},
        "superblock_fields": {"cluster": 0x0123_4567_89AB_CDEF, "partition": 1, "replica": 1, "mapping": 1,
                              "committed": "the commit version of the last committed entry applied"},
        "rounds": rounds,
        "admission": a.admission,
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
    position, horizon, committed = CHECKPOINT
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
                   position=position, horizon=horizon, committed=committed, extent_bytes=small["extent_bytes"],
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

    def tile_case(name, raw, refusal, note="", settings=None):
        refusals.append({"name": name, "structure": "tile", "hex": raw.hex(), "refusal": refusal, "note": note,
                         **({"settings": settings} if settings else {})})

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
    entry_rows = [slot_pair(PERSON, r) for r in (0, 0, 2, 5, 5, 5)]
    base = [rows_s, ends_s, ints_section(3, [3] * 6), ints_section(4, list(range(6))), ints_section(5, [0xABC001] * 6),
            (11, 6, enc_filter([r + nb[i] for i, r in enumerate(entry_rows)], 10)), ints_section(12, [PERSON] * 3)]
    hdr = dict(tenant=TENANT, subject=KNOWS, direction=1, group=GROUP, built=1, horizon=0, entries=6, rows=3)
    good_digest = digest_sum(r + nb[i] + NO_STATEMENT for i, r in enumerate(entry_rows))
    tile_case("unknown section id 15", tile(1, 1, base + [(15, 1, enc_for([0]))], digest=good_digest, **hdr), "Section")
    tile_case("prefix section missing", tile(1, 1, base[:2] + base[3:], digest=good_digest, **hdr), "Section")
    tile_case("row_prefixes section missing", tile(1, 1, base[:-1], digest=good_digest, **hdr), "Section",
              "section 7.3: section 12 is present in kinds 1, 2 and 5")
    tile_case("rows as BYTES in kind 1",
              tile(1, 1, [(1, 4, enc_bytes([slot_pair(PERSON, r) for r in (0, 2, 5)]))] + base[1:],
                   digest=good_digest, **hdr), "Encoding")
    tile_case("prefix count 5 of 6 entries", tile(1, 1, base[:2] + [ints_section(3, [3] * 5)] + base[3:],
                                                  digest=good_digest, **hdr), "Count")
    tile_case("prefix past 2^16", tile(1, 1, base[:2] + [ints_section(3, [3] * 5 + [1 << 16])] + base[3:],
                                       digest=good_digest, **hdr), "Range")
    tile_case("rows descending", tile(1, 1, [ints_section(1, [5, 2, 0])] + base[1:], digest=good_digest, **hdr), "Order")
    tile_case("digest of other entries", tile(1, 1, base, digest=good_digest ^ 1, **hdr), "Digest")
    tile_case("valid control for the cases above", tile(1, 1, base, digest=good_digest, **hdr), "none")
    tile_case("FILTER section while filter_bits_per_key is 0", tile(1, 1, base, digest=good_digest, **hdr), "Section",
              "section 6.3: the valid control above, read with filter_bits_per_key 0, under which section 7.3 "
              "excludes section 11", settings={"filter_bits_per_key": 0})
    # Section 7.3, section 10 of an adjacency block: tiles.json's adjacency_reverse_versioned with one entry's deleted
    # version set to its created version, where a nonzero one must lie above it.
    built, previous, horizon = FIXTURE["reverse"]
    stale = [(term_of_id(person(17, 0xAB_C001)), [(slot_pair(ORG, 0), None, [], version(42, 4), 0),
                                                  (slot_pair(PERSON, 2), None, [], version(42, 4), version(42, 4)),
                                                  (slot_pair(PERSON, 0), None, [], version(40, 2), 0)]),
             (term_of_id(person(18, 0xAB_C002)), [(slot_pair(PERSON, 0), None, [], version(41, 6), version(42, 8))])]
    tile_case("adjacency deleted version equal to its created version",
              encode_block(3, stale, row_fixed=False, subject=KNOWS, direction=2, built=built, previous=previous,
                           horizon=horizon)[0], "Range",
              "section 7.3: tiles.json's adjacency_reverse_versioned with entry 1's deleted version set to "
              "version(42, 4), nonzero and not above its created version(42, 4)")
    alice = person(17, 0xAB_C001)
    tile_case("record deleted version below its created version",
              record_run(6, [], class_i=False, group=GROUP, versioned_recs=[
                  (alice + be(GROUP, 3), be(1, 8), version(45, 4), version(45, 2))])[0], "Range",
              "section 9.1: a kind 6 record created at version(45, 4) and deleted at version(45, 2)")
    tile_case("free set with bit 0 clear",
              free_set_tile(DEFAULTS["grid_extents"], {1, 2, 5}, CHECKPOINT[0], 0, CHECKPOINT[1]), "Range",
              "section 10: extents 1, 2 and 5 allocated and extent 0 not; grid_extents at its default")
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
    section_case("FIXED width past value_bytes_max, 16385 at the defaults", 5,
                 u32(0) + u32(DEFAULTS["tile_bytes_max"] // 4 + 1), "Encoding")
    refusals.extend(sec)
    raw0 = bytearray((vec / "tiles" / "superblock_copy0.block").read_bytes())
    raw0[64] ^= 0x01
    refusals.append({"name": "superblock with a flipped bit", "structure": "superblock in block 0",
                     "hex": bytes(raw0).hex(), "refusal": "Checksum"})
    raw1 = bytearray((vec / "tiles" / "superblock_copy1.block").read_bytes())
    raw1[10] = 2
    refusals.append({"name": "superblock copy field other than its block", "structure": "superblock in block 1",
                     "hex": bytes(raw1).hex(), "refusal": "Copy"})
    write_json("refusals.json", {"specification": "STORAGE.md sections 4, 6.3, 7.3, 9.1, 10 and 19.4",
                                 "cases": refusals})

    kinds = {}
    for name, (raw, fence, desc) in more_tiles().items():
        (vec / "tiles" / f"{name}.tile").write_bytes(raw)
        kinds[name] = {"input": desc, "header": header_fields(raw), "fence_hex": fence.hex(),
                       "tile_key_hex": tile_key(raw, fence).hex(), "file": f"tiles/{name}.tile", "length": len(raw),
                       "checksum": f"{int.from_bytes(raw[8:16], 'little'):016x}", "digest": raw[104:120].hex()}
    for name, raw in (("manifest_leaf", leaf), ("manifest_root", root)):
        kinds[f"record_{name}"] = {"input": {"note": f"kind {raw[7]}: tiles.json's {name}, the same file"},
                                   "header": header_fields(raw), "file": f"tiles/{name}.tile", "length": len(raw),
                                   "checksum": f"{int.from_bytes(raw[8:16], 'little'):016x}",
                                   "digest": raw[104:120].hex()}
    record_kinds = sorted({v["header"]["kind"] for v in kinds.values() if v["header"]["shape"] == 3})
    assert record_kinds == list(range(1, 21)), record_kinds
    write_json("kinds.json", {"specification": "STORAGE.md sections 7 to 9: every adjacency and column kind tiles.json "
                                               "leaves out, and a run of every record kind",
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
