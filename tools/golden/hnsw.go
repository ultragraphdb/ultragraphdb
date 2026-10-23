// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

//go:debug randseednop=0

package main

// Golden vectors for ultragraphdb_store_hnsw (tok/hnsw):
//
//   - hnsw_kernels: the distance kernels of kernels_generic.go (dot product, squared euclidean, euclidean and cosine
//     similarity, float32 and float64) over random and edge vectors, as bits. The kernels are unexported and reached
//     with go:linkname. scripts/golden.sh builds with fused multiply-adds off, so the results are Go's on amd64,
//     which is what Rust computes.
//   - hnsw_matrix: encodeUint64MatrixUnsafe, the neighbour matrix's encoding (unexported, through go:linkname).
//   - hnsw_parse_edges: ParseEdges, the dead list's parser, its results and errors.
//   - hnsw_options: the levels, breadths and metric an index takes from its options (applyOptions), read from the
//     created index's unexported fields, and the errors it refuses options with.
//   - hnsw_graph: whole runs of Dgraph's index over an in-memory transaction (test_helper.go's store, made to read
//     and scan in key order): inserts with the global random source seeded, so that the levels drawn are known,
//     dead nodes, and searches with and without options; the store's contents after the inserts, every insert's
//     edges, and every search's result.

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"math/rand"
	"reflect"
	"sort"
	_ "unsafe" // go:linkname

	"github.com/dgraph-io/dgraph/v25/tok/hnsw"
	"github.com/dgraph-io/dgraph/v25/tok/index"
	opt "github.com/dgraph-io/dgraph/v25/tok/options"
	"github.com/dgraph-io/dgraph/v25/x"
)

//go:linkname hnswDotF32 github.com/dgraph-io/dgraph/v25/tok/hnsw.dotF32
func hnswDotF32(a, b []float32) float32

//go:linkname hnswDotF64 github.com/dgraph-io/dgraph/v25/tok/hnsw.dotF64
func hnswDotF64(a, b []float64) float64

//go:linkname hnswEuclideanSqF32 github.com/dgraph-io/dgraph/v25/tok/hnsw.euclideanSqF32
func hnswEuclideanSqF32(a, b []float32) float32

//go:linkname hnswEuclideanSqF64 github.com/dgraph-io/dgraph/v25/tok/hnsw.euclideanSqF64
func hnswEuclideanSqF64(a, b []float64) float64

//go:linkname hnswEuclideanF32 github.com/dgraph-io/dgraph/v25/tok/hnsw.euclideanF32
func hnswEuclideanF32(a, b []float32) float32

//go:linkname hnswEuclideanF64 github.com/dgraph-io/dgraph/v25/tok/hnsw.euclideanF64
func hnswEuclideanF64(a, b []float64) float64

//go:linkname hnswCosineSimF32 github.com/dgraph-io/dgraph/v25/tok/hnsw.cosineSimF32
func hnswCosineSimF32(a, b []float32) float32

//go:linkname hnswCosineSimF64 github.com/dgraph-io/dgraph/v25/tok/hnsw.cosineSimF64
func hnswCosineSimF64(a, b []float64) float64

//go:linkname hnswEncodeMatrix github.com/dgraph-io/dgraph/v25/tok/hnsw.encodeUint64MatrixUnsafe
func hnswEncodeMatrix(matrix [][]uint64) []byte

func init() {
	register("hnsw_kernels", "ultragraphdb_store_hnsw", genHnswKernels)
	register("hnsw_matrix", "ultragraphdb_store_hnsw", genHnswMatrix)
	register("hnsw_parse_edges", "ultragraphdb_store_hnsw", genHnswParseEdges)
	register("hnsw_options", "ultragraphdb_store_hnsw", genHnswOptions)
	register("hnsw_graph", "ultragraphdb_store_hnsw", genHnswGraph)
}

// hnswKernelCase is one pair of vectors and every kernel's result, all as IEEE bits: float32 bits for the 32-bit
// kernels, float64 bits for the 64-bit ones.
type hnswKernelCase struct {
	Name        string   `json:"name"`
	Bits        int      `json:"bits"`
	A           []uint64 `json:"a"`
	B           []uint64 `json:"b"`
	Dot         uint64   `json:"dot"`
	EuclideanSq uint64   `json:"euclidean_sq"`
	Euclidean   uint64   `json:"euclidean"`
	Cosine      uint64   `json:"cosine"`
}

