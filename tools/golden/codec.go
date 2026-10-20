// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"encoding/hex"
	"fmt"
	"math"
	"math/rand"
	"sort"

	"github.com/dgraph-io/dgraph/v25/codec"
	"github.com/dgraph-io/dgraph/v25/protos/pb"
)

// Golden vectors for ultragraphdb_core_codec: UidPack encodings (codec_uidpack) and the decoder's seeks
// (codec_seek), generated from Dgraph's codec package.

type blockJSON struct {
	Base    uint64 `json:"base"`
	Deltas  string `json:"deltas"`
	NumUids uint32 `json:"num_uids"`
}

type packJSON struct {
	BlockSize uint32      `json:"block_size"`
	Blocks    []blockJSON `json:"blocks"`
}

// packToJSON returns nil for a nil pack: Dgraph's Encoder returns nil when no UID was added.
func packToJSON(p *pb.UidPack) *packJSON {
	if p == nil {
		return nil
	}
	out := &packJSON{BlockSize: p.BlockSize, Blocks: []blockJSON{}}
	for _, b := range p.Blocks {
		out.Blocks = append(out.Blocks, blockJSON{Base: b.Base, Deltas: hex.EncodeToString(b.Deltas), NumUids: b.NumUids})
	}
	return out
}

type codecPackCase struct {
	Name      string    `json:"name"`
	BlockSize int       `json:"block_size"`
	Uids      []uint64  `json:"uids"`
	Pack      *packJSON `json:"pack"`
}

func sortedUnique(uids []uint64) []uint64 {
	sort.Slice(uids, func(i, j int) bool { return uids[i] < uids[j] })
	out := uids[:0]
	for i, u := range uids {
		if i == 0 || u != uids[i-1] {
			out = append(out, u)
		}
	}
	return out
}

// denseUids follows codec_test.go's getUids: a start below 100, gaps below 33.
func denseUids(r *rand.Rand, n int) []uint64 {
	uids := []uint64{}
	if n == 0 {
		return uids
	}
	last := uint64(r.Intn(100))
	uids = append(uids, last)
	for i := 1; i < n; i++ {
		last += uint64(r.Intn(33))
		uids = append(uids, last)
	}
	return uids
}

// sparseUids draws gaps of every group-varint width (1 to 4 bytes) and stays below 2^32.
func sparseUids(r *rand.Rand, n int) []uint64 {
	uids := []uint64{}
	var last uint64
	widths := []int64{1 << 8, 1 << 16, 1 << 24, 1 << 26}
	for i := 0; i < n; i++ {
		last += 1 + uint64(r.Int63n(widths[r.Intn(len(widths))]))
		uids = append(uids, last)
	}
	return uids
}

// spanUids lies around 2^32, so the upper 32 bits change in the middle of the list.
func spanUids(r *rand.Rand, n int) []uint64 {
	uids := []uint64{}
	last := uint64(1<<32) - uint64(n/2)*3
	for i := 0; i < n; i++ {
		last += 1 + uint64(r.Intn(5))
		uids = append(uids, last)
	}
	return uids
}

// bigUids follows codec_test.go's TestEncoding: 50 small UIDs, then UIDs with high bits from bigInts.
func bigUids(r *rand.Rand, n int) []uint64 {
	bigInts := []uint64{0xf000000000000000, 0xf00f000000000000, 0x00f00f0000000000, 0x000f0f0000000000, 0x0f0f0f0f00000000}
	uids := make([]uint64, n)
	for i := 0; i < 50 && i < n; i++ {
		uids[i] = uint64(r.Uint32())
	}
	for i := 50; i < n; i++ {
		uids[i] = uint64(r.Uint32()) + bigInts[r.Intn(5)]
	}
	return sortedUnique(uids)
}

func stepUids(from, to, step uint64) []uint64 {
	uids := []uint64{}
	for u := from; u <= to; u += step {
		uids = append(uids, u)
	}
	return uids
}

