#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The golden frames of every command's body, laid out byte by byte from the layout tables of PROTOCOL.md (the
# header's offsets, the section table, the string lists, the pattern slots, the change records, the key-value
# entries, the term table), independently of the Rust code. Checksums with Python's xxhash 3.6.0 (libxxhash 0.8.2),
# whose XXH3-64 group 2 cross-checked against xxHash v0.8.3's reference vectors; SHA-256 truncated to 16 bytes of a
# hashed lexical form with hashlib.
#
#   python3 -I generate_bodies.py <output directory>
#
# writes one <name>.frame per golden frame and prints each frame's size, checksum and checksum_body, the values
# tests/utils/golden_bodies.rs asserts.

import hashlib
import sys
from pathlib import Path

import xxhash

CLUSTER = 0x0123_4567_89AB_CDEF
RELEASE = 0x0005_0000
ABSENT = 0xFFFF_FFFF
XSD = "http://www.w3.org/2001/XMLSchema#"
EX = "http://ex.org/"
SKOLEM = "https://w3id.org/ultragraphdb/ns/tenant/0/.well-known/genid/"


def le(value, width, signed=False):
    return value.to_bytes(width, "little", signed=signed)


def align8(n):
    return (n + 7) // 8 * 8


def command_header(fields):
    out = bytearray(128)
    for offset, data in fields:
        out[offset:offset + len(data)] = data
    return bytes(out)


def sections(secs, fixed=b""):
    """The body layout: the fixed part, the table of (offset, len) u32 pairs, each section at the end of the one
    before rounded up to 8, zero padding, nothing after the last."""
    assert len(fixed) % 8 == 0
    pos = len(fixed) + 8 * len(secs)
    table = []
    for s in secs:
        pos = align8(pos)
        table.append((pos, len(s)))
        pos += len(s)
    body = bytearray(fixed)
    for offset, length in table:
        body += le(offset, 4) + le(length, 4)
    for (offset, _), s in zip(table, secs):
        body += b"\0" * (offset - len(body))
        body += s
    return bytes(body)


class Strings:
    """A string list: interned in order of first use, as the builders intern."""

    def __init__(self):
        self.items = []

    def intern(self, s):
        if s in self.items:
            return self.items.index(s)
        self.items.append(s)
        return len(self.items) - 1

    def push(self, s):
        self.items.append(s)
        return len(self.items) - 1

    def sections(self):
        ends, text, end = b"", b"", 0
        for s in self.items:
            data = s.encode("utf-8")
            text += data
            end += len(data)
            ends += le(end, 4)
        return [ends, text]


def u32s(values):
    return b"".join(le(v, 4) for v in values)


def kv(entries):
    out = b""
    for key, value in entries:
        entry = le(len(key), 4) + le(ABSENT if value is None else len(value), 4) + key + (value or b"")
        out += entry + b"\0" * (align8(len(entry)) - len(entry))
    return out


# Terms of a request: a term table of 24-byte records (kind u8, direction u8, 6 zero bytes, then two u32 pairs) and a
# text section, the texts back to back in table order. Kinds: 1 IRI, 2 blank node, 3 literal (second text the
# datatype IRI, empty for xsd:string), 4 language-tagged literal (second text the tag; direction 0, 1 ltr, 2 rtl),
# 5 triple term (the pairs hold the subject, predicate and object indices, then 0).
def iri(s):
    return (1, 0, s, "")


def blank(label):
    return (2, 0, label, "")


def lit(lexical, datatype=""):
    return (3, 0, lexical, datatype)


def lang(lexical, tag, direction=0):
    return (4, direction, lexical, tag)


def triple(s, p, o):
    return (5, s, p, o)


class Terms:
    """A term table, each term once, interned in order of first use; a triple's parts before the triple."""

    def __init__(self):
        self.entries = []

    def add(self, term):
        """A term's index; a triple's parts may be terms or indices already in the table."""
        if isinstance(term, int):
            return term
        if term[0] == 5:
            term = (5, self.add(term[1]), self.add(term[2]), self.add(term[3]))
        if term in self.entries:
            return self.entries.index(term)
        self.entries.append(term)
        return len(self.entries) - 1

    def sections(self):
        records, text = b"", b""
        for entry in self.entries:
            if entry[0] == 5:
                records += bytes([5, 0]) + bytes(6) + le(entry[1], 4) + le(entry[2], 4) + le(entry[3], 4) + le(0, 4)
                continue
            kind, direction, first, second = entry
            a, b = first.encode("utf-8"), second.encode("utf-8")
            records += bytes([kind, direction]) + bytes(6) + le(len(text), 4) + le(len(a), 4)
            text += a
            records += le(len(text), 4) + le(len(b), 4)
            text += b
        return [records, text]


