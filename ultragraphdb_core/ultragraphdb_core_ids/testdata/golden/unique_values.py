#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The generator of unique_values.json: golden buckets of unique values from the xxHash reference implementation,
# independent of the Rust code.
#
# The tests read it with ultragraphdb_tool_bdd::load_golden. Run, from a virtual environment holding python-xxhash (the
# Python binding of the xxHash C library):
#
#     python3 -m venv venv && venv/bin/pip install xxhash
#     venv/bin/python -I ultragraphdb_core/ultragraphdb_core_ids/testdata/golden/unique_values.py \
#         > ultragraphdb_core/ultragraphdb_core_ids/testdata/golden/unique_values.json
#
# The rule (system-identity, "The bucket comes from the namespace and the key", scenario "A unique value's bucket is
# computed from its predicate and value"): the bucket's top 8 bits are the top 8 bits of XXH3-64 (seed 0) of the
# namespace as 8 big-endian bytes; its low 16 bits the top 16 bits of XXH3-64 (seed 0) of the predicate's 16 ID bytes
# followed by the value's term encoding. The ID's bytes are built here from its fields (type prefix in bits 127-112,
# sequence in 111-48, bucket in 47-24, 24 reserved zero bits, big-endian), and each term encoding from the term
# encoding's tags as key-encoding states them: a string of at most 15 bytes is the tag 0x20, its length byte and its
# UTF-8 bytes.

import json
import sys

import xxhash

SMALL_STRING = 0x20

# (namespace, predicate's type prefix, bucket and sequence, the string value, why the case is here).
CASES = [
    (0, 1, 0x123456, 5, "x", "the scenario: the value \"x\" of a unique predicate in namespace 0"),
    (1, 1, 0x123456, 5, "x", "namespace 1: the same low bits"),
    (0, 1, 0x123456, 6, "x", "another predicate: other low bits"),
    (0, 1, 0x123456, 5, "y", "another value"),
    (0, 1, 0x000000, 1, "", "the empty string, the smallest ID"),
]


def predicate_bytes(prefix, bucket, sequence):
    return prefix.to_bytes(2, "big") + sequence.to_bytes(8, "big") + bucket.to_bytes(3, "big") + bytes(3)


def small_string(text):
    encoded = text.encode("utf-8")
    assert len(encoded) <= 15
    return bytes([SMALL_STRING, len(encoded)]) + encoded


def value_case(namespace, prefix, bucket, sequence, text, note):
    predicate = predicate_bytes(prefix, bucket, sequence)
    value = small_string(text)
    namespace_hash = xxhash.xxh3_64_intdigest(namespace.to_bytes(8, "big"), seed=0)
    value_hash = xxhash.xxh3_64_intdigest(predicate + value, seed=0)
    return {
        "note": note,
        "namespace": namespace,
        "predicate_prefix": prefix,
        "predicate_bucket": bucket,
        "predicate_sequence": sequence,
        "value": value.hex(),
        "namespace_xxh3": f"{namespace_hash:016x}",
        "value_xxh3": f"{value_hash:016x}",
        "bucket": ((namespace_hash >> 56) << 16) | (value_hash >> 48),
    }


def main():
    # The harness's golden format names a Dgraph commit; these values come from the xxHash reference, not Dgraph.
    golden = {
        "dgraph_commit": "none: the xxHash reference implementation, not Dgraph",
        "generator": "ultragraphdb_core/ultragraphdb_core_ids/testdata/golden/unique_values.py with "
        + f"python-xxhash {xxhash.VERSION} over the xxHash C library {xxhash.XXHASH_VERSION}, "
        + f"Python {sys.version.split()[0]}",
        "cases": [value_case(*case) for case in CASES],
    }
    json.dump(golden, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")


main()
