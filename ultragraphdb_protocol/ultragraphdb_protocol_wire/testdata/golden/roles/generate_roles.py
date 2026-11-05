#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The golden frames of the role commands' bodies (PROTOCOL.md section 13), laid out byte by byte from the layout
# tables of PROTOCOL.md, independently of the Rust code; the helpers are generate_bodies.py's. Checksums with Python's xxhash 3.6.0 (libxxhash 0.8.2),
# whose XXH3-64 group 2 cross-checked against xxHash v0.8.3's reference vectors; SHA-256 truncated to 16 bytes of a
# hashed lexical form with hashlib.
#
#   python3 -I generate_roles.py <output directory>
#
# writes one <name>.frame per golden role frame and prints each frame's size, checksum and checksum_body, the values
# tests/utils/golden_roles.rs asserts.

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


def sha16(text):
    return hashlib.sha256(text.encode()).digest()[:16]


def sid(prefix, sequence, bucket):
    """A system ID's 16 big-endian bytes: type prefix, sequence, bucket, 3 reserved zero bytes (section 1)."""
    return prefix.to_bytes(2, "big") + sequence.to_bytes(8, "big") + bucket.to_bytes(3, "big") + bytes(3)


SESSION = bytes(range(0x11, 0x21))
TRANSACTION = bytes(range(0xA1, 0xB1))

# --- 512 to 518 ---

# 512 Join: a storage process hosting partitions 2, 5 and 9 over TCP; a compute process over a Unix socket.
golden("join_storage", 512, command_header([(0, bytes([3]))]),
       sections([b"tcp://storage-1:7300", u32s([2, 5, 9])]), stream=3)
golden("join_compute", 512, command_header([(0, bytes([2]))]),
       sections([b"unix:///run/ugdb/compute.sock", b""]), stream=3)
# 513 Members: the request, three empty sections; the reply, two compute processes by address, then storage
# partitions 2 and 5 on one process and 9 on another, addresses interned in member order.
golden("members", 513, command_header([]), sections([b"", b"", b""]), stream=5)
addresses = Strings()
members = b""
for kind, partition, address in [(2, 0, "tcp://compute-a:7400"), (2, 0, "tcp://compute-b:7400"),
                                 (3, 2, "tcp://storage-1:7300"), (3, 5, "tcp://storage-1:7300"),
                                 (3, 9, "tcp://storage-2:7300")]:
    members += bytes([kind]) + bytes(3) + le(partition, 4) + le(addresses.intern(address), 4)
golden("members_reply", 513, command_header([]), sections([members] + addresses.sections()), stream=5)
# 514 Subscribe: from the close of epoch 41.
golden("subscribe", 514, command_header([(0, le(41, 8))]), sections([]), stream=7)
# 515 EpochClose: epoch 42 at a time, more to follow; catalog versions 11 and 12 of tenant 3 and 4 of tenant 7;
# two sessions evicted, ascending by their bytes.
announcements = b"".join(le(tenant, 8) + le(version, 8) for tenant, version in [(3, 11), (3, 12), (7, 4)])
evictions = SESSION + bytes(range(0x31, 0x41))
golden("epoch_close", 515, command_header([(0, le(42, 8)), (8, le(1_798_761_600_123_456, 8)), (16, bytes([1]))]),
       sections([announcements, evictions]), stream=7)
# 516 Lease: partition 5 asks for 4,096 sequences of type 2; the reply's block 81,921 to 86,017.
golden("lease", 516, command_header([(0, le(2, 2)), (4, le(5, 4)), (8, le(4096, 8))]), sections([]), stream=9)
golden("lease_reply", 516, command_header([(0, le(2, 2)), (4, le(5, 4)), (8, le(4096, 8)), (16, le(81_921, 8)),
                                           (24, le(86_017, 8))]), sections([]), stream=9)
# 517 Placement: the request for epoch 42; the reply read at position 1,234, applying from 40 until 45: three ranges
# over partitions 1, 2, 1, two tenant runs, a leader per partition.
golden("placement", 517, command_header([(8, le(42, 8))]), sections([b"", b"", b""]), stream=11)
ranges = b"".join(le(start, 4) + le(end, 4) + le(partition, 4) + bytes(4)
                  for start, end, partition in [(0, 0x40_0000, 1), (0x40_0000, 0x80_0000, 2), (0x80_0000, 1 << 24, 1)])
