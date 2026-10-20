// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"fmt"
	"math/rand"
	"sort"

	"github.com/dgraph-io/dgraph/v25/algo"
	"github.com/dgraph-io/dgraph/v25/codec"
	"github.com/dgraph-io/dgraph/v25/protos/pb"
)

// Golden vectors for ultragraphdb_core_algo (algo_setops): the results of Dgraph's sorted-set operations over plain
// and packed UID lists, including inputs with duplicates, where the strategies differ.

// opResult is one operation's result on a case's lists.
type opResult struct {
	Op  string   `json:"op"`
	Arg uint64   `json:"arg"`
	Out []uint64 `json:"out"`
	// OutNil is set when a packed operation returned a nil pack; Index is index-of's result, I and K the offsets
	// the pairwise intersections return; Panics is set when Dgraph's operation panicked.
	OutNil bool `json:"out_nil"`
	Index  int  `json:"index"`
	I      int  `json:"i"`
	K      int  `json:"k"`
	Panics bool `json:"panics"`
}

type algoCase struct {
	Name string `json:"name"`
	// BlockSize packs the inputs of the packed operations; an empty input packs to nil, as Dgraph's Encoder does.
	BlockSize int        `json:"block_size"`
	Lists     [][]uint64 `json:"lists"`
	Results   []opResult `json:"results"`
}

// run records op's result, or that it panicked.
func run(c *algoCase, op string, arg uint64, f func(r *opResult)) {
	r := opResult{Op: op, Arg: arg, Out: []uint64{}}
	func() {
		defer func() {
			if recover() != nil {
				r = opResult{Op: op, Arg: arg, Out: []uint64{}, Panics: true}
			}
		}()
		f(&r)
	}()
	if r.Out == nil {
		r.Out = []uint64{}
	}
	c.Results = append(c.Results, r)
}