// hnswKernelDims are kernels_test.go's dimensions: the unrolls' tails and the vector widths.
var hnswKernelDims = []int{0, 1, 2, 3, 4, 5, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 128, 384, 768, 1000, 1536}

func bits32(v []float32) []uint64 {
	out := make([]uint64, len(v))
	for i, f := range v {
		out[i] = uint64(math.Float32bits(f))
	}
	return out
}

func bits64(v []float64) []uint64 {
	out := make([]uint64, len(v))
	for i, f := range v {
		out[i] = math.Float64bits(f)
	}
	return out
}

func kernelCase32(name string, a, b []float32) hnswKernelCase {
	return hnswKernelCase{
		Name: name, Bits: 32, A: bits32(a), B: bits32(b),
		Dot:         uint64(math.Float32bits(hnswDotF32(a, b))),
		EuclideanSq: uint64(math.Float32bits(hnswEuclideanSqF32(a, b))),
		Euclidean:   uint64(math.Float32bits(hnswEuclideanF32(a, b))),
		Cosine:      uint64(math.Float32bits(hnswCosineSimF32(a, b))),
	}
}

func kernelCase64(name string, a, b []float64) hnswKernelCase {
	return hnswKernelCase{
		Name: name, Bits: 64, A: bits64(a), B: bits64(b),
		Dot:         math.Float64bits(hnswDotF64(a, b)),
		EuclideanSq: math.Float64bits(hnswEuclideanSqF64(a, b)),
		Euclidean:   math.Float64bits(hnswEuclideanF64(a, b)),
		Cosine:      math.Float64bits(hnswCosineSimF64(a, b)),
	}
}

func genHnswKernels() (any, error) {
	r := rand.New(rand.NewSource(20261023))
	cases := []hnswKernelCase{}
	for _, n := range hnswKernelDims {
		for rep := 0; rep < 3; rep++ {
			a32, b32 := make([]float32, n), make([]float32, n)
			a64, b64 := make([]float64, n), make([]float64, n)
			for i := 0; i < n; i++ {
				a32[i], b32[i] = r.Float32()*2-1, r.Float32()*2-1
				a64[i], b64[i] = r.Float64()*2-1, r.Float64()*2-1
			}
			cases = append(cases, kernelCase32(fmt.Sprintf("random_d%d_%d", n, rep), a32, b32))
			cases = append(cases, kernelCase64(fmt.Sprintf("random_d%d_%d", n, rep), a64, b64))
		}
		// Wide magnitudes, so that rounding differs between summation orders.
		a32, b32 := make([]float32, n), make([]float32, n)
		a64, b64 := make([]float64, n), make([]float64, n)
		for i := 0; i < n; i++ {
			e := float64(r.Intn(40) - 20)
			a32[i], b32[i] = float32(r.NormFloat64()*math.Pow(2, e)), float32(r.NormFloat64())
			a64[i], b64[i] = r.NormFloat64()*math.Pow(2, e), r.NormFloat64()
		}
		cases = append(cases, kernelCase32(fmt.Sprintf("wide_d%d", n), a32, b32))
		cases = append(cases, kernelCase64(fmt.Sprintf("wide_d%d", n), a64, b64))
	}
	// Self distance, zero vectors, a vector and its negation, infinities, NaN, subnormals and overflow.
	self32 := []float32{0.25, -1.5, 3.75, 1e-3, 7, -2, 0.5}
	self64 := []float64{0.25, -1.5, 3.75, 1e-3, 7, -2, 0.5}
	neg32, neg64 := make([]float32, len(self32)), make([]float64, len(self64))
	for i := range self32 {
		neg32[i], neg64[i] = -self32[i], -self64[i]
	}
	inf32, inf64 := float32(math.Inf(1)), math.Inf(1)
	nan32, nan64 := float32(math.NaN()), math.NaN()
	sub32 := math.Float32frombits(1)
	sub64 := math.Float64frombits(1)
	edges32 := []struct {
		name string
		a, b []float32
	}{
		{"self", self32, self32},
		{"negation", self32, neg32},
		{"zeros", make([]float32, 16), make([]float32, 16)},
		{"zero_and_one", make([]float32, 5), []float32{1, 1, 1, 1, 1}},
		{"negative_zero", []float32{float32(math.Copysign(0, -1)), 1}, []float32{0, 1}},
		{"infinity", []float32{inf32, 1, 2}, []float32{1, 1, 2}},
		{"infinity_both", []float32{inf32, 1}, []float32{inf32, 1}},
		{"nan", []float32{nan32, 1, 2}, []float32{1, 1, 2}},
		{"subnormal", []float32{sub32, sub32, sub32}, []float32{sub32, 0, sub32}},
		{"overflow", []float32{3e38, 3e38, 3e38}, []float32{-3e38, 3e38, 3e38}},
	}
	for _, e := range edges32 {
		cases = append(cases, kernelCase32(e.name, e.a, e.b))
	}
	edges64 := []struct {
		name string
		a, b []float64
	}{
		{"self", self64, self64},
		{"negation", self64, neg64},
		{"zeros", make([]float64, 16), make([]float64, 16)},
		{"zero_and_one", make([]float64, 5), []float64{1, 1, 1, 1, 1}},
		{"negative_zero", []float64{math.Copysign(0, -1), 1}, []float64{0, 1}},
		{"infinity", []float64{inf64, 1, 2}, []float64{1, 1, 2}},
		{"infinity_both", []float64{inf64, 1}, []float64{inf64, 1}},
		{"nan", []float64{nan64, 1, 2}, []float64{1, 1, 2}},
		{"subnormal", []float64{sub64, sub64, sub64}, []float64{sub64, 0, sub64}},
		{"overflow", []float64{1.7e308, 1.7e308}, []float64{-1.7e308, 1.7e308}},
	}
	for _, e := range edges64 {
		cases = append(cases, kernelCase64(e.name, e.a, e.b))
	}
	return cases, nil
}

