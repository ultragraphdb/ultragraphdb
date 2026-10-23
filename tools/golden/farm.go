// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"encoding/hex"
	"fmt"
	"math/rand"

	"github.com/dgryski/go-farm"
)

// Golden vectors for ultragraphdb_common_utils' go_farm (farm_fingerprint64): go-farm's Fingerprint64, the one
// go-farm function Dgraph calls, at the version Dgraph's go.mod pins. On arm64 Fingerprint64 is go-farm's generic
// naHash64; on amd64 it is assembly checked against the same function by go-farm's own tests.

type farmCase struct {
	Name string `json:"name"`
	// Input is the hashed bytes in hex.
	Input       string `json:"input"`
	Fingerprint uint64 `json:"fingerprint"`
}

func farmFingerprint(name string, in []byte) farmCase {
	return farmCase{Name: name, Input: hex.EncodeToString(in), Fingerprint: farm.Fingerprint64(in)}
}

func randomBytes(r *rand.Rand, n int) []byte {
	b := make([]byte, n)
	for i := range b {
		b[i] = byte(r.Intn(256))
	}
	return b
}

// farmEdgeLengths are the lengths at and beside naHash64's branches (0, 4, 8, 16, 32, 64) and its 64-byte loop,
// up to 64 KiB.
var farmEdgeLengths = []int{
	0, 1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 191, 192, 193,
	255, 256, 257, 1023, 1024, 1025, 4095, 4096, 4097, 65535, 65536,
}

// farmPatterns fill a buffer of length n: the zero byte, the all-ones byte, the high bit alone (which a sign
// extension would change), alternation of the two extremes, and every byte value ascending and descending.
var farmPatterns = []struct {
	name string
	fill func(i int) byte
}{
	{"zeros", func(int) byte { return 0x00 }},
	{"ones", func(int) byte { return 0xff }},
	{"high_bit", func(int) byte { return 0x80 }},
	{"alternating", func(i int) byte {
		if i%2 == 0 {
			return 0x00
		}
		return 0xff
	}},
	{"ascending", func(i int) byte { return byte(i) }},
	{"descending", func(i int) byte { return byte(255 - i) }},
}

func init() {
	register("farm_fingerprint64", "ultragraphdb_common_utils", func() (any, error) {
		r := rand.New(rand.NewSource(20261023))
		cases := []farmCase{}
		// Every length from 0 to 300, random bytes.
		for n := 0; n <= 300; n++ {
			cases = append(cases, farmFingerprint(fmt.Sprintf("random_len_%d", n), randomBytes(r, n)))
		}
		// Random lengths up to 64 KiB, random bytes.
		for i := 0; i < 24; i++ {
			n := r.Intn(64<<10 + 1)
			cases = append(cases, farmFingerprint(fmt.Sprintf("random_long_%d_len_%d", i, n), randomBytes(r, n)))
		}
		// Edge byte patterns at the branch lengths.
		for _, p := range farmPatterns {
			for _, n := range farmEdgeLengths {
				b := make([]byte, n)
				for i := range b {
					b[i] = p.fill(i)
				}
				cases = append(cases, farmFingerprint(fmt.Sprintf("%s_len_%d", p.name, n), b))
			}
		}
		// The inputs Dgraph hashes: a language tag, a value, a key; a tag of Go's []byte(string) over non-ASCII text.
		for _, s := range []string{"en", "de", "zh-Hans", "hello", "été", "\x00\x00\x00name\x00"} {
			cases = append(cases, farmFingerprint(fmt.Sprintf("text_%q", s), []byte(s)))
		}
		return cases, nil
	})
}
