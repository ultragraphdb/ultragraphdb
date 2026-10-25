// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for the order Go 1.27's sort.Sort leaves equal keys in, which Dgraph's results take: sort.Sort is
// pattern-defeating quicksort and not stable, so rows with equal sort keys end where its partitions, insertion sorts
// and heapSort fallback put them. Each case holds its keys and the permutation, the input index at each output
// position, on tie-heavy and patterned inputs at sizes around pdqsort's thresholds (maxInsertion 12,
// shortestNinther 50), and on inputs that reach the heapSort fallback: the heap tables below, in every column, and
// the descending int and float columns with nil values at 257 rows, and eight of their eighteen inputs at 100 rows,
// where Dgraph's Less holds both ways for two nils and its partitions come out unbalanced.
//
// Two layers: "sort" is sort.Sort over (key, index) pairs ordered by key alone; "types" is Dgraph's types.Sort over
// one-column rows whose uids are the input indexes, as int (nil values included), float (half the key) or string
// (the key's decimal text, compared as bytes), ascending or descending.

import (
	"fmt"
	"math/rand"
	"sort"
	"strconv"
	"strings"

	"github.com/dgraph-io/dgraph/v25/types"
)

func init() {
	register("types_sort_order", "ultragraphdb_core_types", genTypesSortOrder)
}

type sortOrderCase struct {
	Name      string   `json:"name"`
	Layer     string   `json:"layer"`
	Tid       string   `json:"tid,omitempty"`
	Desc      bool     `json:"desc,omitempty"`
	Keys      []*int64 `json:"keys"`                 // null: a nil value
	HeapTable bool     `json:"heap_table,omitempty"` // the keys are one of sortOrderHeapTables
	Perm      []int    `json:"perm"`
}

// sortOrderHeapTables reach pdqsort's heapSort fallback in Go 1.27: they were found by sorting random inputs with a
// copy of Go 1.27's sort/zsortinterface.go that counts heapSort calls, identical to it otherwise, and its order
// matched sort.Sort's on every one.
const sortOrderHeapTables = `0 2 2 0 0 2 1 3 2 1 0 3 0 1 0 3 2 1 3 1 2 1 3 0 0 1 3 0 0 1 2 0 3 1 2 2 3 3 1 3 3 2 2 1 1 3 3 3
3 3 2 3 0 2 3 3 3 2 1 3 1 3 0 0 1 0 1 2 3 0 0 0 2 0 2 3 2 1 1 2 2 0 0 3 1 3 1 1 1 2 0 2 3 1 3
1 0 2 0 3 3 0 0 2 0 2 3 0 0 3 2 1 3 0 0 0 0 3 3 3 1 2 2 1 2 1 2 3 0 1 1 0 0 0 0 3 3 1 0 1 0 2 3 3 1 3 1
1 3 2 0 1 3 1 2 1 0 2 0 0 3 3 0 3 1 1 3 2 1 2 2 0 3 3 0 1 1 1 3 3 0 2 1 3 3 2 3 1 2 2 2 2 3
3 1 1 2 3 2 2 1 0 3 1 1 0 0 1 0 2 3 0 1 0 2 0 3 3 1 2 0 3 1 3 3 2 0 3 1 3 3 2 2 2 3 0 3 2
3 3 2 1 1 2 1 1 0 0 2 2 0 0 3 1 1 3 0 1 3 3 3 0 0 1 0 2 3 2 0 3 0 3 3 3 2 0 0 0 0 2 1 1 0 3 2 1 3 3
3 0 2 0 3 3 3 1 0 3 2 0 2 0 2 3 3 2 2 0 1 1 0 0 3 3 2 1 0 2 0 1 2 2 0 2 1 3 0 1 3 3 1 3 2 3 3
0 0 3 3 3 1 1 3 1 2 1 2 0 2 0 0 1 1 3 3 0 2 0 1 1 1 0 0 0 2 3 3 1 0 3 2 2 0 0 1 3 1 1 1 3 0 3 3 0 3 0 1
2 2 1 0 0 2 3 3 3 0 3 2 0 0 3 3 2 3 3 3 1 3 2 2 0 3 2 2 2 0 1 0 0 0 1 2 1 0 2 1 1 3 2 3 3 2 1 3 0
0 1 3 0 2 0 3 3 2 3 0 1 0 0 1 1 0 2 0 3 3 3 0 3 3 1 2 1 0 1 1 2 1 1 1 3 0 0 2 0 3 0 0 3 2 3 1 3 0
1 1 2 3 1 2 1 2 3 2 1 2 0 0 3 2 1 2 3 0 1 2 0 0 0 0 3 3 2 1 0 3 3 3 1 2 2 0 3 3 3 1 0 3 3 1 3 1 1 2
1 3 2 1 1 1 1 2 1 3 0 3 0 3 0 0 3 3 3 0 2 1 1 2 1 3 0 3 3 2 2 0 1 2 1 2 3 1 0 0 0 1 3 0 1 2 2 3 0 2 3 1
3 0 2 1 3 0 3 3 0 3 3 0 0 3 0 1 3 3 0 2 3 3 3 3 3 0 1 2 2 2 0 0 1 0 0 2 0 0 1 0 1 1 0 2 2 2 2 3
3 3 3 3 2 0 1 2 1 1 0 0 0 3 0 0 1 2 0 2 1 3 2 1 2 2 1 2 1 3 1 3 3 2 3 2 0 0 0 3 3 1 2 1 1 2 3 1 1 3 1
2 1 2 0 2 3 3 2 2 2 0 3 0 3 0 2 3 1 0 1 1 2 3 2 0 2 1 2 0 1 0 3 2 3 0 2 3 3 3 1 0 1 3 3 3 1 2 0
1 2 0 3 1 3 0 3 1 1 1 3 0 0 2 1 0 0 2 1 0 1 0 1 3 2 1 1 1 1 3 2 1 3 1 2 0 3 0 2 0 3 3 0 3 0 2 2 1 3 3 3
1 1 2 3 2 3 0 0 0 3 1 3 0 3 3 2 3 2 2 2 1 1 1 1 1 1 3 1 0 2 0 1 1 1 0 3 0 1 0 3 3 3 2 3 3 3 1 0
3 0 2 3 0 2 3 3 1 0 3 2 0 0 2 3 3 1 3 3 1 1 1 0 0 3 3 1 2 3 3 3 2 1 1 1 0 0 2 2 0 2 3 3 2 3 3 0 3 0`