func packCase(name string, blockSize int, uids []uint64) codecPackCase {
	in := append([]uint64{}, uids...)
	return codecPackCase{Name: name, BlockSize: blockSize, Uids: in, Pack: packToJSON(codec.Encode(uids, blockSize))}
}

func genCodecUidPack() (any, error) {
	r := rand.New(rand.NewSource(20261020))
	cases := []codecPackCase{}
	for n := 0; n <= 100; n++ {
		uids := denseUids(r, n)
		cases = append(cases, packCase(fmt.Sprintf("dense/len=%d", n), 256, uids))
		cases = append(cases, packCase(fmt.Sprintf("dense/len=%d/block=10", n), 10, uids))
	}
	for n := 0; n <= 100; n++ {
		cases = append(cases, packCase(fmt.Sprintf("sparse/len=%d", n), 256, sparseUids(r, n)))
	}
	for n := 0; n <= 100; n++ {
		cases = append(cases, packCase(fmt.Sprintf("span32/len=%d", n), 256, spanUids(r, n)))
	}
	for _, n := range []int{0, 1, 2, 3, 5, 13, 18, 100, 99, 98} {
		cases = append(cases, packCase(fmt.Sprintf("big/len=%d", n), 256, bigUids(r, n)))
	}
	for n := 0; n <= 20; n++ {
		uids := denseUids(r, n)
		for _, bs := range []int{128, 5, 1, 0} {
			cases = append(cases, packCase(fmt.Sprintf("dense/len=%d/block=%d", n, bs), bs, uids))
		}
	}
	fixed := []struct {
		name string
		uids []uint64
	}{
		{"one/7", []uint64{7}},
		{"one/0", []uint64{0}},
		{"one/max", []uint64{math.MaxUint64}},
		{"zero-and-max", []uint64{0, math.MaxUint64}},
		{"upper32/1,2,2^32+1", []uint64{1, 2, 1<<32 + 1}},
		{"upper32/around", stepUids(1<<32-3, 1<<32+3, 1)},
		{"upper32/every-boundary", []uint64{1<<32 - 1, 1 << 32, 2<<32 - 1, 2 << 32, 3 << 32, 3<<32 + 1}},
		{"max-deltas", []uint64{0, math.MaxUint32, 1<<32 + math.MaxUint32}},
		{"dense/0..1000", stepUids(0, 1000, 1)},
		{"step10/10..10000", stepUids(10, 10000, 10)},
		{"step10/0..10000", stepUids(0, 10000, 10)},
		{"step3/3..10000", stepUids(3, 10000, 3)},
		{"dense/random-5000", denseUids(r, 5000)},
		{"sparse/random-3000", sparseUids(r, 3000)},
		{"big/random-2000", bigUids(r, 2000)},
	}
	for _, f := range fixed {
		for _, bs := range []int{256, 10} {
			cases = append(cases, packCase(fmt.Sprintf("%s/block=%d", f.name, bs), bs, f.uids))
		}
	}
	return cases, nil
}

// seekStep is one call on a decoder and what it returned. A decode step keeps the first decodeHead UIDs and the
// length of the result, which is the input's suffix from the target on.
type seekStep struct {
	Op     string   `json:"op"`
	Target uint64   `json:"target"`
	Whence string   `json:"whence"`
	Uids   []uint64 `json:"uids"`
	Len    int      `json:"len"`
}

const decodeHead = 8

func fullStep(op string, t uint64, whence string, uids []uint64) seekStep {
	return seekStep{Op: op, Target: t, Whence: whence, Uids: copyUids(uids), Len: len(uids)}
}

func head(uids []uint64) []uint64 {
	if len(uids) > decodeHead {
		return copyUids(uids[:decodeHead])
	}
	return copyUids(uids)
}

type codecSeekCase struct {
	Name      string     `json:"name"`
	BlockSize int        `json:"block_size"`
	Uids      []uint64   `json:"uids"`
	Steps     []seekStep `json:"steps"`
}