# Pattern slots: kind 0 unbound, 1 default graph, 2 bound (value: term index), 3 parameter (value: its number).
def pattern(slots, terms):
    out = b""
    for slot in slots:
        if slot == "unbound":
            out += le(0, 4) + le(0, 4)
        elif slot == "default":
            out += le(1, 4) + le(0, 4)
        elif isinstance(slot, int):
            out += le(3, 4) + le(slot, 4)
        else:
            out += le(2, 4) + le(terms.add(slot), 4)
    return out


def frame(command, ch, data, stream=0, flags=0, sequence=1):
    """A frame laid out from the header table, its body split into chunks of 3,840 then 4,096 bytes, each leaf
    XXH3-64 seeded with its index, the table appended when there is more than one chunk."""
    chunks = [data[:3840]] + [data[i:i + 4096] for i in range(3840, len(data), 4096)]
    leaves = [xxhash.xxh3_64_intdigest(c, seed=i) for i, c in enumerate(chunks)]
    if len(chunks) == 1:
        body, checksum_body = data, leaves[0]
    else:
        table = b"".join(le(l, 8) for l in leaves)
        body, checksum_body = data + table, xxhash.xxh3_64_intdigest(table)
    size = 256 + len(body)
    tail = (le(checksum_body, 8) + le(sequence, 16) + le(CLUSTER, 8) + le(size, 4) + le(0, 4) + le(RELEASE, 4)
            + le(1, 2) + le(command, 2) + le(stream, 8) + bytes([0, flags]) + bytes(62) + ch)
    assert len(tail) == 248
    checksum = xxhash.xxh3_64_intdigest(tail)
    return le(checksum, 8) + tail + body, size, checksum, checksum_body


GOLDEN = {}


def golden(name, command, ch, data, stream=0, flags=0, sequence=1):
    GOLDEN[name] = frame(command, ch, data, stream, flags, sequence)


# 1 Hello: protocol_min 1, protocol_max 1, release, max_frame 1 MiB; the client's name. The client's first frame,
# sequence 1: PROTOCOL.md section 2.4's worked example.
golden("hello", 1, command_header([(0, le(1, 2)), (2, le(1, 2)), (4, le(RELEASE, 4)), (8, le(1 << 20, 4))]),
       sections([b"ultragraph-test"]))
# 2 Welcome: protocol 1, auth 1, release, max_frame 1 MiB, 64 streams, initial credit 4, the largest write set
# 1,040,384 bytes, a transaction's 10,000 ms; the server's name and its one mechanism. The server's first frame.
mechanisms = Strings()
mechanisms.intern("SCRAM-SHA-256")
golden("welcome", 2, command_header([(0, le(1, 2)), (2, bytes([1])), (4, le(RELEASE, 4)), (8, le(1 << 20, 4)),
                                     (12, le(64, 4)), (16, le(4, 4)), (20, le(1_040_384, 4)),
                                     (24, le(10_000, 4))]), sections([b"ultragraph 0.1"] + mechanisms.sections()))
# The Welcome of a server without authentication: auth 0, no mechanism, max_frame at its floor of 524,288.
golden("welcome_no_auth", 2, command_header([(0, le(1, 2)), (4, le(RELEASE, 4)), (8, le(1 << 19, 4)),
                                             (12, le(64, 4)), (16, le(4, 4)), (20, le(1_040_384, 4)),
                                             (24, le(10_000, 4))]), sections([b"ultragraph 0.1", b"", b""]))
# 18 Authenticate: RFC 7677's SCRAM-SHA-256 exchange (section 3) on stream 0, the client's frames 2 and 3, the
# server's 2 and 3; the server's final names principal 7 and an expiry.
CLIENT_FIRST = b"n,,n=user,r=rOprNGfwEbeRWgbNEkqO"
SERVER_FIRST = b"r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096"
CLIENT_FINAL = (b"c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,"
                b"p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=")
SERVER_FINAL = b"v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4="
golden("authenticate_client_first", 18, command_header([]), sections([b"SCRAM-SHA-256", CLIENT_FIRST]),
       sequence=2)
