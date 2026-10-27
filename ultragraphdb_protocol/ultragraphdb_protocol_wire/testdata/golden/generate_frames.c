/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 *
 * The golden frames of ultragraphdb_protocol_wire, written by an implementation independent of the crate: the header
 * laid out field by field from the offsets of the wire-protocol spec ("One header for every frame"), the body split
 * into chunks as "Chunked body checksums" states it, and every checksum computed by the xxHash reference
 * implementation, xxhash.h of https://github.com/Cyan4973/xxHash at tag v0.8.3 (commit
 * e626a72bc2321cd320e953a0ccf1584cad60f363, blob 78fc2e8dbf6db809650fbb59762c5566a71677ca).
 *
 * Regenerate from this directory:
 *
 *   gh api repos/Cyan4973/xxHash/git/blobs/78fc2e8dbf6db809650fbb59762c5566a71677ca --jq .content \
 *       | base64 -d > /tmp/xxhash.h
 *   cc -O2 -I/tmp -o /tmp/generate_frames generate_frames.c && /tmp/generate_frames .
 *
 * It writes one <name>.frame per frame below and prints each frame's size, checksums and leaves, the values the
 * tests assert. Body bytes come from the generator of xxHash's tests/sanity_test.c (fillTestBuffer), continued
 * across frames, so no two chunks are equal.
 */

#define XXH_INLINE_ALL
#include "xxhash.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define HEADER 256
#define FIRST_CHUNK 3840
#define CHUNK 4096

static uint64_t byte_gen = 2654435761U; /* PRIME32, as fillTestBuffer starts */

static void fill(uint8_t *out, size_t len) {
    for (size_t i = 0; i < len; i++) {
        out[i] = (uint8_t)(byte_gen >> 56);
        byte_gen *= 11400714785074694797ULL; /* PRIME64 */
    }
}

static void put(uint8_t *out, size_t at, uint64_t value, size_t width) {
    for (size_t i = 0; i < width; i++) {
        out[at + i] = (uint8_t)(value >> (8 * i));
    }
}

/* The chunk count of `data_len` bytes: one up to 3,840, then one more per 4,096 or part of it. */
static size_t chunks_of(size_t data_len) {
    if (data_len <= FIRST_CHUNK) {
        return 1;
    }
    return 1 + (data_len - FIRST_CHUNK + CHUNK - 1) / CHUNK;
}

typedef struct {
    const char *name;
    uint64_t cluster;
    uint32_t view;
    uint32_t release;
    uint16_t protocol;
    uint16_t command;
    uint64_t stream;
    uint8_t replica;
    uint8_t flags;
    uint8_t command_header_xor;
    size_t data_len;
} spec_t;

static void write_frame(const char *dir, const spec_t *s) {
    size_t chunks = chunks_of(s->data_len);
    size_t table = chunks > 1 ? 8 * chunks : 0;
    size_t size = HEADER + s->data_len + table;
    uint8_t *frame = calloc(size, 1);
    uint8_t *data = frame + HEADER;
    fill(data, s->data_len);

    uint64_t checksum_body;
    printf("%s: size %zu, data %zu, chunks %zu\n", s->name, size, s->data_len, chunks);
    if (chunks == 1) {
        checksum_body = XXH3_64bits_withSeed(data, s->data_len, 0);
    } else {
        for (size_t i = 0; i < chunks; i++) {
            size_t start = i == 0 ? 0 : FIRST_CHUNK + CHUNK * (i - 1);
            size_t end = i == 0 ? FIRST_CHUNK : start + CHUNK;
            if (end > s->data_len) {
                end = s->data_len;
            }
            uint64_t leaf = XXH3_64bits_withSeed(data + start, end - start, (XXH64_hash_t)i);
            put(data + s->data_len, 8 * i, leaf, 8);
            printf("  leaf %zu: bytes %zu..%zu = 0x%016llX\n", i, start, end, (unsigned long long)leaf);
        }
        checksum_body = XXH3_64bits_withSeed(data + s->data_len, table, 0);
    }

    put(frame, 8, checksum_body, 8);
    /* nonce_reserved at 16..32 stays zero */
    put(frame, 32, s->cluster, 8);
    put(frame, 40, size, 4);
    put(frame, 44, s->view, 4);
    put(frame, 48, s->release, 4);
    put(frame, 52, s->protocol, 2);
    put(frame, 54, s->command, 2);
    put(frame, 56, s->stream, 8);
    put(frame, 64, s->replica, 1);
    put(frame, 65, s->flags, 1);
    /* reserved at 66..128 stays zero */
    for (size_t i = 0; i < 128; i++) {
        frame[128 + i] = (uint8_t)((i * 7 + 3) ^ s->command_header_xor);
    }
    uint64_t checksum = XXH3_64bits_withSeed(frame + 8, HEADER - 8, 0);
    put(frame, 0, checksum, 8);
    printf("  checksum 0x%016llX, checksum_body 0x%016llX\n", (unsigned long long)checksum,
           (unsigned long long)checksum_body);

    char path[4096];
    snprintf(path, sizeof path, "%s/%s.frame", dir, s->name);
    FILE *f = fopen(path, "wb");
    if (f == NULL || fwrite(frame, 1, size, f) != size || fclose(f) != 0) {
        perror(path);
        exit(1);
    }
    free(frame);
}

int main(int argc, char **argv) {
    const char *dir = argc > 1 ? argv[1] : ".";
    /* Hello on the connection: view, stream, replica and flags zero, as a client sends it. */
    const spec_t hello = {"hello", 0x0123456789ABCDEFULL, 0, 0x00050000, 1, 1, 0, 0, 0, 0x00, 100};
    /* Read on a stream, every field distinct and nonzero, a body of six chunks and a 48-byte table (20 KiB). */
    const spec_t read = {"read", 0x0123456789ABCDEFULL, 0x0A0B0C0D, 0x01020304, 0x0302, 7, 0x1122334455667788ULL,
                         0x5A, 0xA5, 0xFF, 20432};
    /* Credit with an empty body: one empty chunk, no table. */
    const spec_t credit = {"credit", 0x0123456789ABCDEFULL, 0, 0x00050000, 1, 14, 9, 0, 1, 0x11, 0};
    /* Result whose body fills the first chunk exactly: no table. */
    const spec_t one_chunk = {"one_chunk", 0x0123456789ABCDEFULL, 0, 0x00050000, 1, 13, 3, 0, 0, 0x22, 3840};
    /* Result one byte past the first chunk: two chunks, the second of one byte, and a 16-byte table. */
    const spec_t two_chunks = {"two_chunks", 0x0123456789ABCDEFULL, 0, 0x00050000, 1, 13, 4, 0, 0, 0x33, 3841};
    /* Round of two full chunks, ending the frame's second block, then its 16-byte table. */
    const spec_t round = {"round", 0x0123456789ABCDEFULL, 0, 0x00050000, 1, 17, 0, 0, 0, 0x44, 7936};
    write_frame(dir, &hello);
    write_frame(dir, &read);
    write_frame(dir, &credit);
    write_frame(dir, &one_chunk);
    write_frame(dir, &two_chunks);
    write_frame(dir, &round);
    return 0;
}
