#!/usr/bin/env python3
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# Writes the seeds of the fuzz target `catalog_contents_decode` into fuzz_seeds/catalog_contents_decode/, encoded from
# the formats' text alone (CatalogContents' codec and the catalog entry's format in catalog_codec), not by the crate:
# valid contents, whose names say what they hold, and malformed ones, named `malformed_*`. The unit tests decode every
# seed: a valid one re-encodes to its bytes, a malformed one is refused naming its version. Run with
# `python3 -I seeds.py` from this directory.

import os
import struct

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fuzz_seeds", "catalog_contents_decode")


def string(s):
    b = s.encode("utf-8")
    return struct.pack(">I", len(b)) + b


def strings(items):
    return struct.pack(">H", len(items)) + b"".join(string(s) for s in items)


def optional(s):
    return b"\x00" if s is None else b"\x01" + string(s)


def predicate(iri, kind=2, single=False, datatype=None, unique=False, tokenizers=(), count=False, domains=()):
    """A predicate's entry, format 2: no vector index."""
    return (bytes([2, 1]) + string(iri) + bytes([kind, 0 if single else 1]) + optional(datatype)
            + bytes([1 if unique else 0]) + strings(list(tokenizers)) + bytes([1 if count else 0, 0])
            + strings(list(domains)))


def entity_type(iri, parent=None, abstract=False):
    return bytes([2, 2]) + string(iri) + optional(parent) + bytes([1 if abstract else 0])


def relation(iri, roles):
    return bytes([2, 3]) + string(iri) + strings(roles)


def contents(tenant, version, entries, fmt=1):
    out = bytes([fmt]) + struct.pack(">QQI", tenant, version, len(entries))
    for e in entries:
        out += struct.pack(">I", len(e)) + e
    return out


EX = "http://ex.org/"
SEEDS = {
    "empty_tenant_0_version_1": contents(0, 1, []),
    "edge_knows_tenant_3_version_7": contents(3, 7, [predicate(EX + "knows", kind=0)]),
    "indexed_name_tenant_1_version_2": contents(1, 2, [predicate(EX + "name", kind=1, single=True,
                                                             datatype="http://www.w3.org/2001/XMLSchema#string",
                                                             unique=True, tokenizers=["exact", "term"], count=True,
                                                             domains=[EX + "Person"])]),
    "every_kind_tenant_0_version_9": contents(0, 9, [predicate(EX + "p"), entity_type(EX + "Employee", EX + "Person", True),
                                                 relation(EX + "marriage", [EX + "a", EX + "b"])]),
    "malformed_format_2": contents(0, 1, [], fmt=2),
    "malformed_truncated": contents(0, 1, [predicate(EX + "p")])[:-3],
    "malformed_trailing_byte": contents(0, 1, []) + b"\x00",
    "malformed_count_past_the_bytes": contents(0, 1, [])[:-4] + struct.pack(">I", 0xFFFFFFFF),
    "malformed_entry_of_no_bytes": bytes([1]) + struct.pack(">QQII", 0, 1, 1, 0),
    "malformed_unknown_entry_kind": contents(0, 1, [bytes([2, 9]) + string(EX + "p")]),
}

os.makedirs(OUT, exist_ok=True)
for name, data in SEEDS.items():
    with open(os.path.join(OUT, name), "wb") as f:
        f.write(data)