golden("authenticate_server_first", 18, command_header([(0, le(1, 4))]), sections([b"", SERVER_FIRST]), sequence=2)
golden("authenticate_client_final", 18, command_header([(0, le(2, 4))]), sections([b"", CLIENT_FINAL]), sequence=3)
golden("authenticate_server_final", 18, command_header([(0, le(3, 4)), (4, bytes([1])), (8, le(7, 8)),
                                                       (16, le(1_798_761_600_000_000, 8))]),
       sections([b"", SERVER_FINAL]), sequence=3)
# 3 Register: the session assigned, 16 bytes (section 6.1).
SESSION = bytes(range(0x11, 0x21))
golden("register", 3, command_header([(0, SESSION)]), sections([]))


def token(partition, position, epoch):
    """A session token (section 4.8): partition u32, 4 zero bytes, position u64, epoch u64."""
    return le(partition, 4) + bytes(4) + le(position, 8) + le(epoch, 8)


# 4 Begin (section 6.2): tenant 3, read version 0, read-only, level 2 (monotonic-fast), deadline 50 ms, the token
# of partition 2's commit at position 41 in epoch 30; on stream 5.
golden("begin", 4, command_header([(0, le(3, 8)), (16, bytes([1, 2])), (20, le(50, 4)), (24, token(2, 41, 30))]),
       sections([]), stream=5)
# A read-only Begin at level 0 naming its read version, no token, the Welcome's deadline.
golden("begin_named", 4, command_header([(0, le(4, 8)), (8, le((29 << 22) | 0x3FFFFF, 8)), (16, bytes([1, 0]))]),
       sections([]), stream=7)
# 5 Commit through no session (section 6.3): the reply of a declaration, commit version the first of epoch 32, the zero
# token, no results.
golden("commit_no_session", 5, command_header([(24, le(32 << 22, 8))]), sections([b"", b"", b""]), stream=9)
# 5 Commit (section 6.3): session, request 7, commit version (30 << 22) | 100, the token of partition 2's round at
# position 41 in epoch 30; three results (value i64, node u32, created u8, 3 zero bytes): an insert-if-absent that
# found <x>, a counter add's new value -5 and a compare-and-set; the term table holding <x>, the result's node.
COMMIT_VERSION = (30 << 22) | 100
results = (le(0, 8) + le(0, 4) + bytes(4) + le(-5, 8, signed=True) + le(ABSENT, 4) + bytes(4)
           + le(0, 8) + le(ABSENT, 4) + bytes(4))
terms = Terms()
terms.add(iri(f"{EX}x"))
golden("commit", 5, command_header([(0, SESSION), (16, le(7, 8)), (24, le(COMMIT_VERSION, 8)),
                                    (32, token(2, 41, 30))]), sections([results] + terms.sections()), stream=5)
# The request: the session and request 7 alone, three empty sections.
golden("commit_request", 5, command_header([(0, SESSION), (16, le(7, 8))]), sections([b"", b"", b""]), stream=5)
# 6 Abort.
golden("abort", 6, command_header([]), sections([]), stream=5)
# 7 Read: in the transaction of stream 5, limit 100, the pattern (?, <a>, <name>, ?), a resume key; on a stream of
# its own.
terms = Terms()
slots = pattern(["unbound", iri(f"{EX}a"), iri(f"{EX}name"), "unbound"], terms)
golden("read", 7, command_header([(0, le(5, 8)), (8, le(100, 4))]),
       sections([slots] + terms.sections() + [b"\x01\x02\x03"]), stream=11)

# 8 Write: the command header all zero; change records of 16 bytes (kind, flags, 0, 0, args_start, args_len, 0), args, the term
# table, then the names a declaration takes. A tuple arg is a term, a str a name, an int itself.
terms, strings = Terms(), Strings()
records, args = b"", []


def change(kind, flags, items, extra=0):
    global records
    start = len(args)
    for item in items:
        if isinstance(item, tuple):
            args.append(terms.add(item))
        elif isinstance(item, str):
            args.append(strings.intern(item))
        else:
            args.append(item)
    records += bytes([kind, flags, 0, 0]) + le(start, 4) + le(len(args) - start, 4) + le(extra, 4)