runs = b"".join(le(tenant, 8) + bytes([run]) + bytes(7) for tenant, run in [(3, 0x40), (7, 0x81)])
leaders = b"".join(le(partition, 4) + le(replica, 4) for partition, replica in [(1, 3), (2, 4)])
golden("placement_reply", 517, command_header([(0, le(1_234, 8)), (8, le(42, 8)), (16, le(40, 8)), (24, le(45, 8))]),
       sections([ranges, runs, leaders]), stream=11)
# 518 Catalog: the request for tenant 3's current version; the reply, version 12 applying from epoch 44, read at
# position 1,240, its 37 opaque bytes.
golden("catalog", 518, command_header([(0, le(3, 8))]), sections([b""]), stream=13)
golden("catalog_reply", 518, command_header([(0, le(3, 8)), (8, le(12, 8)), (16, le(44, 8)), (24, le(1_240, 8))]),
       sections([bytes((i * 37 + 5) % 256 for i in range(37))]), stream=13)

# --- end 512 to 518 ---

# --- 519 to 524 ---

# 519 Challenge: the principal "user"'s salt and count; the request with iterations 0 and no salt, the reply with
# RFC 7677's salt and 4,096 iterations.
import base64

golden("challenge", 519, command_header([]), sections([b"user", b""]), stream=21)
golden("challenge_reply", 519, command_header([(0, le(4096, 4))]),
       sections([b"user", base64.b64decode("W22ZaJ0SNY7soEsUEjb6gQ==")]), stream=21)
# 520 Verify: RFC 7677's AuthMessage and ClientProof; the request (verified 0, principal 0, no signature), and the
# verified reply naming principal 7 and the ServerSignature.
AUTH_MESSAGE = (b"n=user,r=rOprNGfwEbeRWgbNEkqO,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,"
                b"s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096,c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0")
PROOF = base64.b64decode("dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=")
SIGNATURE = base64.b64decode("6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=")
golden("verify", 520, command_header([]), sections([b"user", AUTH_MESSAGE, PROOF, b""]), stream=23)
golden("verify_reply", 520, command_header([(0, bytes([1])), (8, le(7, 8))]),
       sections([b"user", AUTH_MESSAGE, PROOF, SIGNATURE]), stream=23)
# 521 Bind: partition 3, a read-write transaction (kind 1) of tenant 5, the token's epoch 30, principal 9 and the
# transaction's identity; the request with read_version 0, the reply with the watermark (29 << 22) | 500; and a
# Drop's binding (kind 2) to partition 4 in tenant 6, epoch 0, principal 11.
WATERMARK = (29 << 22) | 500
for name, read_version in [("bind", 0), ("bind_reply", WATERMARK)]:
    golden(name, 521, command_header([(0, le(3, 4)), (4, bytes([1])), (8, le(5, 8)), (16, le(read_version, 8)),
                                      (24, le(30, 8)), (32, le(9, 8)), (40, TRANSACTION)]), sections([]), stream=25)
golden("bind_drop", 521, command_header([(0, le(4, 4)), (4, bytes([2])), (8, le(6, 8)), (32, le(11, 8)),
                                         (40, SESSION)]), sections([]), stream=27)
# 522 Recall: session SESSION's reply to request 8 from partition 3.
golden("recall", 522, command_header([(0, le(3, 4)), (8, SESSION), (24, le(8, 8))]), sections([]), stream=29)
# 523 ComputeRead: a read-only transaction at level 2 (monotonic-fast) of tenant 5, its floor epoch 31, principal 9,
# limit 100, 50 ms left, the pattern (?, <a>, <name>, ?) and a resume key; and a read of a read-write transaction at
# level 1, bound to partition 3 at the Bind's version (29 << 22) | 500, with the transaction's identity, no limit,
# 9,000 ms left, the pattern (<g>, ?, <knows>, <b>) and no resume key.
terms = Terms()
slots = pattern(["unbound", iri(f"{EX}a"), iri(f"{EX}name"), "unbound"], terms)
golden("compute_read", 523, command_header([(4, bytes([1])), (5, bytes([2])), (8, le(5, 8)), (24, le(31, 8)),
                                            (32, le(9, 8)), (56, le(100, 4)), (60, le(50, 4))]),
       sections([slots] + terms.sections() + [b"rk1"]), stream=31)
terms = Terms()
slots = pattern([iri(f"{EX}g"), "unbound", iri(f"{EX}knows"), iri(f"{EX}b")], terms)
golden("compute_read_rw", 523, command_header([(0, le(3, 4)), (5, bytes([1])), (8, le(5, 8)),
                                               (16, le(WATERMARK, 8)), (32, le(9, 8)), (40, TRANSACTION),
                                               (60, le(9000, 4))]),
       sections([slots] + terms.sections() + [b""]), stream=33)