// sortOrderPatterns name the inputs, each a function of its size; the random ones draw from r.
var sortOrderPatterns = []string{"equal", "two", "three", "sorted", "reverse", "organ_pipe", "sawtooth", "dups",
	"nearly_sorted"}

func sortOrderKeys(r *rand.Rand, pattern string, n int) []int64 {
	k := make([]int64, n)
	for i := range k {
		switch pattern {
		case "equal":
			k[i] = 0
		case "two":
			k[i] = int64(r.Intn(2))
		case "three":
			k[i] = int64(r.Intn(3))
		case "sorted", "nearly_sorted":
			k[i] = int64(i / 3)
		case "reverse":
			k[i] = int64((n - i) / 3)
		case "organ_pipe":
			k[i] = int64(min(i, n-1-i))
		case "sawtooth":
			k[i] = int64(i % 8)
		case "dups":
			k[i] = int64(r.Intn(max(1, n/8)))
		}
	}
	if pattern == "nearly_sorted" && n > 1 {
		for s := 0; s < 3; s++ {
			a, b := r.Intn(n), r.Intn(n)
			k[a], k[b] = k[b], k[a]
		}
	}
	return k
}

type sortOrderPair struct {
	key int64
	idx int
}

type sortOrderPairs []sortOrderPair

func (p sortOrderPairs) Len() int           { return len(p) }
func (p sortOrderPairs) Less(i, j int) bool { return p[i].key < p[j].key }
func (p sortOrderPairs) Swap(i, j int)      { p[i], p[j] = p[j], p[i] }

// sortOrderPlain sorts (key, index) pairs with sort.Sort.
func sortOrderPlain(name string, keys []int64, heap bool) sortOrderCase {
	p := make(sortOrderPairs, len(keys))
	c := sortOrderCase{Name: name, Layer: "sort", Keys: make([]*int64, len(keys)), HeapTable: heap,
		Perm: make([]int, len(keys))}
	for i, k := range keys {
		p[i] = sortOrderPair{k, i}
		c.Keys[i] = &k
	}
	sort.Sort(p)
	for i, x := range p {
		c.Perm[i] = x.idx
	}
	return c
}