A, KNOWS, B = iri(f"{EX}a"), iri(f"{EX}knows"), iri(f"{EX}b")
change(1, 0, [ABSENT, A, KNOWS, B])
change(3, 0, [iri(f"{EX}g"), blank("b1"), iri(f"{EX}nick"), lang("x", "en")])
change(5, 0, [ABSENT, A, iri(f"{EX}nick")])
REIFIER = f"{SKOLEM}00002a-00000000000000000000000007"
change(4, 0, [ABSENT, iri(REIFIER)])
change(2, 0, [ABSENT, A, KNOWS, iri(f"{EX}c"), iri(f"{EX}since"), lit("2020", f"{XSD}gYear")])
change(6, 0, [ABSENT, iri(f"{SKOLEM}00002a-00000000000000000000000009"), iri(f"{EX}weight"),
              lit("0.5", f"{XSD}decimal")])
change(7, 1, [ABSENT, A, KNOWS, B, iri(f"{EX}since"), ABSENT])
# A statement about a statement: <a> <says> <<( <a> <knows> <b> )>>; a plain string and a right-to-left string.
change(1, 0, [ABSENT, A, iri(f"{EX}says"), triple(A, KNOWS, B)])
change(1, 0, [ABSENT, A, iri(f"{EX}label"), lit("plain")])
change(1, 0, [ABSENT, A, iri(f"{EX}label"), lang("مرحبا", "ar", 2)])
# A predicate: property (1) | single (4) | unique (8) | count index (16).
change(8, 1 | 4 | 8 | 16, [f"{EX}name", f"{XSD}string", 2, "term", "exact", 1, f"{EX}Person"])
# Either (2) | vector index (32), hnsw with two options.
change(8, 2 | 32, [f"{EX}embedding", ABSENT, 0, 0, "hnsw", 2, "metric", "euclidean", "maxLevels", "3"])
change(9, 0, [f"{EX}Person", f"{EX}Agent"])
change(10, 0, [f"{EX}Employment", f"{EX}employee", f"{EX}employer"])
golden("write", 8, command_header([]),
       sections([records, u32s(args)] + terms.sections() + strings.sections()), stream=5)
WRITE_TERMS = len(terms.entries)

# 9 Operation: upsert-and-link of epi.id "x", one property for creation, one link with the node as its object.
terms = Terms()
op_args = [ABSENT] + [terms.add(t) for t in [iri(f"{EX}epi.id"), lit("x"), iri(f"{EX}label"), lit("first"),
                                             iri(f"{EX}HAS_ENTITY"), iri(f"{EX}m1")]] + [1]
golden("operation", 9, command_header([(0, bytes([2])), (16, le(1, 4)), (20, le(1, 4))]),
       sections([u32s(op_args)] + terms.sections()), stream=5)
# A counter add of -5.
terms = Terms()
op_args = [ABSENT] + [terms.add(t) for t in [iri(f"{EX}counters"), iri(f"{EX}next-id")]]
golden("operation_counter", 9, command_header([(0, bytes([4])), (8, le(-5, 8, signed=True))]),
       sections([u32s(op_args)] + terms.sections()), stream=7)
# An insert-if-absent whose value is a triple term nested 32 deep, the most a table holds: t0 = <a>, t(k+1) =
# <<( t(k) <p> t(k) )>>.
terms = Terms()
p = terms.add(iri(f"{EX}p"))
deep = terms.add(iri(f"{EX}a"))
for _ in range(32):
    deep = terms.add(triple(deep, p, deep))
op_args = [ABSENT, p, deep]
golden("operation_deep", 9, command_header([(0, bytes([1]))]), sections([u32s(op_args)] + terms.sections()),
       stream=9)

# 10 Prepare (section 8.1): (default graph, $0, <name>, ?), the parameter a node (1); the reply with statement 1, a
# schema id and the result columns: the object a term (2), then the reifier column a node (1). Ten sections: the
# warnings (section 4.9), sections 7 to 9, empty for a pattern.
for name, statement, schema, result_types in [("prepare", 0, 0, b""), ("prepare_reply", 1, 0x5C4E_3A2B_1D0F_7E6A,
                                                                       bytes([2, 1]))]:
    terms = Terms()
    slots = pattern(["default", 0, iri(f"{EX}name"), "unbound"], terms)
    golden(name, 10, command_header([(0, le(0, 8)), (8, le(statement, 8)), (16, le(schema, 8))]),
           sections([slots] + terms.sections() + [bytes([1]), result_types, b"", b"", b"", b"", b""]), stream=13)