# 524 Scan: at 1 (read_version, once epoch 30 closed), the version bound of epoch 30, principal 9, no transaction,
# 1,000 entries per range, 30,000 ms to its deadline; two ranges: partition 3 from 0x10 "abc" to 0x10 "abd", partition 4 from 0x20 "k" to the
# end of its keyspace (an empty past key). Each range: partition u32, 4 zero bytes, first, first_len, past, past_len.
def scan_ranges(ranges):
    records, keys = b"", b""
    for partition, first, past in ranges:
        at = len(keys)
        records += le(partition, 4) + bytes(4) + le(at, 4) + le(len(first), 4) + le(at + len(first), 4) + le(len(past), 4)
        keys += first + past
    return [records, keys]


# Sections 2 and 3, the term table, are empty with no transaction.
golden("scan", 524, command_header([(0, bytes([1])), (8, le((30 << 22) | 0x3FFFFF, 8)), (16, le(30, 8)),
                                    (24, le(9, 8)), (48, le(1000, 4)), (52, le(30000, 4))]),
       sections(scan_ranges([(3, b"\x10abc", b"\x10abd"), (4, b"\x20k", b"")]) + [b"", b""]), stream=35)
# A read-write transaction's Scan: at 1 at the Bind's version, epoch 0, the transaction's identity, no limit, no deadline, one
# range of partition 3 holding the one key 0x30 "key" (past key 0x30 "key" 0x00), naming no IRI.
golden("scan_transaction", 524, command_header([(0, bytes([1])), (8, le(WATERMARK, 8)), (24, le(9, 8)),
                                                (32, TRANSACTION)]),
       sections(scan_ranges([(3, b"\x30key", b"\x30key\x00")]) + [b"", b""]), stream=37)
# The same transaction's Scan naming two IRIs: its first ranges hold each IRI's dictionary key alone, in the table's
# order (keyspace byte 0x1B, the tenant big-endian, kind 1, SHA-256 of the IRI truncated to 16 bytes), and a third
# holds the one forward list of a predicate and subject of the default graph (keyspace byte 0x10, the tenant, 16
# zero bytes, the predicate's and subject's system IDs).
def holding(key):
    return (3, key, key + b"\0")


def dictionary_key(text):
    return b"\x1b" + (5).to_bytes(8, "big") + b"\x01" + sha16(text)


terms = Terms()
terms.add(iri(f"{EX}a"))
terms.add(iri(f"{EX}b"))
forward_key = b"\x10" + (5).to_bytes(8, "big") + bytes(16) + sid(1, 7, 0x2A0001) + sid(1, 8, 0x2A0002)
golden("scan_transaction_terms", 524, command_header([(0, bytes([1])), (8, le(WATERMARK, 8)), (24, le(9, 8)),
                                                      (32, TRANSACTION)]),
       sections(scan_ranges([holding(dictionary_key(f"{EX}a")), holding(dictionary_key(f"{EX}b")),
                             holding(forward_key)]) + terms.sections()), stream=39)

# --- end 519 to 524 ---

# --- 525 to 530 ---
# 525 Entries: range 2 of its Scan, ended (1), read at epoch 0x1234; three entries in ascending key order, every value
# present, one of them empty (section 12.2's key-value layout).
golden("entries", 525, command_header([(0, le(3, 4)), (4, le(2, 4)), (8, bytes([1])), (16, le(0x1234, 8))]),
       sections([kv([(b"\x10key-a", b"value-a"), (b"\x10key-b", b""), (b"\x10key-b\x00", b"v")])]),
       stream=23, flags=1)
# A list larger than a frame: range 0, read at epoch 0x1234, in two frames. Each value is object records back to back:
# encoding_len u32, statements u32, the encoding, 16 bytes per statement ID. The first frame, end 0 and continues 1,
# holds list-a whole and list-b's first object, under two statement IDs. The second, end 1 and LAST, opens with list-b
# again, the key repeated: its next objects, an encoding of 4 bytes with no statement ID and an ID under one.
def object_record(encoding, statements=()):
    return le(len(encoding), 4) + le(len(statements), 4) + encoding + b"".join(statements)


golden("entries_continues", 525, command_header([(0, le(2, 4)), (9, bytes([1])), (16, le(0x1234, 8))]),
       sections([kv([(b"\x10list-a", object_record(sid(1, 21, 0x000A01))),
                     (b"\x10list-b", object_record(sid(1, 22, 0x000A02), [sid(2, 31, 0x000B01), sid(2, 32, 0x000B02)]))])]),
       stream=41)
