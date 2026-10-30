#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The golden frames of every command's body, laid out byte by byte from the layout tables of PROTOCOL.md (the
# header's offsets, the section table, the string lists, the pattern slots, the change records, the key-value
# entries, the term table), independently of the Rust code. Checksums with Python's xxhash 3.6.0 (libxxhash 0.8.2),
# whose XXH3-64 group 2 cross-checked against xxHash v0.8.3's reference vectors; BLAKE2b-128 of a hashed lexical
# form with hashlib.
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
SKOLEM = "https://w3id.org/ultragraphdb/ns/namespace/0/.well-known/genid/"


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
# 3 Register: the session assigned.
golden("register", 3, command_header([(0, le(0x1122_3344_5566_7788, 8))]), sections([]))
# 4 Begin: namespace 3, read version, read-only, on stream 5.
golden("begin", 4, command_header([(0, le(3, 8)), (8, le(0xAB_CDEF_0002, 8)), (16, bytes([1]))]), sections([]),
       stream=5)
# 5 Commit: session 9, request 7, commit version 1000; two results.
results = le(0x10, 8) + bytes([1]) + bytes(7) + le(0x20, 8) + bytes([0]) + bytes(7)
golden("commit", 5, command_header([(0, le(9, 8)), (8, le(7, 8)), (16, le(1000, 8))]), sections([results]),
       stream=5)