# A Prepare of a GQL text of tenant 4 with one parameter, $who, a node; the reply: statement 2, a schema id, the column
# names following the parameter's and the columns' types, a node and a string; no warning in either.
GQL = "MATCH (n) WHERE n = $who RETURN n, n.name"
for name, statement, schema, result_types, words in [("prepare_text", 0, 0, b"", ["who"]),
                                                     ("prepare_text_reply", 2, 0x5C4E_3A2B_1D0F_7E6A, bytes([1, 6]),
                                                      ["who", "n", "name"])]:
    names = Strings()
    for word in words:
        names.push(word)
    golden(name, 10, command_header([(0, le(4, 8)), (8, le(statement, 8)), (16, le(schema, 8)), (24, bytes([1]))]),
           sections([GQL.encode(), b"", b"", bytes([1]), result_types] + names.sections() + [b"", b"", b""]),
           stream=47)
# 11 Execute: statement 1, in the transaction of stream 5, no limit, one parameter, no resume key.
terms = Terms()
params = [terms.add(iri(f"{EX}a"))]
golden("execute", 11, command_header([(0, le(1, 8)), (8, le(5, 8))]),
       sections(terms.sections() + [u32s(params), b""]), stream=15)


# 12 Describe (section 8.3): the request, nine empty sections; and the answer: labels, relationship types, property
# keys, then the declarations, each record's `extra` 0 (section 4.6), and no entry (sections 7 and 8 empty).
def describe(labels, rels, props, read_version):
    global records, args, strings, terms
    strings, records, args, terms = Strings(), b"", [], Terms()
    secs = [u32s([strings.intern(s) for s in group]) for group in (labels, rels, props)]
    change(8, 0, [f"{EX}knows", ABSENT, 0, 0])
    change(8, 1, [f"{EX}name", ABSENT, 1, "term", 0])
    change(9, 0, [f"{EX}Person", ABSENT])
    body = sections(strings.sections() + secs + [records, u32s(args), b"", b""])
    return command_header([(0, le(0, 8)), (8, le(read_version, 8))]), body


golden("describe", 12, command_header([]), sections([b""] * 9), stream=17)
ch, body = describe([f"{EX}Person"], [f"{EX}knows"], [f"{EX}name"], 42)
golden("describe_reply", 12, ch, body, stream=17)
SCHEMA_ID = xxhash.xxh3_64_intdigest(describe([], [], [], 42)[1])

# 13 Result (section 9.1): two unbound slots and the reifier column, three rows, read at epoch 30 in partition 2;
# terms in order of first use, row by row; cells column by column; the first term an IRI, tag 0x01 and its text's
# hash, a blank node, tag 0x02 and its skolem IRI's hash, and the first row's reifier, tag 0x02 and its skolem IRI's
# hash, each text in the string list (section 10.1); no statement ID. Twelve sections: no column name and no warning
# (sections 7 to 11) on a pattern's batch.
def sha16(text):
    return hashlib.sha256(text.encode()).digest()[:16]


LONG = "a long lexical form beyond fifteen"
BLANK = f"{SKOLEM}00002a-0123456789abcdef0123456789"
READ_REIFIER = f"{SKOLEM}00002a-fedcba9876543210fedcba9876"
encodings = [bytes([0x01]) + sha16(f"{EX}a"), bytes([0x20, 2]) + b"Al", bytes([0x02]) + sha16(READ_REIFIER),
             bytes([0x21]) + sha16(LONG), bytes([0x02]) + sha16(BLANK)]
strings = Strings()
texts = [(strings.intern(f"{EX}a"), ABSENT), (ABSENT, ABSENT), (strings.intern(READ_REIFIER), ABSENT),
         (strings.intern(LONG), ABSENT), (strings.intern(BLANK), ABSENT)]
offsets, pos = [], 0
for e in encodings:
    offsets.append(pos)
    pos += len(e)
# Row 0: <a>, "Al", reifier; row 1: <a>, the long literal, unbound; row 2: the blank node, unbound, unbound.
cells = u32s([0, 0, 4] + [1, 3, ABSENT] + [2, ABSENT, ABSENT])
golden("result", 13, command_header([(0, le(3, 4)), (4, le(3, 4)), (8, le(30, 8)), (16, le(2, 4))]),
       sections([cells, u32s(offsets), b"".join(encodings), u32s([v for t in texts for v in t])]
                + strings.sections() + [b"rk", b"", b"", b"", b"", b""]),
       stream=11, flags=1)