golden("entries_continued", 525, command_header([(0, le(1, 4)), (8, bytes([1])), (16, le(0x1234, 8))]),
       sections([kv([(b"\x10list-b", object_record(b"\x30abc") + object_record(sid(1, 23, 0x000A03), [sid(2, 33, 0x000B03)]))])]),
       stream=41, flags=1, sequence=2)

# 526 Reference: registration records of 48 bytes (partition u32, group u32, iri u32, outcome u16, 2 zero bytes,
# generation u64, close u64, entity 16 bytes), the term table of the IRIs named by text, kind 1 each. Registration 0
# names <o> by its IRI for bucket group 0x2A, last seen at generation 4; 1 names a predicate by its system ID, no group;
# 2 names a blank node by its skolem IRI for group 0x10, never seen. The reply accepts 0 (generation 5, close 41) and 1
# (close 41), and refuses 2 with 26 (WrongPartition).
def registration(partition, group, iri_index, outcome, generation, close, entity):
    return (le(partition, 4) + le(group, 4) + le(iri_index, 4) + le(outcome, 2) + bytes(2) + le(generation, 8)
            + le(close, 8) + entity)


REF_O, REF_BLANK = f"{EX}o", f"{SKOLEM}00a001-0123456789abcdef0123456789"
REF_PREDICATE = sid(1, 9, 0x40)
REF_ENTITY = sid(1, 77, 0xA80)
terms = Terms()
o_index, blank_index = terms.add(iri(REF_O)), terms.add(iri(REF_BLANK))
request_regs = [(3, 0x2A, o_index, 0, 4, 0, bytes(16)), (5, ABSENT, ABSENT, 0, 0, 0, REF_PREDICATE),
                (8, 0x10, blank_index, 0, 0, 0, bytes(16))]
reply_regs = [(3, 0x2A, o_index, 0, 5, 41, REF_ENTITY), (5, ABSENT, ABSENT, 0, 0, 41, REF_PREDICATE),
              (8, 0x10, blank_index, 26, 0, 0, bytes(16))]
for name, reply, regs in [("reference", 0, request_regs), ("reference_reply", 1, reply_regs)]:
    golden(name, 526, command_header([(0, bytes([reply]))]),
           sections([b"".join(registration(*r) for r in regs)] + terms.sections()), stream=25)

# 527 CloseRequest: partition 7 asks to close its open epoch 30.
golden("close_request", 527, command_header([(0, le(7, 4)), (8, le(30, 8))]), sections([]), stream=27, flags=1)

# 528 Closed: partitions 2, 4 and 9 (partition u32, 4 zero bytes, epoch u64); the reply names their latest closed
# epochs 28, none and 31.
for name, epochs in [("closed", (0, 0, 0)), ("closed_reply", (28, 0, 31))]:
    parts = b"".join(le(p, 4) + bytes(4) + le(e, 8) for p, e in zip((2, 4, 9), epochs))
    golden(name, 528, command_header([]), sections([parts]), stream=29)

# 529 DefaultEntry: the predicate <age> of tenant 3; the reply found the record (found 1).
terms = Terms()
terms.add(iri(f"{EX}age"))
for name, found in [("default_entry", 0), ("default_entry_reply", 1)]:
    golden(name, 529, command_header([(0, le(3, 8)), (8, bytes([found]))]), sections(terms.sections()), stream=31)

# 530 Report: partition 6 of tenant 4. Catalog version 12's change of <age> refused by the literal "abc" of
# xsd:integer; version 13's check passed; version 14's build of <embedding> failed.
terms = Terms()
terms.add(iri(f"{EX}age"))
terms.add(lit("abc", f"{XSD}integer"))
golden("report_refused", 530, command_header([(0, le(6, 4)), (4, bytes([2])), (8, le(4, 8)), (16, le(12, 8))]),
       sections(terms.sections()), stream=33)
golden("report_passed", 530, command_header([(0, le(6, 4)), (4, bytes([1])), (8, le(4, 8)), (16, le(13, 8))]),
       sections([b"", b""]), stream=35)
terms = Terms()
terms.add(iri(f"{EX}embedding"))
golden("report_build_failed", 530, command_header([(0, le(6, 4)), (4, bytes([3])), (8, le(4, 8)), (16, le(14, 8))]),
       sections(terms.sections()), stream=37)

# --- end 525 to 530 ---

out = Path(sys.argv[1])
for name, (data, size, checksum, checksum_body) in GOLDEN.items():
    (out / f"{name}.frame").write_bytes(data)
    print(f"{name:24} size {size:6}  checksum {checksum:#018x}  checksum_body {checksum_body:#018x}")
