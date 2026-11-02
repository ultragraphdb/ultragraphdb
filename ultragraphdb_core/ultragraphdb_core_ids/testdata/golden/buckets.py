#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The generator of buckets.json: golden buckets from the xxHash reference implementation, independent of the Rust code.
#
# The tests read it with ultragraphdb_tool_bdd::load_golden. Run, from a virtual environment holding python-xxhash (the
# Python binding of the xxHash C library):
#
#     python3 -m venv venv && venv/bin/pip install xxhash
#     venv/bin/python -I ultragraphdb_core/ultragraphdb_core_ids/testdata/golden/buckets.py \
#         > ultragraphdb_core/ultragraphdb_core_ids/testdata/golden/buckets.json
#
# The rule (system-identity, "The bucket comes from the namespace and the key"): the bucket's top 8 bits are the top 8
# bits of XXH3-64 (seed 0) of the namespace as 8 big-endian bytes; its low 16 bits the top 16 bits of XXH3-64 (seed 0)
# of the declaring type's prefix as 2 big-endian bytes followed by the key's canonical encoding, an IRI's length as a
# 4-byte big-endian count and its UTF-8 bytes. Every byte string is built here from that text alone.

import json
import sys

import xxhash

# (namespace, type prefix, IRI, why the case is here). The IRI lengths take the hashed input (6 + the IRI's bytes)
# through each of XXH3-64's length classes: 4-8, 9-16, 17-128, 129-240 and above 240 bytes, several 1,024-byte blocks
# among them.
CASES = [
    (0, 1, "http://ex.org/a", "the scenario: namespace 0, the Resource prefix"),
    (1, 1, "http://ex.org/a", "the scenario: namespace 1, the same key"),
    (2, 1, "http://ex.org/a", "namespace 2"),
    (0x0102030405060708, 1, "http://ex.org/a", "a namespace whose 8 bytes all differ"),
    (2**64 - 1, 1, "http://ex.org/a", "the largest namespace"),
    (0, 2, "http://ex.org/a", "the Statement prefix over the same key"),
    (0, 3, "http://ex.org/a", "the first declared prefix"),
    (0, 65535, "http://ex.org/a", "the largest prefix"),
    (0, 1, "", "the empty IRI: 6 hashed bytes"),
    (0, 1, "urn:x", "11 hashed bytes"),
    (0, 1, "http://ex.org/" + "b" * 136, "156 hashed bytes"),
    (0, 1, "http://ex.org/" + "c" * 286, "306 hashed bytes"),
    (0, 1, "http://ex.org/" + "d" * 4986, "5,006 hashed bytes, four full blocks and a part"),
    (7, 1, "http://ex.org/ü日本", "non-ASCII UTF-8"),
]


def bucket_case(namespace, prefix, iri, note):
    namespace_bytes = namespace.to_bytes(8, "big")
    iri_bytes = iri.encode("utf-8")
    key_bytes = prefix.to_bytes(2, "big") + len(iri_bytes).to_bytes(4, "big") + iri_bytes
    namespace_hash = xxhash.xxh3_64_intdigest(namespace_bytes, seed=0)
    key_hash = xxhash.xxh3_64_intdigest(key_bytes, seed=0)
    namespace_top8 = namespace_hash >> 56
    key_top16 = key_hash >> 48
    return {
        "note": note,
        "namespace": namespace,
        "prefix": prefix,
        "iri": iri,
        "namespace_xxh3": f"{namespace_hash:016x}",
        "key_xxh3": f"{key_hash:016x}",
        "bucket": (namespace_top8 << 16) | key_top16,
    }


def main():
    # The harness's golden format names a Dgraph commit; these values come from the xxHash reference, not Dgraph.
    golden = {
        "dgraph_commit": "none: the xxHash reference implementation, not Dgraph",
        "generator": "ultragraphdb_core/ultragraphdb_core_ids/testdata/golden/buckets.py with "
        + f"python-xxhash {xxhash.VERSION} over the xxHash C library {xxhash.XXHASH_VERSION}, "
        + f"Python {sys.version.split()[0]}",
        "cases": [bucket_case(*case) for case in CASES],
    }
    json.dump(golden, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")


main()