# The same batch before its stream's last: not flagged, and holding no resume key.
golden("result_more", 13, command_header([(0, le(3, 4)), (4, le(3, 4)), (8, le(30, 8)), (16, le(2, 4))]),
       sections([cells, u32s(offsets), b"".join(encodings), u32s([v for t in texts for v in t])]
                + strings.sections() + [b"", b"", b"", b"", b"", b""]),
       stream=11)
# A query's first batch: two projections named, `n` and `age`, one row of an IRI that is no blank node and a null, read at
# level 2 over data that may have been retracted (monotonic 1), flagged LAST; its compilation reported no warning.
strings = Strings()
query_text = strings.intern(f"{EX}a")
names = Strings()
names.push("n")
names.push("age")
golden("result_query", 13, command_header([(0, le(2, 4)), (4, le(1, 4)), (8, le(30, 8)), (20, bytes([1]))]),
       sections([u32s([0, ABSENT]), u32s([0]), bytes([0x01]) + sha16(f"{EX}a"), u32s([query_text, ABSENT])]
                + strings.sections() + [b""] + names.sections() + [b"", b"", b""]),
       stream=49, flags=1)
# 14 Credit: two frames on stream 11.  15 Cancel.
golden("credit", 14, command_header([(0, le(2, 4))]), sections([]), stream=11)
golden("cancel", 15, command_header([]), sections([]), stream=11)
# 16 Error: a conflict on a scanned range (detail 3); and a failed compare-and-set with the value found.
golden("error", 16, command_header([(0, le(2, 2)), (8, le(3, 8))]),
       sections([b"conflict on a scanned range", b"", b""]), stream=5, flags=1)
# The value found, "done", a one-entry request term table.
terms = Terms()
terms.add(lit("done"))
golden("error_operation", 16, command_header([(0, le(8, 2))]),
       sections([b"compare-and-set found another value"] + terms.sections()), stream=7, flags=1)

# 28 and 29: an Install's sources refused, two diagnostics; a query refused, diagnostic 12 of SPARQL.
golden("error_schema", 16, command_header([(0, le(28, 2)), (8, le(2, 8))]),
       sections([b"E101 1:1 unknown type; E102 2:1 unknown unit", b"", b""]), stream=51, flags=1)
golden("error_query", 16, command_header([(0, le(29, 2)), (8, le((2 << 16) | 12, 8))]),
       sections([b"S012 1:8 a feature outside the core", b"", b""]), stream=53, flags=1)

# 19 Drop: graph <g> of tenant 3, through the session's request 8, on a stream of its own; the reply with the
# drop's commit version (31 << 22) | 2 and its token.
for name, commit_version, tok in [("drop", 0, bytes(24)), ("drop_reply", (31 << 22) | 2, token(2, 42, 31))]:
    terms = Terms()
    terms.add(iri(f"{EX}g"))
    golden(name, 19, command_header([(0, bytes([2])), (8, le(3, 8)), (16, SESSION), (32, le(8, 8)),
                                     (40, le(commit_version, 8)), (48, tok)]), sections(terms.sections()),
           stream=19, flags=1 if commit_version else 0)


# 20 DirectWrite (section 6.6): through the session's request 9 in tenant 3, the token of partition 2's commit at
# position 41 in epoch 30, on a stream of its own: an insert, an edge with one property and a delete in a graph, each
# args following the last; no names.
terms, strings = Terms(), Strings()
records, args = b"", []
change(1, 0, [ABSENT, A, KNOWS, B])
change(2, 0, [ABSENT, A, KNOWS, iri(f"{EX}c"), iri(f"{EX}since"), lit("2020", f"{XSD}gYear")])
change(3, 0, [iri(f"{EX}g"), blank("b1"), iri(f"{EX}nick"), lang("x", "en")])
golden("direct_write", 20, command_header([(0, SESSION), (16, le(9, 8)), (24, le(3, 8)), (32, token(2, 41, 30))]),
       sections([records, u32s(args)] + terms.sections() + strings.sections()), stream=21)
# The same frame as the frontend relays it: `principal` 9 filled, every other command-header byte and the body unchanged.
golden("direct_write_relayed", 20, command_header([(0, SESSION), (16, le(9, 8)), (24, le(3, 8)), (32, token(2, 41, 30)),
                                                   (56, le(9, 8))]),
       sections([records, u32s(args)] + terms.sections() + strings.sections()), stream=21)


