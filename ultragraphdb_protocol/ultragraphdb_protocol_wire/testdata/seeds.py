#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The fuzz seeds of the bodies stage 6.6 added or changed (PROTOCOL.md sections 4.9, 6.5, 6.7, 7.3, 7.4, 8.1, 8.3 to 8.6,
# 9.1, 9.4, 10, 13.7 and 13.15), laid out byte by byte from the layout tables of PROTOCOL.md, independently of the Rust
# code, as golden/bodies/generate_bodies.py lays out the golden frames; the helpers are that script's. Checksums with
# Python's xxhash 3.6.0 (libxxhash 0.8.2); SHA-256 truncated to 16 bytes of a hashed text with hashlib.
#
#   python3 -I seeds.py
#
# writes, beside this script, fuzz_seeds/body/<name>.frame and fuzz_seeds/body_fields/<name>.frame for each frame, and
# fuzz_seeds/continuation/<name>.bin for each stream of continuation frames, back to back.

import hashlib
from pathlib import Path

import xxhash

CLUSTER = 0x0123_4567_89AB_CDEF
RELEASE = 0x0005_0000
ABSENT = 0xFFFF_FFFF
EX = "http://ex.org/"
SKOLEM = "https://w3id.org/ultragraphdb/ns/tenant/0/.well-known/genid/"
LAST = 1


def le(value, width, signed=False):
    return value.to_bytes(width, "little", signed=signed)


def align8(n):
    return (n + 7) // 8 * 8


def command_header(fields):
    out = bytearray(128)
    for offset, data in fields:
        out[offset:offset + len(data)] = data
    return bytes(out)


def sections(secs):
    """The body layout (section 4.1): the table of (offset, len) u32 pairs, each section at the end of the one before
    rounded up to 8, zero padding, nothing after the last."""
    pos = 8 * len(secs)
    table = []
    for s in secs:
        pos = align8(pos)
        table.append((pos, len(s)))
        pos += len(s)
    body = bytearray()
    for offset, length in table:
        body += le(offset, 4) + le(length, 4)
    for (offset, _), s in zip(table, secs):
        body += b"\0" * (offset - len(body))
        body += s
    return bytes(body)


class Strings:
    """A string list (section 4.3), interned in order of first use."""

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


class Terms:
    """A request term table of IRIs (section 4.4): 24-byte records (kind 1, direction 0, 6 zero bytes, then the two
    texts' (offset, len) pairs, the second empty) and the texts back to back; each IRI once, in order of first use."""

    def __init__(self):
        self.iris = []

    def iri(self, text):
        if text in self.iris:
            return self.iris.index(text)
        self.iris.append(text)
        return len(self.iris) - 1

    def sections(self):
        records, text = b"", b""
        for iri in self.iris:
            data = iri.encode("utf-8")
            records += bytes([1, 0]) + bytes(6) + le(len(text), 4) + le(len(data), 4)
            text += data
            records += le(len(text), 4) + le(0, 4)
        return [records, text]


def frame(command, ch, data, stream=0, flags=0, sequence=1):
    """A frame laid out from the header table (section 2.1), its body split into chunks of 3,840 then 4,096 bytes, each
    leaf XXH3-64 seeded with its index, the table appended when there is more than one chunk."""
    chunks = [data[:3840]] + [data[i:i + 4096] for i in range(3840, len(data), 4096)]
    leaves = [xxhash.xxh3_64_intdigest(c, seed=i) for i, c in enumerate(chunks)]
    if len(chunks) == 1:
        body, checksum_body = data, leaves[0]
    else:
        table = b"".join(le(leaf, 8) for leaf in leaves)
        body, checksum_body = data + table, xxhash.xxh3_64_intdigest(table)
    size = 256 + len(body)
    tail = (le(checksum_body, 8) + le(sequence, 16) + le(CLUSTER, 8) + le(size, 4) + le(0, 4) + le(RELEASE, 4)
            + le(1, 2) + le(command, 2) + le(stream, 8) + bytes([0, flags]) + bytes(62) + ch)
    assert len(tail) == 248
    return le(xxhash.xxh3_64_intdigest(tail), 8) + tail + body