// hnswMatrixCase is a neighbour matrix and its encoding in hex; a nil encoding is "".
type hnswMatrixCase struct {
	Name    string     `json:"name"`
	Matrix  [][]uint64 `json:"matrix"`
	Encoded string     `json:"encoded"`
}

func genHnswMatrix() (any, error) {
	r := rand.New(rand.NewSource(20261024))
	matrices := map[string][][]uint64{
		"empty":            {},
		"one_empty_row":    {{}},
		"three_empty_rows": {{}, {}, {}},
		"one_uid":          {{1}},
		"rows":             {{1, 2, 3}, {}, {0xffffffffffffffff, 0}},
		"high_bits":        {{0x0102030405060708, 0x8000000000000000}},
	}
	for i := 0; i < 8; i++ {
		rows := r.Intn(6) + 1
		m := make([][]uint64, rows)
		for j := range m {
			m[j] = make([]uint64, r.Intn(40))
			for k := range m[j] {
				m[j][k] = r.Uint64()
			}
		}
		matrices[fmt.Sprintf("random_%d", i)] = m
	}
	names := make([]string, 0, len(matrices))
	for n := range matrices {
		names = append(names, n)
	}
	sort.Strings(names)
	cases := []hnswMatrixCase{}
	for _, n := range names {
		m := matrices[n]
		cases = append(cases, hnswMatrixCase{Name: n, Matrix: m, Encoded: hex.EncodeToString(hnswEncodeMatrix(m))})
	}
	return cases, nil
}

// hnswParseEdgesCase is ParseEdges' result, or its error's text.
type hnswParseEdgesCase struct {
	Input string   `json:"input"`
	Uids  []uint64 `json:"uids"`
	Error string   `json:"error"`
}

func genHnswParseEdges() (any, error) {
	inputs := []string{
		"", " ", "\n\t", "[]", "[ ]", "[1]", "[1,2,3]", "[1, 2, 3]", "[ 1 , 2 ]", "[1 2 3]", "[1   2]",
		"[1\n2\t3]", "  [4,5]  ", "[18446744073709551615]", "[18446744073709551616]", "[-1]", "[1,]", "[,1]",
		"[1,,2]", "[a]", "1,2", "[1,2", "1,2]", "[[1]]", "[1]]", "null", "[0]", "[1.0]", "[ 1 2,3 ]",
		"[007]", "[+1]", "[0x1]",
	}
	cases := []hnswParseEdgesCase{}
	for _, in := range inputs {
		uids, err := hnsw.ParseEdges(in)
		c := hnswParseEdgesCase{Input: in, Uids: uids}
		if err != nil {
			c.Error = err.Error()
		}
		cases = append(cases, c)
	}
	return cases, nil
}