# 6 Abort.
golden("abort", 6, command_header([]), sections([]), stream=5)
# 7 Read: in the transaction of stream 5, limit 100, statement IDs asked (flags bit 0), the pattern
# (?, <a>, <name>, ?), a resume key; on a stream of its own.
terms = Terms()
slots = pattern(["unbound", iri(f"{EX}a"), iri(f"{EX}name"), "unbound"], terms)
golden("read", 7, command_header([(0, le(5, 8)), (8, le(100, 4)), (12, le(1, 4))]),
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
change(4, 0, [ABSENT, 7, 5])
change(2, 0, [ABSENT, A, KNOWS, iri(f"{EX}c"), iri(f"{EX}since"), lit("2020", f"{XSD}gYear")])
change(6, 0, [ABSENT, 9, 0, iri(f"{EX}weight"), lit("0.5", f"{XSD}decimal")])
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

# 10 Prepare: (default graph, $0, <name>, ?), the parameter a node (1); the reply with statement 1, a schema id and
# the result column a term (2).
for name, statement, schema, result_types in [("prepare", 0, 0, b""), ("prepare_reply", 1, 0x5C4E_3A2B_1D0F_7E6A,
                                                                       bytes([2]))]:
    terms = Terms()
    slots = pattern(["default", 0, iri(f"{EX}name"), "unbound"], terms)
    golden(name, 10, command_header([(0, le(0, 8)), (8, le(statement, 8)), (16, le(schema, 8))]),
           sections([slots] + terms.sections() + [bytes([1]), result_types]), stream=13)
# 11 Execute: statement 1, in the transaction of stream 5, no limit, no statement IDs, one parameter, no resume key.
terms = Terms()
params = [terms.add(iri(f"{EX}a"))]
golden("execute", 11, command_header([(0, le(1, 8)), (8, le(5, 8))]),
       sections(terms.sections() + [u32s(params), b""]), stream=15)


# 12 Describe: the request, and the answer: labels, relationship types, property keys, then the declarations.
def describe(labels, rels, props, read_version):
    global records, args, strings, terms
    strings, records, args, terms = Strings(), b"", [], Terms()
    secs = [u32s([strings.intern(s) for s in group]) for group in (labels, rels, props)]
    change(8, 1, [f"{EX}name", ABSENT, 1, "term", 0])
    change(8, 0, [f"{EX}knows", ABSENT, 0, 0])
    change(9, 0, [f"{EX}Person", ABSENT], extra=1)
    body = sections(strings.sections() + secs + [records, u32s(args)])
    return command_header([(0, le(0, 8)), (8, le(read_version, 8))]), body


golden("describe", 12, command_header([]), sections([b"", b"", b"", b"", b"", b"", b""]), stream=17)
ch, body = describe([f"{EX}Person"], [f"{EX}knows"], [f"{EX}name"], 42)
golden("describe_reply", 12, ch, body, stream=17)
SCHEMA_ID = xxhash.xxh3_64_intdigest(describe([], [], [], 42)[1])

# 13 Result: two columns, three rows; terms in order of first use, row by row; cells column by column; the last
# term a blank node, tag 0x02, its text its skolem IRI; each row's statement ID, as a Read with flags bit 0 asks.
def uid(n, tag=0x01):
    return bytes([tag]) + n.to_bytes(8, "big")


LONG = "a long lexical form beyond fifteen"
encodings = [uid(5), bytes([0x20, 2]) + b"Al", bytes([0x21]) + hashlib.blake2b(LONG.encode(), digest_size=16).digest(),
             uid(6, 0x02)]
strings = Strings()
texts = [(strings.intern(f"{EX}a"), ABSENT), (ABSENT, ABSENT), (strings.intern(LONG), ABSENT),
         (strings.intern(f"{SKOLEM}6"), ABSENT)]
offsets, pos = [], 0
for e in encodings:
    offsets.append(pos)
    pos += len(e)
cells = u32s([0, 0, 3] + [1, 2, ABSENT])
golden("result", 13, command_header([(0, le(2, 4)), (4, le(3, 4))]),
       sections([cells, u32s(offsets), b"".join(encodings), u32s([v for t in texts for v in t])]
                + strings.sections() + [b"rk", b"".join(le(i, 8) for i in (0x1001, 0x1002, 0x1003))]),
       stream=11, flags=1)
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

# 19 Drop: graph <g> of namespace 3, through session 9's request 8, on a stream of its own; the reply with the
# drop's commit version 2002.
for name, commit_version in [("drop", 0), ("drop_reply", 2002)]:
    terms = Terms()
    terms.add(iri(f"{EX}g"))
    golden(name, 19, command_header([(0, bytes([2])), (8, le(3, 8)), (16, le(9, 8)), (24, le(8, 8)),
                                     (32, le(commit_version, 8))]), sections(terms.sections()), stream=19,
           flags=1 if commit_version else 0)


# 17 Round: position 3, its header's sequence, its parent, incarnation 1, first version 2000, three transactions; the
# second aborted with a conflict on a list (detail 1); the first record acted for principal 7. Two write-set records, the second's delta 5,000 bytes, so the body takes chunks; the
# first carries two commit-time operations' results in section 9 (uid u64, created u8, 7 zero bytes), the second none.
def record(namespace, read_version, session, request, tables, commit_record, results=(), operation=None,
           principal=0):
    fixed = le(namespace, 8) + le(read_version, 8) + le(session, 8) + le(request, 8) + le(principal, 8)
    secs = [kv(tables.get(i, [])) for i in range(5)] + [commit_record] + [kv(tables.get(i, [])) for i in (6, 7, 8)]
    secs.append(b"".join(le(uid, 8) + bytes([created]) + bytes(7) for uid, created in results))
    secs.append(b"" if operation is None else le(operation[0], 2) + bytes(6) + le(operation[1], 8))
    return sections(secs, fixed)


rec0 = record(0, 1998, 4, 2, {0: [(b"\x10key-a", b"delta-a")], 1: [(b"\x20dict", None)],
                              4: [(b"sess-4", b"reply")], 6: [(b"\x10key-a", b"")],
                              7: [(b"\x10key-a", b"\x01obj")], 8: [(b"\x30a", b"\x30b")]}, b"cr",
             results=[(0x2A, 1), (0x2B, 0)], principal=7)
rec1 = record(0, 1990, 6, 9, {0: [(b"\x10big", bytes((i * 31 + 7) % 251 for i in range(5000)))]}, b"")
decisions = (le(0, 2) + le(0, 2) + le(0, 4) + le(0, 8) + le(2, 2) + le(0, 2) + le(ABSENT, 4) + le(1, 8)
             + le(0, 2) + le(0, 2) + le(1, 4) + le(0, 8))
rec_table, rec_data = b"", b""
for r in (rec0, rec1):
    rec_data += b"\0" * (align8(len(rec_data)) - len(rec_data))
    rec_table += le(len(rec_data), 4) + le(len(r), 4)
    rec_data += r
round_ch = command_header([(0, le(3, 8)), (8, le(0x0F0E_0D0C_0B0A_0908, 8)), (16, le(1, 8)), (24, le(2000, 8)),
                           (32, le(3, 4))])
golden("round", 17, round_ch, sections([decisions, rec_table, rec_data]), sequence=3)

out = Path(sys.argv[1])
for name, (data, size, checksum, checksum_body) in GOLDEN.items():
    (out / f"{name}.frame").write_bytes(data)
    print(f"{name:18} size {size:6}  checksum {checksum:#018x}  checksum_body {checksum_body:#018x}")
print(f"describe_reply schema id {SCHEMA_ID:#018x}")
print(f"write terms {WRITE_TERMS}")
print(f"round record 0: {len(rec0)} bytes, record 1: {len(rec1)} bytes, records at {len(decisions)}")