// sortOrderTypes sorts one-column rows of tid with Dgraph's types.Sort; a nil key is a nil value.
func sortOrderTypes(name, tid string, desc bool, keys []*int64, heap bool) (sortOrderCase, error) {
	c := sortOrderCase{Name: name, Layer: "types", Tid: tid, Desc: desc, Keys: keys, HeapTable: heap,
		Perm: make([]int, len(keys))}
	rows := make([][]types.Val, len(keys))
	uids := make([]uint64, len(keys))
	for i, k := range keys {
		uids[i] = uint64(i)
		var v types.Val
		switch tid {
		case "int":
			v.Tid = types.IntID
			if k != nil {
				v.Value = *k
			}
		case "float":
			v.Tid = types.FloatID
			if k != nil {
				v.Value = float64(*k) / 2
			}
		case "string":
			v.Tid = types.StringID
			if k != nil {
				v.Value = strconv.FormatInt(*k, 10)
			}
		}
		rows[i] = []types.Val{v}
	}
	if err := types.Sort(rows, &uids, []bool{desc}, ""); err != nil {
		return c, fmt.Errorf("%s: %w", name, err)
	}
	for i, u := range uids {
		c.Perm[i] = int(u)
	}
	return c, nil
}

func genTypesSortOrder() (any, error) {
	r := rand.New(rand.NewSource(27))
	var cases []sortOrderCase
	// 4096 only for the tie-heaviest and the patterned inputs, which keeps the file a few hundred kilobytes.
	large := map[string]bool{"equal": true, "two": true, "organ_pipe": true, "sawtooth": true, "dups": true}
	for _, n := range []int{0, 1, 2, 12, 13, 49, 50, 51, 100, 257, 1000, 4096} {
		for _, p := range sortOrderPatterns {
			if n == 4096 && !large[p] {
				continue
			}
			cases = append(cases, sortOrderPlain(fmt.Sprintf("sort/%s/%d", p, n), sortOrderKeys(r, p, n), false))
		}
	}
	var heap [][]int64
	for _, line := range strings.Split(sortOrderHeapTables, "\n") {
		var keys []int64
		for _, f := range strings.Fields(line) {
			k, err := strconv.ParseInt(f, 10, 64)
			if err != nil {
				return nil, err
			}
			keys = append(keys, k)
		}
		heap = append(heap, keys)
	}
	for i, keys := range heap {
		cases = append(cases, sortOrderPlain(fmt.Sprintf("sort/heap%d/%d", i, len(keys)), keys, true))
	}
	// One column of int or float, a fifth of the values nil, or of strings; asc or desc.
	columns := []struct {
		tid  string
		desc bool
		nils bool
	}{{"int", false, true}, {"int", true, true}, {"string", false, false}, {"float", true, true}}
	for _, col := range columns {
		for _, n := range []int{0, 1, 2, 12, 13, 49, 50, 51, 100, 257} {
			for _, p := range sortOrderPatterns {
				keys := sortOrderKeys(r, p, n)
				ptrs := make([]*int64, n)
				for i := range keys {
					if !col.nils || r.Intn(5) != 0 {
						ptrs[i] = &keys[i]
					}
				}
				name := fmt.Sprintf("types/%s_%s/%s/%d", col.tid, sortOrderDir(col.desc), p, n)
				c, err := sortOrderTypes(name, col.tid, col.desc, ptrs, false)
				if err != nil {
					return nil, err
				}
				cases = append(cases, c)
			}
		}
	}
	// The heap tables through types.Sort: as ints ascending, negated descending (the same comparisons) and as
	// strings, whose one-digit keys compare as the ints do.
	for _, col := range []struct {
		tid    string
		desc   bool
		negate bool
	}{{"int", false, false}, {"int", true, true}, {"string", false, false}} {
		for i, keys := range heap {
			ptrs := make([]*int64, len(keys))
			for j, k := range keys {
				if col.negate {
					k = -k
				}
				ptrs[j] = &k
			}
			name := fmt.Sprintf("types/%s_%s/heap%d/%d", col.tid, sortOrderDir(col.desc), i, len(keys))
			c, err := sortOrderTypes(name, col.tid, col.desc, ptrs, true)
			if err != nil {
				return nil, err
			}
			cases = append(cases, c)
		}
	}
	return cases, nil
}

func sortOrderDir(desc bool) string {
	if desc {
		return "desc"
	}
	return "asc"
}