// hnswOptionsCase is the options an index is created with and the parameters it takes, or the error refusing them.
type hnswOptionsCase struct {
	Name           string            `json:"name"`
	Options        map[string]string `json:"options"`
	MaxLevels      int64             `json:"max_levels"`
	EfConstruction int64             `json:"ef_construction"`
	EfSearch       int64             `json:"ef_search"`
	Metric         string            `json:"metric"`
	Error          string            `json:"error"`
}

func genHnswOptions() (any, error) {
	factory := hnsw.CreateFactory[float32](32)
	sets := []struct {
		name string
		opts map[string]string
	}{
		{"defaults", map[string]string{}},
		{"exponent_4", map[string]string{"exponent": "4"}},
		{"exponent_1", map[string]string{"exponent": "1"}},
		{"exponent_and_levels", map[string]string{"exponent": "4", "maxLevels": "2"}},
		{"exponent_and_breadths", map[string]string{"exponent": "2", "efConstruction": "7", "efSearch": "9"}},
		{"levels", map[string]string{"maxLevels": "5"}},
		{"breadths", map[string]string{"efConstruction": "16", "efSearch": "12"}},
		{"cosine", map[string]string{"metric": "cosine"}},
		{"dotproduct", map[string]string{"metric": "dotproduct"}},
		{"euclidean", map[string]string{"metric": "euclidean"}},
		{"everything", map[string]string{"exponent": "3", "maxLevels": "4", "efConstruction": "60",
			"efSearch": "40", "metric": "cosine"}},
	}
	cases := []hnswOptionsCase{}
	allowed := factory.AllowedOptions()
	for i, s := range sets {
		o := opt.NewOptions()
		keys := make([]string, 0, len(s.opts))
		for k := range s.opts {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		c := hnswOptionsCase{Name: s.name, Options: s.opts}
		var perr error
		for _, k := range keys {
			v, err := allowed.GetParsedOption(k, s.opts[k])
			if err != nil {
				perr = err
				break
			}
			o.SetOpt(k, v)
		}
		if perr != nil {
			c.Error = perr.Error()
			cases = append(cases, c)
			continue
		}
		idx, err := factory.Create(fmt.Sprintf("0-opts_%d", i), o, 32)
		if err != nil {
			c.Error = err.Error()
			cases = append(cases, c)
			continue
		}
		v := reflect.ValueOf(idx).Elem()
		c.MaxLevels = v.FieldByName("maxLevels").Int()
		c.EfConstruction = v.FieldByName("efConstruction").Int()
		c.EfSearch = v.FieldByName("efSearch").Int()
		c.Metric = v.FieldByName("simType").FieldByName("indexType").String()
		cases = append(cases, c)
	}
	// An option of the wrong type: Create refuses it with GetOpt's error.
	for _, which := range []string{"maxLevels", "efConstruction", "efSearch"} {
		o := opt.NewOptions()
		o.SetOpt(which, "x")
		_, err := factory.Create("0-opts_wrong_"+which, o, 32)
		c := hnswOptionsCase{Name: "wrong_type_" + which, Options: map[string]string{which: "string x"}}
		if err != nil {
			c.Error = err.Error()
		}
		cases = append(cases, c)
	}
	o := opt.NewOptions()
	o.SetOpt("metric", 7)
	_, err := factory.Create("0-opts_wrong_metric", o, 32)
	c := hnswOptionsCase{Name: "wrong_type_metric", Options: map[string]string{"metric": "int 7"}}
	if err != nil {
		c.Error = err.Error()
	}
	cases = append(cases, c)
	_, err = factory.Create("0-opts_0", opt.NewOptions(), 32)
	c = hnswOptionsCase{Name: "name_taken", Options: map[string]string{}}
	if err != nil {
		c.Error = err.Error()
	}
	cases = append(cases, c)
	return cases, nil
}

// goldenTxn is an in-memory index.Txn over one map, as test_helper.go's inMemTxn is over one timestamp's map; its Find
// scans the predicate's data keys in key order, as Dgraph's LocalCache.Find does, and an absent key is Dgraph's
// ErrNoValue.
type goldenTxn struct {
	db map[string][]byte
}

var errGoldenNoValue = errors.New("No value found")

func (t *goldenTxn) StartTs() uint64 { return 1 }

func (t *goldenTxn) Get(key []byte) ([]byte, error) { return t.GetWithLockHeld(key) }

func (t *goldenTxn) GetWithLockHeld(key []byte) ([]byte, error) {
	v, ok := t.db[string(key)]
	if !ok || len(v) == 0 {
		return nil, errGoldenNoValue
	}
	return v, nil
}

func (t *goldenTxn) Find(prefix []byte, filter func([]byte) bool) (uint64, error) {
	attr := string(prefix)
	pfx := x.DataKey(attr, 0)
	pfx = pfx[:len(pfx)-8]
	keys := []string{}
	for k := range t.db {
		if len(k) >= len(pfx) && k[:len(pfx)] == string(pfx) {
			keys = append(keys, k)
		}
	}
	sort.Strings(keys)
	for _, k := range keys {
		v := t.db[k]
		if len(v) == 0 {
			continue
		}
		if filter(v) {
			pk, err := x.Parse([]byte(k))
			if err != nil {
				return 0, err
			}
			return pk.Uid, nil
		}
	}
	return 0, errors.New("Key not found")
}

func (t *goldenTxn) AddMutation(ctx context.Context, key []byte, kv *index.KeyValue) error {
	return t.AddMutationWithLockHeld(ctx, key, kv)
}

func (t *goldenTxn) AddMutationWithLockHeld(_ context.Context, key []byte, kv *index.KeyValue) error {
	t.db[string(key)] = append([]byte(nil), kv.Value...)
	return nil
}

func (t *goldenTxn) LockKey([]byte)   {}
func (t *goldenTxn) UnlockKey([]byte) {}

// hnswGraphOp is one step of a graph run: an insert, a delete (the vector removed and its uid appended to the dead
// list, as posting/index.go does), or a search.
type hnswGraphOp struct {
	Op string `json:"op"`
	// Uid is the inserted, deleted or (for search_uid) queried uid.
	Uid uint64 `json:"uid"`
	// Vector is the inserted vector or the query, as bits of the run's width.
	Vector []uint64 `json:"vector"`
	K      int      `json:"k"`
	// EfOverride and Threshold are the search's options; a threshold of null is none.
	EfOverride int      `json:"ef_override"`
	Threshold  *float64 `json:"threshold"`
	// Edges are an insert's edges, "attr/entity/value hex", sorted (Go builds them from a map).
	Edges []string `json:"edges"`
	// Result is a search's uids, in Dgraph's order.
	Result []uint64 `json:"result"`
	Error  string   `json:"error"`
}

// hnswGraphCase is one run: the index's options, the seed of the global random source and the draws it gave, the
// steps, and the store's contents after the last step, "key hex" to "value hex".
type hnswGraphCase struct {
	Name     string            `json:"name"`
	Bits     int               `json:"bits"`
	Pred     string            `json:"pred"`
	Options  map[string]string `json:"options"`
	Seed     int64             `json:"seed"`
	Draws    []uint64          `json:"draws"`
	Ops      []hnswGraphOp     `json:"ops"`
	Contents map[string]string `json:"contents"`
}

type hnswGraphSpec struct {
	name    string
	bits    int
	options map[string]string
	nodes   int
	dims    int
	deletes []uint64
	more    int
}

func vecBytes32(v []float32) []byte {
	b := make([]byte, 4*len(v))
	for i, f := range v {
		binary.LittleEndian.PutUint32(b[4*i:], math.Float32bits(f))
	}
	return b
}

func vecBytes64(v []float64) []byte {
	b := make([]byte, 8*len(v))
	for i, f := range v {
		binary.LittleEndian.PutUint64(b[8*i:], math.Float64bits(f))
	}
	return b
}

func edgeNames(edges []*index.KeyValue) []string {
	out := []string{}
	for _, e := range edges {
		out = append(out, fmt.Sprintf("%s/%d/%s", e.Attr, e.Entity, hex.EncodeToString(e.Value)))
	}
	sort.Strings(out)
	return out
}

// runGraph runs spec with T the vectors' type: its inserts, its deletes and the inserts after them, then searches of
// every kind.
func runGraph[T float32 | float64](spec hnswGraphSpec, seed int64, toBits func([]T) []uint64,
	toBytes func([]T) []byte, rnd func(r *rand.Rand) T) (hnswGraphCase, error) {
	pred := x.NamespaceAttr(0, "vec_"+spec.name)
	factory := hnsw.CreateFactory[T](spec.bits)
	allowed := factory.AllowedOptions()
	o := opt.NewOptions()
	keys := make([]string, 0, len(spec.options))
	for k := range spec.options {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		v, err := allowed.GetParsedOption(k, spec.options[k])
		if err != nil {
			return hnswGraphCase{}, err
		}
		o.SetOpt(k, v)
	}
	gc := hnswGraphCase{Name: spec.name, Bits: spec.bits, Pred: pred, Options: spec.options, Seed: seed}
	// The global source gives the levels; a copy seeded alike records the draws it gives.
	rand.Seed(seed)
	copySrc := rand.New(rand.NewSource(seed))
	for i := 0; i < 4*(spec.nodes+spec.more); i++ {
		gc.Draws = append(gc.Draws, math.Float64bits(copySrc.Float64()))
	}
	vr := rand.New(rand.NewSource(seed + 1))
	txn := &goldenTxn{db: map[string][]byte{}}
	ctx := context.Background()
	vecs := map[uint64][]T{}
	newVec := func() []T {
		v := make([]T, spec.dims)
		for i := range v {
			v[i] = rnd(vr)
		}
		return v
	}
	insert := func(uid uint64, v []T) error {
		txn.db[string(x.DataKey(pred, uid))] = toBytes(v)
		vecs[uid] = v
		// Every insert builds a fresh index, as Dgraph's CreateIndex does.
		idx, err := factory.CreateOrReplace(pred, o, spec.bits)
		if err != nil {
			return err
		}
		edges, err := idx.Insert(ctx, hnsw.NewTxnCache(txn, 1), uid, v)
		op := hnswGraphOp{Op: "insert", Uid: uid, Vector: toBits(v), Edges: edgeNames(edges)}
		if err != nil {
			op.Error = err.Error()
		}
		gc.Ops = append(gc.Ops, op)
		return nil
	}
	for i := 1; i <= spec.nodes; i++ {
		if err := insert(uint64(i*3), newVec()); err != nil {
			return gc, err
		}
	}
	if len(spec.deletes) > 0 {
		deadKey := string(x.DataKey(hnsw.ConcatStrings(pred, hnsw.VecDead), 1))
		dead := []uint64{}
		for _, uid := range spec.deletes {
			delete(txn.db, string(x.DataKey(pred, uid)))
			delete(vecs, uid)
			dead = append(dead, uid)
			b, err := json.Marshal(dead)
			if err != nil {
				return gc, err
			}
			txn.db[deadKey] = b
			gc.Ops = append(gc.Ops, hnswGraphOp{Op: "delete", Uid: uid})
		}
	}
	for i := 1; i <= spec.more; i++ {
		if err := insert(uint64(1000+i*3), newVec()); err != nil {
			return gc, err
		}
	}
	idx, err := factory.CreateOrReplace(pred, o, spec.bits)
	if err != nil {
		return gc, err
	}
	qc := hnsw.NewQueryCache(&goldenLocalCache{txn}, 1)
	so := idx.(index.OptionalSearchOptions[T])
	for q := 0; q < 6; q++ {
		query := newVec()
		for _, k := range []int{1, 3, 10} {
			res, err := idx.Search(ctx, qc, query, k, index.AcceptAll[T])
			op := hnswGraphOp{Op: "search", Vector: toBits(query), K: k, Result: res}
			if err != nil {
				op.Error = err.Error()
			}
			gc.Ops = append(gc.Ops, op)
		}
		for _, ef := range []int{0, 2, 50} {
			for _, th := range []*float64{nil, ptrF(0.5), ptrF(1.2)} {
				res, err := so.SearchWithOptions(ctx, qc, query, 4,
					index.VectorIndexOptions[T]{EfOverride: ef, DistanceThreshold: th})
				op := hnswGraphOp{Op: "search_options", Vector: toBits(query), K: 4, EfOverride: ef,
					Threshold: th, Result: res}
				if err != nil {
					op.Error = err.Error()
				}
				gc.Ops = append(gc.Ops, op)
			}
		}
	}
	uids := make([]uint64, 0, len(vecs))
	for uid := range vecs {
		uids = append(uids, uid)
	}
	sort.Slice(uids, func(i, j int) bool { return uids[i] < uids[j] })
	for i, uid := range uids {
		if i%3 != 0 {
			continue
		}
		res, err := idx.SearchWithUid(ctx, qc, uid, 3, index.AcceptAll[T])
		op := hnswGraphOp{Op: "search_uid", Uid: uid, K: 3, Result: res}
		if err != nil {
			op.Error = err.Error()
		}
		gc.Ops = append(gc.Ops, op)
		res, err = so.SearchWithUidAndOptions(ctx, qc, uid, 3,
			index.VectorIndexOptions[T]{EfOverride: 20, DistanceThreshold: ptrF(0.9)})
		op = hnswGraphOp{Op: "search_uid_options", Uid: uid, K: 3, EfOverride: 20, Threshold: ptrF(0.9),
			Result: res}
		if err != nil {
			op.Error = err.Error()
		}
		gc.Ops = append(gc.Ops, op)
	}
	// A uid without a vector.
	res, err := idx.SearchWithUid(ctx, qc, 999999, 3, index.AcceptAll[T])
	op := hnswGraphOp{Op: "search_uid", Uid: 999999, K: 3, Result: res}
	if err != nil {
		op.Error = err.Error()
	}
	gc.Ops = append(gc.Ops, op)
	gc.Contents = map[string]string{}
	for k, v := range txn.db {
		gc.Contents[hex.EncodeToString([]byte(k))] = hex.EncodeToString(v)
	}
	return gc, nil
}

func ptrF(f float64) *float64 { return &f }

// goldenLocalCache is the query side of goldenTxn: Dgraph's viLocalCache over a LocalCache.
type goldenLocalCache struct{ txn *goldenTxn }

func (c *goldenLocalCache) Get(key []byte) ([]byte, error) { return c.txn.Get(key) }
func (c *goldenLocalCache) GetWithLockHeld(key []byte) ([]byte, error) {
	return c.txn.GetWithLockHeld(key)
}
func (c *goldenLocalCache) Find(prefix []byte, filter func([]byte) bool) (uint64, error) {
	return c.txn.Find(prefix, filter)
}

func genHnswGraph() (any, error) {
	specs := []hnswGraphSpec{
		{name: "euclidean", bits: 32, options: map[string]string{"maxLevels": "3", "efConstruction": "12",
			"efSearch": "8"}, nodes: 60, dims: 4},
		{name: "cosine", bits: 32, options: map[string]string{"maxLevels": "3", "efConstruction": "12",
			"efSearch": "8", "metric": "cosine"}, nodes: 60, dims: 4},
		{name: "dotproduct", bits: 32, options: map[string]string{"maxLevels": "3", "efConstruction": "12",
			"efSearch": "8", "metric": "dotproduct"}, nodes: 60, dims: 4},
		{name: "defaults", bits: 32, options: map[string]string{}, nodes: 40, dims: 8},
		{name: "exponent", bits: 32, options: map[string]string{"exponent": "2", "metric": "cosine"}, nodes: 40,
			dims: 3},
		{name: "euclidean_f64", bits: 64, options: map[string]string{"maxLevels": "4", "efConstruction": "6",
			"efSearch": "5"}, nodes: 50, dims: 5},
		{name: "cosine_f64", bits: 64, options: map[string]string{"maxLevels": "2", "efConstruction": "6",
			"efSearch": "5", "metric": "cosine"}, nodes: 50, dims: 5},
		{name: "dead", bits: 32, options: map[string]string{"maxLevels": "3", "efConstruction": "8",
			"efSearch": "6"}, nodes: 30, dims: 3, deletes: []uint64{3, 15, 30, 45}, more: 15},
		{name: "dead_entry", bits: 32, options: map[string]string{"maxLevels": "2", "efConstruction": "8",
			"efSearch": "6"}, nodes: 12, dims: 2, deletes: []uint64{3}, more: 6},
		{name: "one_level", bits: 32, options: map[string]string{"maxLevels": "1", "efConstruction": "5",
			"efSearch": "4"}, nodes: 25, dims: 2},
	}
	cases := []hnswGraphCase{}
	for i, s := range specs {
		seed := int64(7001 + i)
		var gc hnswGraphCase
		var err error
		if s.bits == 32 {
			gc, err = runGraph[float32](s, seed, bits32, vecBytes32, func(r *rand.Rand) float32 {
				return r.Float32()*2 - 1
			})
		} else {
			gc, err = runGraph[float64](s, seed, bits64, vecBytes64, func(r *rand.Rand) float64 {
				return r.Float64()*2 - 1
			})
		}
		if err != nil {
			return nil, err
		}
		cases = append(cases, gc)
	}
	return cases, nil
}