def operation_fields(kind, delta=0, properties=0, links=0):
    """An operation's 24 bytes (section 7.3): kind, 7 zero bytes, delta i64, properties u32, links u32."""
    return bytes([kind]) + bytes(7) + le(delta, 8, signed=True) + le(properties, 4) + le(links, 4)


def operation(fields, items):
    """Operation `fields` in section 6 and its `items` in the args after those before it, each a term or itself."""
    global operations
    operations += fields
    args.extend(terms.add(item) if isinstance(item, tuple) else item for item in items)


# 21 ConditionalWrite (section 6.7): through the session's request 10 in tenant 3, the token of partition 2's commit
# at position 42 in epoch 31, on a stream of its own: one insert, then one operation of each kind, an upsert-and-link's
# link with the node as its object; the args the insert's, then each operation's in order.
terms, strings = Terms(), Strings()
records, args, operations = b"", [], b""
change(1, 0, [ABSENT, A, KNOWS, B])
operation(operation_fields(1, properties=1), [ABSENT, iri(f"{EX}epi.id"), lit("y"), iri(f"{EX}label"), lit("second")])
operation(operation_fields(2, properties=1, links=1),
          [iri(f"{EX}g"), iri(f"{EX}epi.id"), lit("x"), iri(f"{EX}label"), lit("first"), iri(f"{EX}HAS_ENTITY"),
           iri(f"{EX}m1"), 1])
operation(operation_fields(3), [ABSENT, A, iri(f"{EX}state"), lit("old"), lit("new")])
operation(operation_fields(4, delta=-5), [ABSENT, iri(f"{EX}counters"), iri(f"{EX}next-id")])
golden("conditional_write", 21, command_header([(0, SESSION), (16, le(10, 8)), (24, le(3, 8)), (32, token(2, 42, 31))]),
       sections([records, u32s(args)] + terms.sections() + strings.sections() + [operations]), stream=23)
# Through no session, holding no token, no change: section 0 empty and one counter add in tenant 4.
terms, strings = Terms(), Strings()
records, args, operations = b"", [], b""
operation(operation_fields(4, delta=1), [ABSENT, iri(f"{EX}counters"), iri(f"{EX}next-id")])
golden("conditional_write_operation_only", 21, command_header([(24, le(4, 8))]),
       sections([records, u32s(args)] + terms.sections() + strings.sections() + [operations]), stream=25)


# 22 Query: a GQL text in the transaction of stream 5, limit 100, one parameter `who` bound to <a>, a resume key.
terms = Terms()
query_value = terms.add(iri(f"{EX}a"))
query_names = Strings()
query_names.push("who")
golden("query", 22, command_header([(0, le(5, 8)), (8, bytes([1])), (12, le(100, 4))]),
       sections([GQL.encode()] + query_names.sections() + [u32s([query_value])] + terms.sections() + [b"\x01\x02"]),
       stream=55)
# 23 Install (section 8.4): a schema and a module of tenant 3, built on version 11; the reply, version 12 taking effect
# at epoch 44, `staged` 0 (no migration), and the coordination report. Seven sections: the report, the migration
# report and the warnings (section 4.9), all empty in the request; the reply holds no migration and no warning.
sources = Strings()
for word in ["crm", "namespace crm {}", "crm/people", "module people {}"]:
    sources.push(word)
golden("install", 23, command_header([(0, le(3, 8)), (8, le(11, 8)), (24, le(0, 8))]),
       sections(sources.sections() + [b"", b"", b"", b"", b""]), stream=57)
golden("install_reply", 23, command_header([(0, le(3, 8)), (8, le(12, 8)), (16, le(44, 8)), (24, le(0, 8))]),
       sections(sources.sections() + [b"coordination-free: 2", b"", b"", b"", b""]), stream=57, flags=1)


# 17 Round (section 12.1): position 3, its header's sequence, its parent, incarnation 1, first version (7 << 22) | 100,
# five entries, derivation 1; the second aborted with a conflict on a list (detail 1). Four write-set records (section
# 12.2): a transaction of tenant 3 at catalog 11, a home's registration, a build's start in tenant 4 at catalog 12,
# and a Drop of a predicate, the last; then three dictionary entries in ascending key order. The transaction's commit
# record of 4,000 bytes makes the body take chunks.
def sid(prefix, sequence, bucket):
    """A system ID's 16 big-endian bytes: type prefix, sequence, bucket, 3 reserved zero bytes (section 1)."""
    return prefix.to_bytes(2, "big") + sequence.to_bytes(8, "big") + bucket.to_bytes(3, "big") + bytes(3)