// whences names codec's two seek positions; their type is unexported, so each call site switches on the name.
var whences = []string{"start", "current"}

func seek(d *codec.Decoder, t uint64, whence string) []uint64 {
	if whence == "start" {
		return d.Seek(t, codec.SeekStart)
	}
	return d.Seek(t, codec.SeekCurrent)
}

func seekToBlock(d *codec.Decoder, t uint64, whence string) []uint64 {
	if whence == "start" {
		return d.SeekToBlock(t, codec.SeekStart)
	}
	return d.SeekToBlock(t, codec.SeekCurrent)
}

func copyUids(uids []uint64) []uint64 {
	return append([]uint64{}, uids...)
}

// seekTargets are the list's block bases and their neighbours, a few inner UIDs and their neighbours, 0, the
// last UID and beyond it, and the given extras.
func seekTargets(pack *pb.UidPack, uids []uint64, extra ...uint64) []uint64 {
	set := map[uint64]bool{0: true, math.MaxUint64: true}
	add := func(u uint64) {
		set[u] = true
		if u > 0 {
			set[u-1] = true
		}
		if u < math.MaxUint64 {
			set[u+1] = true
		}
	}
	for _, b := range pack.Blocks {
		add(b.Base)
	}
	for i := 0; i < len(uids); i += 1 + len(uids)/7 {
		add(uids[i])
	}
	add(uids[len(uids)-1])
	for _, u := range extra {
		set[u] = true
	}
	out := []uint64{}
	for u := range set {
		out = append(out, u)
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

func seekCase(name string, blockSize int, uids []uint64, extra ...uint64) codecSeekCase {
	pack := codec.Encode(copyUids(uids), blockSize)
	targets := seekTargets(pack, uids, extra...)
	steps := []seekStep{}
	for _, t := range targets {
		decoded := codec.Decode(pack, t)
		steps = append(steps, seekStep{Op: "decode", Target: t, Uids: head(decoded), Len: len(decoded)})
		for _, w := range whences {
			dec := codec.Decoder{Pack: pack}
			steps = append(steps, fullStep("seek", t, w, seek(&dec, t, w)))
		}
	}
	// SeekToBlock and LinearSeek keep the decoder's position between calls: one decoder each, ascending targets.
	for _, w := range whences {
		dec := codec.NewDecoder(pack)
		for _, t := range targets {
			if t == math.MaxUint64 {
				continue
			}
			steps = append(steps, fullStep("seek_to_block", t, w, seekToBlock(dec, t, w)))
		}
	}
	dec := codec.NewDecoder(pack)
	for _, t := range targets {
		if t == math.MaxUint64 {
			// LinearSeek never returns for MaxUint64: the base past the last block reads as MaxUint64.
			continue
		}
		steps = append(steps, fullStep("linear_seek", t, "", dec.LinearSeek(t)))
	}
	return codecSeekCase{Name: name, BlockSize: blockSize, Uids: uids, Steps: steps}
}

func genCodecSeek() (any, error) {
	r := rand.New(rand.NewSource(7))
	cases := []codecSeekCase{
		seekCase("step10/10..10000/block=256", 256, stepUids(10, 10000, 10), 5005, 5010, 5120, 5130),
		seekCase("step10/0..2000/block=10", 10, stepUids(0, 2000, 10), 1101, 1999, 2001),
		seekCase("step3/3..1000/block=10", 10, stepUids(3, 1000, 3)),
		seekCase("span32/block=256", 256, spanUids(r, 300)),
		seekCase("big/block=10", 10, bigUids(r, 100)),
		seekCase("one/7/block=256", 256, []uint64{7}),
	}
	return cases, nil
}

func init() {
	register("codec_uidpack", "ultragraphdb_core_codec", genCodecUidPack)
	register("codec_seek", "ultragraphdb_core_codec", genCodecSeek)
}