func sortedList(r *rand.Rand, n int, limit int64, dups bool) []uint64 {
	out := make([]uint64, 0, n)
	for i := 0; i < n; i++ {
		out = append(out, uint64(r.Int63n(limit)))
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	if dups {
		return out
	}
	uniq := out[:0]
	for i, u := range out {
		if i == 0 || u != out[i-1] {
			uniq = append(uniq, u)
		}
	}
	return uniq
}

func pack(uids []uint64, blockSize int) *pb.UidPack {
	enc := codec.Encoder{BlockSize: blockSize}
	for _, u := range uids {
		enc.Add(u)
	}
	return enc.Done()
}

func packs(lists [][]uint64, blockSize int) []*pb.UidPack {
	out := []*pb.UidPack{}
	for _, l := range lists {
		out = append(out, pack(l, blockSize))
	}
	return out
}

func plain(lists [][]uint64) []*pb.List {
	out := []*pb.List{}
	for _, l := range lists {
		out = append(out, &pb.List{Uids: append([]uint64{}, l...)})
	}
	return out
}

func setPacked(r *opResult, p *pb.UidPack) {
	if p == nil {
		r.OutNil = true
		return
	}
	r.Out = codec.Decode(p, 0)
}

func pairCase(name string, u, v []uint64, blockSize int) algoCase {
	c := algoCase{Name: name, BlockSize: blockSize, Lists: [][]uint64{u, v}}
	run(&c, "intersect_with", 0, func(r *opResult) {
		o := &pb.List{}
		algo.IntersectWith(&pb.List{Uids: u}, &pb.List{Uids: v}, o)
		r.Out = o.Uids
	})
	run(&c, "intersect_with_lin", 0, func(r *opResult) { r.I, r.K = algo.IntersectWithLin(u, v, &r.Out) })
	run(&c, "intersect_with_jump", 0, func(r *opResult) { r.I, r.K = algo.IntersectWithJump(u, v, &r.Out) })
	run(&c, "intersect_with_bin", 0, func(r *opResult) { r.K = algo.IntersectWithBin(u, v, &r.Out) })
	run(&c, "difference", 0, func(r *opResult) { r.Out = algo.Difference(&pb.List{Uids: u}, &pb.List{Uids: v}).Uids })
	after := []uint64{0, 1}
	if len(u) > 0 {
		after = append(after, u[len(u)/2], u[len(u)-1], u[len(u)-1]+1)
	}
	for _, a := range after {
		run(&c, "intersect_compressed_with", a, func(r *opResult) {
			o := &pb.List{}
			algo.IntersectCompressedWith(pack(u, blockSize), a, &pb.List{Uids: v}, o)
			r.Out = o.Uids
		})
	}
	if len(u) > 0 {
		run(&c, "intersect_compressed_with_lin_jump", 0, func(r *opResult) {
			dec := codec.Decoder{Pack: pack(u, blockSize)}
			dec.Seek(0, codec.SeekStart)
			algo.IntersectCompressedWithLinJump(&dec, v, &r.Out)
		})
		run(&c, "intersect_compressed_with_bin", 0, func(r *opResult) {
			dec := codec.Decoder{Pack: pack(u, blockSize)}
			dec.Seek(0, codec.SeekStart)
			algo.IntersectCompressedWithBin(&dec, v, &r.Out)
		})
	}
	run(&c, "intersect_with_lin_packed", 0, func(r *opResult) {
		setPacked(r, algo.IntersectWithLinPacked(pack(u, blockSize), pack(v, blockSize)))
	})
	run(&c, "difference_packed", 0, func(r *opResult) {
		setPacked(r, algo.DifferencePacked(pack(u, blockSize), pack(v, blockSize)))
	})
	return c
}

func listsCase(name string, lists [][]uint64, blockSize int) algoCase {
	c := algoCase{Name: name, BlockSize: blockSize, Lists: lists}
	run(&c, "merge_sorted", 0, func(r *opResult) { r.Out = algo.MergeSorted(plain(lists)).Uids })
	run(&c, "merge_sorted_more_mem", 0, func(r *opResult) { r.Out = algo.MergeSortedMoreMem(plain(lists)).Uids })
	if len(lists) > 0 {
		run(&c, "intersect_sorted", 0, func(r *opResult) { r.Out = algo.IntersectSorted(plain(lists)).Uids })
	}
	run(&c, "merge_sorted_packed", 0, func(r *opResult) { setPacked(r, algo.MergeSortedPacked(packs(lists, blockSize))) })
	run(&c, "intersect_sorted_packed", 0, func(r *opResult) {
		setPacked(r, algo.IntersectSortedPacked(packs(lists, blockSize)))
	})
	return c
}

func indexCase(name string, list []uint64, blockSize int, targets []uint64) algoCase {
	c := algoCase{Name: name, BlockSize: blockSize, Lists: [][]uint64{list}}
	for _, t := range targets {
		run(&c, "index_of", t, func(r *opResult) { r.Index = algo.IndexOf(&pb.List{Uids: list}, t) })
		run(&c, "index_of_packed", t, func(r *opResult) { r.Index = algo.IndexOfPacked(pack(list, blockSize), t) })
	}
	return c
}

func genAlgoSetOps() (any, error) {
	r := rand.New(rand.NewSource(42))
	cases := []algoCase{}
	sizes := []int{0, 1, 3, 11, 100, 400}
	for _, n := range sizes {
		for _, m := range sizes {
			for _, dups := range []bool{false, true} {
				limit := int64(2*(n+m) + 1)
				u := sortedList(r, n, limit, dups)
				v := sortedList(r, m, limit, dups)
				cases = append(cases, pairCase(fmt.Sprintf("pair/n=%d/m=%d/dups=%v", n, m, dups), u, v, 10))
			}
		}
	}
	// Length ratios that pick each of IntersectWith's and IntersectCompressedWith's strategies.
	for _, m := range []int{150, 1200, 3000, 6000} {
		u := sortedList(r, 10, 8000, false)
		v := sortedList(r, m, 8000, false)
		cases = append(cases, pairCase(fmt.Sprintf("ratio/n=10/m=%d", m), u, v, 256))
		cases = append(cases, pairCase(fmt.Sprintf("ratio/n=%d/m=10", m), v, u, 256))
	}
	// Dgraph's DifferencePacked stops once v's blocks run out a block before u's do.
	cases = append(cases, pairCase("blocks/u-outlasts-v", []uint64{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15}, []uint64{1}, 5))
	// Lists across 2^32, which close blocks early.
	cases = append(cases, pairCase("upper32", []uint64{1, 5, 1<<32 + 1, 1<<32 + 5, 2<<32 + 7}, []uint64{5, 1<<32 + 5, 2<<32 + 7, 3 << 32}, 10))

	for _, k := range []int{0, 1, 2, 3, 5, 12, 13, 99, 100, 101, 150} {
		for _, dups := range []bool{false, true} {
			lists := [][]uint64{}
			for i := 0; i < k; i++ {
				lists = append(lists, sortedList(r, r.Intn(30), 200, dups))
			}
			cases = append(cases, listsCase(fmt.Sprintf("lists/k=%d/dups=%v", k, dups), lists, 5))
		}
	}
	// Dgraph's IntersectSortedPacked intersects only its two shortest lists.
	cases = append(cases, listsCase("lists/three", [][]uint64{{1, 2, 3, 4}, {2, 3, 4, 5, 6}, {1, 2, 3, 4, 5, 6, 7}, {2, 3}}, 5))
	cases = append(cases, listsCase("lists/shortest-two-share", [][]uint64{{1, 2, 3, 9}, {2, 3, 9, 11, 12}, {7, 8, 9, 10, 11, 12}}, 5))

	list := sortedList(r, 300, 1000, false)
	targets := []uint64{0, 1, 999, 1000, 1 << 40}
	for i := 0; i < len(list); i += 13 {
		targets = append(targets, list[i], list[i]+1)
	}
	cases = append(cases, indexCase("index/300", list, 10, targets))
	cases = append(cases, indexCase("index/empty", []uint64{}, 10, []uint64{0, 5}))
	return cases, nil
}

func init() {
	register("algo_setops", "ultragraphdb_core_algo", genAlgoSetOps)
}