def sha16(text):
    return hashlib.sha256(text.encode()).digest()[:16]


def sid(prefix, sequence, bucket):
    """A system ID's 16 big-endian bytes: type prefix, sequence, bucket, 3 reserved zero bytes (section 1)."""
    return prefix.to_bytes(2, "big") + sequence.to_bytes(8, "big") + bucket.to_bytes(3, "big") + bytes(3)


def token(partition, position, epoch):
    """A session token (section 4.8): partition u32, 4 zero bytes, position u64, epoch u64."""
    return le(partition, 4) + bytes(4) + le(position, 8) + le(epoch, 8)


def warnings(entries):
    """The warnings' three sections (section 4.9): 24 bytes each, `diagnostic` (number, language << 16), `source`,
    `offset`, `length`, `message` and 4 zero bytes; then the messages, interned in warning order."""
    messages, records = Strings(), b""
    for number, language, source, offset, length, message in entries:
        records += (le(number | language << 16, 4) + le(source, 4) + le(offset, 4) + le(length, 4)
                    + le(messages.intern(message), 4) + bytes(4))
    return [records] + messages.sections()


FRAMES, STREAMS = {}, {}
SESSION = bytes(range(0x11, 0x21))

# 24 Purge (section 8.5): tenant 3's request naming two instances; the reply, the purge epoch 44 and one removed.
terms = Terms()
terms.iri(f"{EX}p1")
terms.iri(f"{EX}p2")
FRAMES["purge"] = frame(24, command_header([(0, le(3, 8))]), sections(terms.sections()), stream=59)
FRAMES["purge_reply"] = frame(24, command_header([(0, le(3, 8)), (8, le(44, 8)), (16, le(1, 4))]),
                              sections(terms.sections()), stream=59, flags=LAST)

# 25 Migrate (section 8.6): staged 5 of tenant 3; tenant @0, staged @8, action @16, state @17, refusal @20, epoch @24.
# The request to start; a dry run's reply in state 4 (checked) at epoch 44 with its report; a cutover's reply in state 9
# (failed) at epoch 45, its check refused with UGS2201 and the refusal's message.
def migrate(action, state=0, refusal=0, epoch=0, message=b"", report=b"", flags=0):
    ch = command_header([(0, le(3, 8)), (8, le(5, 8)), (16, bytes([action, state])), (20, le(refusal, 4)),
                         (24, le(epoch, 8))])
    return frame(25, ch, sections([message, report]), stream=61, flags=flags)


FRAMES["migrate"] = migrate(1)
FRAMES["migrate_checked_reply"] = migrate(2, 4, epoch=44, report=b"renamed: Human -> Person", flags=LAST)
FRAMES["migrate_failed_reply"] = migrate(3, 9, 2201, 45, b"UGS2201 <http://ex.org/p1> breaks rule r", flags=LAST)

# 12 Describe (section 8.3): tenant 9's answer at read version 42 with no declaration and four entries by kind, then
# name: the schema crm (kind 1) and the type Person (kind 3), their bytes opaque; the source crm (19), its text; the
# retired name Human (20), renamed to Person at version 3 until 2027-01-15 (item 1, fate 1, current the string index of
# Person). The strings: the entries' names, then the current names, each once.
SOURCE = "namespace crm {\n  type Person {}\n}\n"
UNTIL = 1_800_000_000_000_000
strings = Strings()
entries = [(1, "crm", b"\x01\x00\x02\x00"), (3, "Person", b"\x03type"), (19, "crm", SOURCE.encode()),
           (20, "Human", None)]
names = [strings.intern(name) for _, name, _ in entries]
records, data = b"", b""
for (kind, _, payload), name in zip(entries, names):
    if payload is None:
        payload = bytes([1, 1, 0, 0]) + le(strings.intern("Person"), 4) + le(3, 8) + le(UNTIL, 8)
    records += bytes([kind, 0, 0, 0]) + le(name, 4) + le(len(data), 4) + le(len(payload), 4)
    data += payload