def record(kind, tenant, catalog=0, read_version=0, session=bytes(16), request=0, principal=0, inserted=(),
           removed=(), statements=(), operations=(), registrations=(), commit_record=b"", points=(), cstatements=(),
           ranges=(), results=(), result_terms=None):
    fixed = (bytes([kind]) + bytes(7) + le(tenant, 8) + le(catalog, 8) + le(read_version, 8) + session
             + le(request, 8) + le(principal, 8))
    assert len(fixed) == 64
    ops = b"".join(le(code, 2) + bytes(6) + argument for code, argument in operations)
    regs = b"".join(entity + le(group, 4) + bytes(4) + le(generation, 8) for entity, group, generation in registrations)
    res = b"".join(le(value, 8, signed=True) + le(node, 4) + bytes([created]) + bytes(3)
                   for value, node, created in results)
    terms = result_terms or Terms()
    secs = ([kv(inserted), kv(removed), kv(statements), ops, regs, commit_record, kv([(k, b"") for k in points]),
             kv(cstatements), kv(ranges), res] + terms.sections())
    assert len(secs) == 12
    return sections(secs, fixed)


SESSION = bytes(range(0x11, 0x21))
FIRST_VERSION = (7 << 22) | 100
node_terms = Terms()
node_terms.add(iri(f"{EX}x"))
rec0 = record(1, 3, catalog=11, read_version=(7 << 22) | 50, session=SESSION, request=7, principal=9,
              inserted=[(b"quad-a", sid(2, 5, 0x2A)), (b"quad-b", b"")], removed=[(b"quad-c", sid(2, 6, 0x2A))],
              statements=[(b"g+stmt-5", b"record-5"), (b"g+stmt-6", None)],
              registrations=[(sid(1, 9, 0x40), 0x40 >> 6, 3)],
              commit_record=bytes((i * 31 + 7) % 251 for i in range(4000)), points=[b"\x10key-a"],
              cstatements=[(b"\x10list-a", b"\x01obj")], ranges=[(b"\x30a", b"\x30b")],
              results=[(0, 0, 1), (-5, ABSENT, 0), (0, ABSENT, 0)], result_terms=node_terms)
rec1 = record(2, 3, registrations=[(sid(1, 12, 0x80), 0x80 >> 6, 1)])
rec2 = record(3, 4, catalog=12, operations=[(6, le(12, 8) + bytes(8))])
rec3 = record(1, 3, catalog=11, session=SESSION, request=8, principal=9, operations=[(1, sid(1, 4, 0x2A))],
              ranges=[(b"\x40p-start", b"\x40p-end")])
records = (rec0, rec1, rec2, rec3)
outcomes = [(0, 0, 0), (2, ABSENT, 1), (0, 1, 0), (0, 2, 0), (0, 3, 0)]
decisions = b"".join(le(outcome, 2) + bytes(2) + le(index, 4) + le(detail, 8) for outcome, index, detail in outcomes)
rec_table, rec_data, rec_offsets = b"", b"", []
for r in records:
    rec_data += b"\0" * (align8(len(rec_data)) - len(rec_data))
    rec_offsets.append(len(rec_data))
    rec_table += le(len(rec_data), 4) + le(len(r), 4)
    rec_data += r
dictionary = kv([(b"\x20dict-a", b"value-a"), (b"\x20dict-b", b""), (b"\x21dict-c", b"value-c")])
round_ch = command_header([(0, le(3, 8)), (8, le(0x0F0E_0D0C_0B0A_0908, 8)), (16, le(1, 8)),
                           (24, le(FIRST_VERSION, 8)), (32, le(len(outcomes), 4)), (36, le(1, 4))])
round_body = sections([decisions, rec_table, rec_data, dictionary])
golden("round", 17, round_ch, round_body, sequence=3)

out = Path(sys.argv[1])
for name, (data, size, checksum, checksum_body) in GOLDEN.items():
    (out / f"{name}.frame").write_bytes(data)
    print(f"{name:32} size {size:6}  checksum {checksum:#018x}  checksum_body {checksum_body:#018x}")
print(f"describe_reply schema id {SCHEMA_ID:#018x}")
print(f"write terms {WRITE_TERMS}")
print(f"round first version {FIRST_VERSION}; records {[len(r) for r in records]} bytes at {rec_offsets} within "
      f"section 2, which starts at body offset {round_body.index(rec_data)}; body {len(round_body)} bytes")