DESCRIBE_CH = command_header([(0, le(9, 8)), (8, le(42, 8))])
describe_sections = strings.sections() + [b"", b"", b"", b"", b"", records, data]
FRAMES["describe_entries"] = frame(12, DESCRIBE_CH, sections(describe_sections), stream=17, flags=LAST)


def cut(section, unit, parts):
    """`section` in `parts` pieces, each a whole number of `unit` bytes but the last's remainder."""
    units = len(section) // unit
    bounds = [unit * (units * i // parts) for i in range(parts)] + [len(section)]
    return [section[bounds[i]:bounds[i + 1]] for i in range(parts)]


# The same answer in three frames (Continuation): each section cut between whole units, u32s in sections 0, 2, 3, 4
# and 6, 16-byte entries in 5 and 7, any byte of the ASCII texts of 1 and 8; each frame with the first's command
# header, the last alone flagged LAST.
UNITS = [4, 1, 4, 4, 4, 16, 4, 16, 1]
pieces = [cut(section, unit, 3) for section, unit in zip(describe_sections, UNITS)]
STREAMS["describe_three_frames"] = b"".join(
    frame(12, DESCRIBE_CH, sections([piece[i] for piece in pieces]), stream=17, flags=LAST if i == 2 else 0,
          sequence=i + 1)
    for i in range(3))

# 518 Catalog (section 13.7): tenant 3's version 12, applying from epoch 44, read at position 1,240, 300 opaque bytes
# in three frames of 100 each, the last alone flagged LAST; the first of them alone, a frame the reply continues past.
CATALOG_CH = command_header([(0, le(3, 8)), (8, le(12, 8)), (16, le(44, 8)), (24, le(1_240, 8))])
version_bytes = bytes((i * 37 + 5) % 256 for i in range(300))
catalog_frames = [frame(518, CATALOG_CH, sections([version_bytes[100 * i:100 * (i + 1)]]), stream=13,
                        flags=LAST if i == 2 else 0, sequence=i + 1) for i in range(3)]
FRAMES["role_catalog_continues"] = catalog_frames[0]
STREAMS["catalog_three_frames"] = b"".join(catalog_frames)

# 23 Install (section 8.4): the reply to tenant 3's schema crm and its migration crm_to_v2, built on version 11:
# version 12 at epoch 44, staged 5, the report, the migration report, and one warning of UGS (language 4) in source 1,
# its bytes 10 to 15.
sources = Strings()
for word in ["crm", "namespace crm {}", "crm_to_v2", "migration crm_to_v2 {}"]:
    sources.push(word)
FRAMES["install_staged_reply"] = frame(
    23, command_header([(0, le(3, 8)), (8, le(12, 8)), (16, le(44, 8)), (24, le(5, 8))]),
    sections(sources.sections() + [b"coordination-free: 2", b"renamed: Human -> Person"]
             + warnings([(110, 4, 1, 10, 5, "UGS0110 2:11 Human is retired: use Person")])),
    stream=57, flags=LAST)

# 10 Prepare (section 8.1) of a UQL text (language 3) of tenant 4 with one parameter, who, a node (1); the reply:
# statement 2, a schema id, the column `name` a string (6), and one warning of UQL (language 3) at bytes 6 to 11.
UQL = "match Person p where p = $who return p.name"
for name, statement, schema, result_types, words, warned in [
        ("prepare_uql", 0, 0, b"", ["who"], []),
        ("prepare_uql_reply", 2, 0x5FC0_D2C8_2332_781C, bytes([6]), ["who", "name"],
         [(110, 3, 0, 6, 6, "UQL0110 1:7 Person is retired: use Agent")])]:
    names = Strings()
    for word in words:
        names.push(word)
    FRAMES[name] = frame(
        10, command_header([(0, le(4, 8)), (8, le(statement, 8)), (16, le(schema, 8)), (24, bytes([3]))]),
        sections([UQL.encode(), b"", b"", bytes([1]), result_types] + names.sections() + warnings(warned)),
        stream=47)

# 22 Query (section 7.4) of UQL (language 3) in the transaction of stream 5, limit 100, `who` bound to <a>.
terms = Terms()
query_names = Strings()
query_names.push("who")
FRAMES["query_uql"] = frame(
    22, command_header([(0, le(5, 8)), (8, bytes([3])), (12, le(100, 4))]),
    sections([UQL.encode()] + query_names.sections() + [u32s([terms.iri(f"{EX}a")])] + terms.sections() + [b""]),
    stream=55)

# 13 Result (section 9.1): a UQL query's first batch, one row of four projections holding the values of tags 0x41 (a
# bfloat16vector [1, 2]), 0x50 (1.5 m), 0x51 (precision 9, the second) and 0x30 of code 35 (a measure's lexical form),
# none with a text (section 10.3); its column names and one warning; flagged LAST.
encodings = [bytes([0x41, 2, 0, 0x80, 0x3F, 0x00, 0x40]),
             bytes([0x50]) + (0x3FF8_0000_0000_0000).to_bytes(8, "big") + bytes([1]) + b"m",
             bytes([0x51, 9]), bytes([0x30, 35, 5]) + b"1.5 m"]
offsets, pos = [], 0
for encoding in encodings:
    offsets.append(pos)
    pos += len(encoding)
columns = Strings()
for word in ["v", "m", "p", "k"]:
    columns.push(word)
FRAMES["result_query_terms"] = frame(
    13, command_header([(0, le(4, 4)), (4, le(1, 4)), (8, le(30, 8))]),
    sections([u32s([0, 1, 2, 3]), u32s(offsets), b"".join(encodings), u32s([ABSENT] * 8), b"", b"", b""]
             + columns.sections() + warnings([(110, 3, 0, 6, 6, "UQL0110 1:7 Person is retired: use Agent")])),
    stream=49, flags=LAST)
# A pattern's batch whose reifier names an edge-encoded instance of a declared relation by its handle IRI, tag 0x01
# (sections 9.1, 10.2): <a>, "Al", the instance's IRI; read at epoch 30 in partition 2.
HANDLE = f"{EX}Employment/0000002a-07"
strings = Strings()
FRAMES["result_reifier_iri"] = frame(
    13, command_header([(0, le(3, 4)), (4, le(1, 4)), (8, le(30, 8)), (16, le(2, 4))]),
    sections([u32s([0, 1, 2]), u32s([0, 17, 21]),
              bytes([0x01]) + sha16(f"{EX}a") + bytes([0x20, 2]) + b"Al" + bytes([0x01]) + sha16(HANDLE),
              u32s([strings.intern(f"{EX}a"), ABSENT, ABSENT, ABSENT, strings.intern(HANDLE), ABSENT])]
             + strings.sections() + [b"", b"", b"", b"", b"", b""]),
    stream=11, flags=LAST)


# 9 Operation (section 7.3): delete an instance (5) in the default graph; retract an instance (6) in graph <g>.
for name, kind, graph in [("operation_delete", 5, None), ("operation_retract", 6, f"{EX}g")]:
    terms = Terms()
    args = [ABSENT if graph is None else terms.iri(graph), terms.iri(f"{EX}x")]
    FRAMES[name] = frame(9, command_header([(0, bytes([kind]))]), sections([u32s(args)] + terms.sections()),
                         stream=5)


def operation_fields(kind):
    """An operation's 24 bytes (section 7.3): kind, 7 zero bytes, delta, properties and links, all 0 for kinds 5, 6."""
    return bytes([kind]) + bytes(23)


# 21 ConditionalWrite (section 6.7): through the session's request 11 in tenant 3 with the token of partition 2's
# commit at position 43 in epoch 31: no change, then a delete of <x> and a retraction of <y> in graph <g>.
terms = Terms()
args = [ABSENT, terms.iri(f"{EX}x"), terms.iri(f"{EX}g"), terms.iri(f"{EX}y")]
FRAMES["conditional_write_instances"] = frame(
    21, command_header([(0, SESSION), (16, le(11, 8)), (24, le(3, 8)), (32, token(2, 43, 31))]),
    sections([b"", u32s(args)] + terms.sections() + [b"", b"", operation_fields(5) + operation_fields(6)]),
    stream=23)

# 19 Drop (section 6.5): tenant 6's data (kind 3) under catalog version 7 through the session's request 9; everything
# in tenant 6 (kind 4) under version 7, the reply with commit version (31 << 22) | 3 and its token.
for name, kind, request, commit_version, tok, flags in [
        ("drop_tenant_data", 3, 9, 0, bytes(24), 0),
        ("drop_whole_tenant_reply", 4, 10, (31 << 22) | 3, token(2, 44, 31), LAST)]:
    FRAMES[name] = frame(
        19, command_header([(0, bytes([kind])), (8, le(6, 8)), (16, SESSION), (32, le(request, 8)),
                            (40, le(commit_version, 8)), (48, tok), (72, le(7, 8))]),
        sections([b"", b""]), stream=19, flags=flags)

# 16 Error (section 9.4): code 30 (EntityDeleted) and code 31 (Fenced), `detail` 0.
FRAMES["error_entity_deleted"] = frame(16, command_header([(0, le(30, 2))]),
                                       sections([b"<http://ex.org/x> was deleted", b"", b""]), stream=5, flags=LAST)
FRAMES["error_fenced"] = frame(16, command_header([(0, le(31, 2))]),
                               sections([b"tenant 3 is fenced", b"", b""]), stream=7, flags=LAST)


# 526 Reference (section 13.15): `reply` @0, `mode` @1, `tenant` @8; registrations of 48 bytes (partition, group, iri,
# outcome u16, status u8, a zero byte, generation, close, entity). A get-only batch (mode 1) of tenant 3: <o> for group
# 0x2A last seen at generation 4, <m> with no group, and two entities by ID, one for group 0x10. The reply: <o> live
# (2) at generation 5 and close 41, <m> absent (1), the first ID deleted (3), the second retracted (4). A get-or-create
# reply (mode 0) refusing an entity by its ID with 30, its tombstone held.
def registration(partition, group, iri_index, outcome, status, generation, close, entity):
    return (le(partition, 4) + le(group, 4) + le(iri_index, 4) + le(outcome, 2) + bytes([status, 0])
            + le(generation, 8) + le(close, 8) + entity)


terms = Terms()
o, m = terms.iri(f"{EX}o"), terms.iri(f"{EX}m")
LIVE, DELETED, RETRACTED = sid(1, 77, 0xA80), sid(1, 78, 0xA80), sid(1, 79, 0xA80)
request_regs = [(3, 0x2A, o, 0, 0, 4, 0, bytes(16)), (5, ABSENT, m, 0, 0, 0, 0, bytes(16)),
                (3, 0x10, ABSENT, 0, 0, 0, 0, DELETED), (3, ABSENT, ABSENT, 0, 0, 0, 0, RETRACTED)]
reply_regs = [(3, 0x2A, o, 0, 2, 5, 41, LIVE), (5, ABSENT, m, 0, 1, 0, 0, bytes(16)),
              (3, 0x10, ABSENT, 0, 3, 0, 0, DELETED), (3, ABSENT, ABSENT, 0, 4, 0, 0, RETRACTED)]
for name, reply, regs in [("role_reference_get_only", 0, request_regs),
                          ("role_reference_get_only_reply", 1, reply_regs)]:
    FRAMES[name] = frame(526, command_header([(0, bytes([reply, 1])), (8, le(3, 8))]),
                         sections([b"".join(registration(*r) for r in regs)] + terms.sections()), stream=25)
FRAMES["role_reference_entity_deleted"] = frame(
    526, command_header([(0, bytes([1, 0])), (8, le(3, 8))]),
    sections([registration(3, ABSENT, ABSENT, 30, 0, 0, 0, DELETED), b"", b""]), stream=25)

HERE = Path(__file__).resolve().parent / "fuzz_seeds"
for target in ["body", "body_fields"]:
    for name, data in FRAMES.items():
        (HERE / target / f"{name}.frame").write_bytes(data)
(HERE / "continuation").mkdir(exist_ok=True)
for name, data in STREAMS.items():
    (HERE / "continuation" / f"{name}.bin").write_bytes(data)
for name, data in {**FRAMES, **STREAMS}.items():
    print(f"{name:34} {len(data):6} bytes")
