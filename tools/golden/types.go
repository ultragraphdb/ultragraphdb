// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_types: Dgraph's scalar values (types/conversion.go, value.go,
// scalar_types.go, password.go, sort.go). Every value goes through the fork's own functions: Convert from the
// binary or text form of a type to every other type, then Marshal of the result to binary and to string.
//
// The types vectors are generated with GOFLAGS=-gcflags=all=-d=fmahash=n like the S2 ones (types_s2.go); they come
// out the same without it.

import (
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"math"
	"math/big"
	"math/rand"
	"strings"
	"time"

	geom "github.com/twpayne/go-geom"
	"github.com/twpayne/go-geom/encoding/wkb"

	"github.com/dgraph-io/dgraph/v25/types"
)

func init() {
	register("types_convert", "ultragraphdb_core_types", genTypesConvert)
	register("types_parse_time", "ultragraphdb_core_types", genTypesParseTime)
	register("types_type_for_value", "ultragraphdb_core_types", genTypesTypeForValue)
	register("types_password", "ultragraphdb_core_types", genTypesPassword)
	register("types_sort", "ultragraphdb_core_types", genTypesSort)
	register("types_sort_lang", "ultragraphdb_core_types", genTypesSortLang)
}

// typeIDs are every identifier Convert and Marshal are asked for, Dgraph's twelve and OBJECT (10), which Dgraph
// has no TypeID constant for.
var typeIDs = []types.TypeID{
	types.DefaultID, types.BinaryID, types.IntID, types.FloatID, types.BoolID, types.DateTimeID, types.GeoID,
	types.UidID, types.PasswordID, types.StringID, types.TypeID(10), types.BigFloatID, types.VFloatID,
}

// rendered is a converted value: its Go type, Marshal to binary and to string (hex), and for big floats the exact
// value and precision. A panic in Dgraph's code is recorded as its message.
type rendered struct {
	GoType   string `json:"go_type"`
	Nil      bool   `json:"nil,omitempty"`
	Bin      string `json:"bin,omitempty"`
	BinErr   string `json:"bin_err,omitempty"`
	Str      string `json:"str,omitempty"`
	StrErr   string `json:"str_err,omitempty"`
	Prec     uint   `json:"prec,omitempty"`
	P        string `json:"p,omitempty"`
	Salted   bool   `json:"salted,omitempty"`
	Panicked string `json:"panic,omitempty"`
}

type convCase struct {
	From     uint32    `json:"from"`
	To       uint32    `json:"to"`
	In       string    `json:"in"`
	Err      string    `json:"err,omitempty"`
	Panicked string    `json:"panic,omitempty"`
	Out      *rendered `json:"out,omitempty"`
}

// convert runs types.Convert and recovers a panic (big.Float.SetFloat64 panics on NaN).
func convert(v types.Val, to types.TypeID) (out types.Val, err error, panicked string) {
	defer func() {
		if r := recover(); r != nil {
			panicked = fmt.Sprint(r)
		}
	}()
	out, err = types.Convert(v, to)
	return
}

func h(b []byte) string { return hex.EncodeToString(b) }

func i64(v int64) []byte {
	var b [8]byte
	binary.LittleEndian.PutUint64(b[:], uint64(v))
	return b[:]
}

func f64(v float64) []byte {
	var b [8]byte
	binary.LittleEndian.PutUint64(b[:], math.Float64bits(v))
	return b[:]
}

func tbin(t time.Time) []byte {
	b, err := t.MarshalBinary()
	if err != nil {
		panic(err)
	}
	return b
}

func wkbOf(g geom.T) []byte {
	b, err := wkb.Marshal(g, binary.LittleEndian)
	if err != nil {
		panic(err)
	}
	return b
}

// marshalTo runs types.Marshal and recovers a panic (Dgraph asserts on the Go type of the value).
func marshalTo(v types.Val, to types.TypeID) (out types.Val, err error, panicked string) {
	defer func() {
		if r := recover(); r != nil {
			panicked = fmt.Sprint(r)
		}
	}()
	out = types.ValueForType(to)
	err = types.Marshal(v, &out)
	return
}

func goBytes(v any) []byte {
	switch x := v.(type) {
	case []byte:
		return x
	case string:
		return []byte(x)
	}
	panic(fmt.Sprintf("not bytes: %T", v))
}

func render(v types.Val) *rendered {
	r := &rendered{GoType: fmt.Sprintf("%T", v.Value)}
	if v.Value == nil {
		r.Nil = true
		return r
	}
	switch x := v.Value.(type) {
	case big.Float:
		r.Prec, r.P = x.Prec(), x.Text('p', 0)
	case *big.Float:
		r.Prec, r.P = x.Prec(), x.Text('p', 0)
	}
	if v.Tid == types.PasswordID {
		if s, ok := v.Value.(string); ok && strings.HasPrefix(s, "$2a$") {
			r.Salted = true
			r.Str = h([]byte(s[:7]))
			return r
		}
	}
	b, err, p := marshalTo(v, types.BinaryID)
	switch {
	case p != "":
		r.Panicked = p
	case err != nil:
		r.BinErr = err.Error()
	default:
		r.Bin = h(goBytes(b.Value))
	}
	s, err, p := marshalTo(v, types.StringID)
	switch {
	case p != "":
		r.Panicked = p
	case err != nil:
		r.StrErr = err.Error()
	case s.Value == nil:
		r.StrErr = "<nil>"
	default:
		r.Str = h(goBytes(s.Value))
	}
	return r
}

// convertInputs are the binary or text forms each source type is converted from.
func convertInputs() map[types.TypeID][][]byte {
	r := rand.New(rand.NewSource(7))
	ints := []int64{0, 1, -1, 3, -1221, 9999, 1257894000, -4410000, 2204578800, -2150326800, math.MaxInt64,
		math.MinInt64, 1 << 53, 1<<53 + 1, 253402300799, 253402300800, -62135596800, -62135596801}
	floats := []float64{0, math.Copysign(0, -1), 1, -1, 1.3, -3.5, 3.4434, 0.5e2, 13816.251, 2.5e-7, 1e20, -1e20,
		1e21, 1e-7, 123456789, 1e6, 1e-5, 522638295213.3243, -522638295213.3243, 9.2233720368547758e18,
		-9.2233720368547758e18, 9.223372036854775e18, math.MaxFloat64, math.SmallestNonzeroFloat64, math.NaN(),
		math.Inf(1), math.Inf(-1), 1257894000.001, -4409999.999, 0.1, 1.0 / 3, 100, 1e15, 1e16, 1e-4,
		123456.789e-10}
	for i := 0; i < 40; i++ {
		floats = append(floats, math.Float64frombits(r.Uint64()))
		floats = append(floats, (r.Float64()-0.5)*math.Pow(10, float64(r.Intn(40)-20)))
	}
	in := map[types.TypeID][][]byte{}
	for _, v := range ints {
		in[types.IntID] = append(in[types.IntID], i64(v))
	}
	in[types.IntID] = append(in[types.IntID], []byte{}, []byte{1, 2, 3}, append(i64(5), 9, 9))
	for _, v := range floats {
		in[types.FloatID] = append(in[types.FloatID], f64(v))
	}
	in[types.FloatID] = append(in[types.FloatID], []byte{}, []byte{0, 0, 0, 0, 0, 0, 0xf0})
	in[types.BoolID] = [][]byte{{}, {0}, {1}, {2}, {8}, {1, 5}}
	times := []time.Time{
		{},
		time.Date(2006, 1, 2, 15, 4, 5, 0, time.UTC),
		time.Date(2016, 1, 1, 0, 0, 0, 0, time.UTC),
		time.Date(2009, time.November, 10, 23, 0, 0, 1000000, time.UTC),
		time.Date(1969, time.November, 10, 23, 0, 0, 1000000, time.UTC),
		time.Date(1901, time.November, 10, 23, 0, 0, 0, time.UTC),
		time.Date(2018, 5, 30, 9, 30, 10, 500000000, time.FixedZone("", -6*3600)),
		time.Date(2018, 5, 28, 14, 41, 57, 0, time.FixedZone("", 23*3600+59*60)),
		time.Date(2018, 5, 28, 14, 41, 57, 123456789, time.FixedZone("", 5*3600+30*60+15)),
		time.Date(2018, 5, 28, 14, 41, 57, 0, time.FixedZone("", 0)),
		time.Date(2018, 5, 28, 14, 41, 57, 0, time.FixedZone("", 25*3600)),
		time.Date(9999, 12, 31, 23, 59, 59, 999999999, time.UTC),
		time.Date(10000, 1, 1, 0, 0, 0, 0, time.UTC),
		time.Date(-1, 1, 1, 0, 0, 0, 0, time.UTC),
		time.Date(2262, 4, 11, 23, 47, 16, 854775808, time.UTC),
		time.Date(1677, 9, 21, 0, 12, 43, 145224191, time.UTC),
		time.Date(2000, 2, 29, 12, 0, 0, 7, time.UTC),
	}
	for _, t := range times {
		in[types.DateTimeID] = append(in[types.DateTimeID], tbin(t))
	}
	in[types.DateTimeID] = append(in[types.DateTimeID], []byte{}, []byte{8}, []byte{1, 2, 3},
		append(tbin(time.Date(2006, 1, 2, 15, 4, 5, 0, time.UTC)), 0))
	for _, g := range goldenGeoms() {
		in[types.GeoID] = append(in[types.GeoID], wkbOf(g))
	}
	in[types.GeoID] = append(in[types.GeoID], []byte{}, []byte{1}, []byte{2, 1, 0, 0, 0}, []byte{1, 9, 0, 0, 0},
		[]byte{1, 1, 0, 0, 0, 1, 2}, []byte{1, 0xe9, 3, 0, 0})
	texts := []string{"", "a", "abc", "hello", "2016", "srfrog", "1", "0", "-1221", "13816", "203716381366627",
		"9223372036854775807", "9223372036854775808", "-9223372036854775809", "3.0", "-3a.5", "+5", "1.3",
		"13816.251", "-1221.12", "-0.0", "1e10", "1e-2", "1e400", "-1e400", "NaN", "nan", "Inf", "-Inf", "inf",
		"Infinity", "1_000", "0x1p-2", "0x1.8p1", "2.5e-7", "1e-400", "true", "True", "TRUE", "T", "t", "F", "f",
		"false", "False", "FALSE", "yes", "3", "-3", "2006-01-02T15:04:05", "2006-01-02", "2006-01", "2006",
		"2018-10-03T08:05:01Z", "2018-10-03T08:05:01+25:00", "2018-05-30T09:30:10-06:00",
		"2006-01-02 15:04:05 -0700 MST", "testing", "pass", "password", "123456",
		"{'type':'Point','coordinates':[1,2]}", `{"type":"Point","coordinates":[1.5,-2.25]}`,
		"{'type':'MultiLineString','coordinates':[[[1,2,3],[4,5,6],[7,8,9],[1,2,3]]]}",
		"{'type':'Polygon','coordinates':[[[-122,37],[-123,37],[-123,38],[-122,38],[-122,37]]]}",
		"{'type':'Curve','coordinates':[1,2]}", "{}", "thisisntjson", "{'type':'Point'}",
		"{'type':'Point','coordinates':[1]}", "{'type':'Point','coordinates':[1e-7,1e21]}",
		"{'Type':'Point','COORDINATES':[3,4]}", "{'type':'GeometryCollection','geometries':[{'type':'Point','coordinates':[1,2]}]}",
		"[1.0, 2.0]", "[1 2 3]", "[ 0.1,0.2 , 3.1415926539 ]", "[]", "", "[1,", "[a]", "1,2", "[1e39]", "[\n1\t2 ]",
		"0b101", "1e1000000000", "\xff\xfe", "日本",
	}
	for _, t := range texts {
		in[types.StringID] = append(in[types.StringID], []byte(t))
	}
	in[types.DefaultID] = in[types.StringID]
	in[types.PasswordID] = [][]byte{[]byte(""), []byte("password"), []byte("$2a$10$abc")}
	in[types.BigFloatID] = [][]byte{[]byte("0"), []byte("1"), []byte("-1.5"), []byte("2.5e-7"), []byte("1e21"),
		[]byte("123456789012345678901234567890.123"), []byte("1257894000.001"), []byte("-4409999.999"),
		[]byte("9.3e18"), []byte("-9.3e18"), []byte("+Inf"), []byte("-Inf"), []byte("NaN"), []byte(""),
		[]byte("1e400"), []byte("0.1"), []byte("-0")}
	for _, v := range [][]float32{{}, {0.1}, {0}, {0.65433, 1.855, 3.1415926539}, {float32(math.Inf(1)), -1e-45}} {
		in[types.VFloatID] = append(in[types.VFloatID], types.FloatArrayAsBytes(v))
	}
	in[types.VFloatID] = append(in[types.VFloatID], []byte{1, 2, 3})
	// Binary: every other source's bytes, which Convert reinterprets as the target type.
	for _, tid := range []types.TypeID{types.IntID, types.FloatID, types.BoolID, types.DateTimeID, types.GeoID,
		types.StringID, types.BigFloatID, types.VFloatID} {
		in[types.BinaryID] = append(in[types.BinaryID], in[tid]...)
	}
	in[types.UidID] = [][]byte{i64(7)}
	in[types.TypeID(10)] = [][]byte{[]byte("x")}
	return in
}

func genTypesConvert() (any, error) {
	in := convertInputs()
	var cases []convCase
	for _, from := range typeIDs {
		for _, data := range in[from] {
			for _, to := range typeIDs {
				if to == types.PasswordID && (from == types.StringID || from == types.DefaultID) && len(data) > 40 {
					continue // bcrypt refuses passwords over 72 bytes; keep the salted cases few
				}
				c := convCase{From: uint32(from), To: uint32(to), In: h(data)}
				out, err, p := convert(types.Val{Tid: from, Value: data}, to)
				if p != "" {
					c.Panicked = p
				} else if err != nil {
					c.Err = err.Error()
				} else {
					c.Out = render(out)
				}
				cases = append(cases, c)
			}
		}
	}
	// A value that is not a []byte is refused.
	for _, to := range typeIDs {
		c := convCase{From: uint32(types.StringID), To: uint32(to), In: "string:a"}
		_, err := types.Convert(types.Val{Tid: types.StringID, Value: "a"}, to)
		if err != nil {
			c.Err = err.Error()
		}
		cases = append(cases, c)
	}
	return cases, nil
}

type parseTimeCase struct {
	In   string `json:"in"`
	Err  string `json:"err,omitempty"`
	Bin  string `json:"bin,omitempty"`
	Text string `json:"text,omitempty"`
	Unix int64  `json:"unix,omitempty"`
	Nsec int    `json:"nsec,omitempty"`
}

func genTypesParseTime() (any, error) {
	ins := []string{
		"2018-10-28T04:00:10Z", "2018-10-28T04:00:10-00:00", "2018-05-30T09:30:10.5Z", "2018-05-30T09:30:10.5-00:00",
		"2018-05-30T09:30:10-06:00", "2018-05-28T14:41:57+23:00", "2018-05-28T14:41:57-23:40",
		"2018-05-28T14:41:57+23:59", "2018-05-28T14:41:57+25:00", "2018-05-28T14:41:57+30:00",
		"2018-05-28T14:41:57-25:01", "2018-05-28T14:41:57-30:00", "2018-10-28T04:00:10", "2018-05-30T09:30:10.5",
		"2018", "2018-01", "2018-01-01", "2018-10", "2018-10-03", "2018-10-03T08:05:01Z", "2018-10-03T08:05:01+25:00",
		"abcd", "12345", "123456", "1234567", "12345678", "123456789", "1234567891", "11111111111111111Z",
		"111111111111111:11", "1111-11-11T11:11111111:1", "1111-11-11T11:11:1111:11", "18-10-28T04:00:10Z",
		"318-10-28T04:00:10", "2018-110-28T04:00:10", "20181-4-28T25:00:10", "2018-10-218T04:00:10",
		"2018-14-28T25:00:10", "2018-142-8T25:00:10", "2018-05-33T09:65:10.5", "201", "2018-011", "2018-01-011",
		"", "2006-01-02 15:04:05 -0700 MST", "2006-01-02 15:04:05 +0000 UTC", "2006-01-02 15:04:05 +0530 IST",
		"2006-01-02 15:04:05 +0530 UTC", "2006-01-02 15:04:05 -0700 mst", "2006-01-02 15:04:05.123 -0700 MST",
		"2018-02-29", "2016-02-29", "2018-00", "2018-13", "2018-04-31", "0000", "9999-12-31T23:59:59.999999999Z",
		"2018-10-03T08:05:01.123456789123Z", "2018-10-03T08:05:01,5Z", "2018-10-03T24:00:00Z", "2018-10-03T23:60:00Z",
		"2018-10-03T23:59:60Z", "2018-10-03T08:05:01+24:00", "2018-10-03T08:05:01+23:61", "2018-10-03T08:05:01.5",
		"2018-10-03T08:05:01.", "2018-10-03T8:05:01Z", "2018-10-03T08:5:01Z", "2018-10-03T08:05:01Zjunk",
		"2018-10-03T08:05:01+0500", "2018-10-03T08:05:01 extra", "2018-10-03 08:05:01", "2018-1-3", "2018-1",
		"-018", "+2018-10-03", "2018-10-03X08:05:01Z", "2018-10-03T08:05:01+05:30", "2018-10-03T08:05:01-00:01",
		"1969-12-31T23:59:59.999999999Z", "0001-01-01T00:00:00Z", "2018-10-03T08:05:01.000Z",
		"2018-10-03T08:05:01.-5Z",
	}
	var cases []parseTimeCase
	for _, s := range ins {
		c := parseTimeCase{In: s}
		t, err := types.ParseTime(s)
		if err != nil {
			c.Err = err.Error()
		} else {
			if b, err := t.MarshalBinary(); err == nil {
				c.Bin = h(b)
			} else {
				c.Bin = "error: " + err.Error()
			}
			if txt, err := t.MarshalText(); err == nil {
				c.Text = string(txt)
			} else {
				c.Text = "error: " + err.Error()
			}
			c.Unix, c.Nsec = t.Unix(), t.Nanosecond()
		}
		cases = append(cases, c)
	}
	return cases, nil
}

type typeForValueCase struct {
	In  string    `json:"in"`
	Tid uint32    `json:"tid"`
	Out *rendered `json:"out,omitempty"`
}

func genTypesTypeForValue() (any, error) {
	ins := []string{"true", "TRUE", "True", "t", "false", "FALSE", "False", "f", "2018", "2018-10", "2018-10-03",
		"2018-10-03T20:47:53Z", "123", "-123", "+123", "0001", "+0", "-0", "1World", "3.14159", "-273.15",
		"2.99792e8", "9.1095E-28", "-.0", "+.0", ".1", "1.", "1-800-4GOLANG", "+1800-446-5264", "212.555.9876",
		"testing", "", "-", "+", ".", "tRUE", "Fa", "9223372036854775808", "1e400", "1e5", "1E5", "1-2", "2018-13",
		"2018-10-03T25:00:00Z", "0x10", "1_000", ".e5", "-.e5", "12a", "+.", "NaN", "Inf", "2018-10-03 08:05:01 -0700 MST",
		"1.5.5", "--1", "-+1", "+-", "1e", "1.e5", "١٢٣"}
	var cases []typeForValueCase
	for _, s := range ins {
		tid, v := types.TypeForValue([]byte(s))
		c := typeForValueCase{In: s, Tid: uint32(tid)}
		if v != nil {
			c.Out = render(types.Val{Tid: tid, Value: v})
		}
		cases = append(cases, c)
	}
	return cases, nil
}

type passwordCase struct {
	Plain string `json:"plain"`
	Hash  string `json:"hash,omitempty"`
	Err   string `json:"err,omitempty"`
}

type verifyCase struct {
	Plain string `json:"plain"`
	Hash  string `json:"hash"`
	Err   string `json:"err,omitempty"`
}

type passwordFile struct {
	Encrypt []passwordCase `json:"encrypt"`
	Verify  []verifyCase   `json:"verify"`
}

func genTypesPassword() (any, error) {
	var f passwordFile
	plains := []string{"", "12345", "123456", "1234567890", "testing", "password", "pässwörd", strings.Repeat("x", 72)}
	for _, p := range plains {
		c := passwordCase{Plain: p}
		hash, err := types.Encrypt(p)
		if err != nil {
			c.Err = err.Error()
		} else {
			c.Hash = hash
		}
		f.Encrypt = append(f.Encrypt, c)
	}
	for _, e := range f.Encrypt {
		if e.Hash == "" {
			continue
		}
		for _, p := range plains {
			c := verifyCase{Plain: p, Hash: e.Hash}
			if err := types.VerifyPassword(p, e.Hash); err != nil {
				c.Err = err.Error()
			}
			f.Verify = append(f.Verify, c)
		}
	}
	for _, c := range []verifyCase{{Plain: "123456", Hash: "$2a$10$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC"},
		{Plain: "123456", Hash: "$2a$10$kXtSFCVmzsu0lwVqaWxo5OXlLGUakcY2t18QcqcVpvoTHsPqclOca"},
		{Plain: "1234567890", Hash: "$2a$10$WdCWNpOP6c4l7ECv3hEWKeD3oSiszlRJFmT4uRT1P/W9V9zUye8pS"},
		{Plain: "123456", Hash: "notahash"}, {Plain: "123456", Hash: "$2a$10$short"}} {
		if err := types.VerifyPassword(c.Plain, c.Hash); err != nil {
			c.Err = err.Error()
		}
		f.Verify = append(f.Verify, c)
	}
	return []any{f}, nil // one case: the file holds a slice of cases
}

// sortVal is one value of a sort row: its type and binary form (Convert from binary rebuilds it), or nil.
type sortVal struct {
	Tid uint32 `json:"tid"`
	Bin string `json:"bin,omitempty"`
	Nil bool   `json:"nil,omitempty"`
}

type sortCase struct {
	Name  string      `json:"name"`
	Rows  [][]sortVal `json:"rows"`
	Desc  []bool      `json:"desc"`
	TopN  int         `json:"top_n,omitempty"`
	Uids  []uint64    `json:"uids,omitempty"`
	Err   string      `json:"err,omitempty"`
	Sizes int         `json:"sizes"`
}

func sortValOf(r *rand.Rand, tid types.TypeID, pool int) (types.Val, sortVal) {
	var b []byte
	switch tid {
	case types.IntID:
		b = i64(int64(r.Intn(pool)) - int64(pool/2))
	case types.FloatID:
		b = f64(float64(r.Intn(pool))/2 - float64(pool/4))
	case types.StringID, types.DefaultID:
		n := 1 + r.Intn(3)
		s := make([]byte, n)
		for i := range s {
			s[i] = "abc"[r.Intn(3)]
		}
		b = s
	case types.DateTimeID:
		b = tbin(time.Unix(int64(r.Intn(pool))*3600, 0).UTC())
	case types.BigFloatID:
		b = []byte(fmt.Sprintf("%d.5", r.Intn(pool)-pool/2))
	case types.BoolID:
		b = []byte{byte(r.Intn(2))}
	}
	v, err := types.Convert(types.Val{Tid: types.BinaryID, Value: b}, tid)
	if err != nil {
		panic(err)
	}
	return v, sortVal{Tid: uint32(tid), Bin: h(b)}
}

func genTypesSort() (any, error) {
	r := rand.New(rand.NewSource(11))
	var cases []sortCase
	kinds := [][]types.TypeID{{types.IntID}, {types.FloatID}, {types.StringID}, {types.DateTimeID},
		{types.BigFloatID}, {types.DefaultID}, {types.IntID, types.FloatID}, {types.StringID, types.IntID},
		{types.IntID, types.StringID}, {types.BoolID}, {types.UidID}}
	sizes := []int{0, 1, 2, 3, 5, 8, 12, 13, 20, 49, 50, 51, 100, 257}
	for ci := 0; ci < 160; ci++ {
		n := sizes[ci%len(sizes)]
		kind := kinds[r.Intn(len(kinds))]
		cols := 1 + r.Intn(2)
		pool := []int{3, 10, 1000}[r.Intn(3)]
		nilRate := []int{0, 0, 5, 2}[r.Intn(4)]
		mixed := len(kind) > 1
		var rows [][]types.Val
		var srows [][]sortVal
		for i := 0; i < n; i++ {
			var row []types.Val
			var srow []sortVal
			for c := 0; c < cols; c++ {
				tid := kind[0]
				if mixed {
					tid = kind[r.Intn(len(kind))]
				}
				if tid == types.UidID {
					row = append(row, types.Val{Tid: tid, Value: uint64(i)})
					srow = append(srow, sortVal{Tid: uint32(tid), Bin: h(i64(int64(i)))})
					continue
				}
				if nilRate > 0 && r.Intn(nilRate) == 0 {
					row = append(row, types.Val{Tid: tid})
					srow = append(srow, sortVal{Tid: uint32(tid), Nil: true})
					continue
				}
				v, sv := sortValOf(r, tid, pool)
				row = append(row, v)
				srow = append(srow, sv)
			}
			rows = append(rows, row)
			srows = append(srows, srow)
		}
		desc := make([]bool, cols)
		for c := range desc {
			desc[c] = r.Intn(2) == 0
		}
		uids := make([]uint64, n)
		for i := range uids {
			uids[i] = uint64((i + 1) * 100)
		}
		c := sortCase{Name: fmt.Sprintf("case%d", ci), Rows: srows, Desc: desc, Sizes: n}
		var err error
		if ci%3 == 2 && n > 0 {
			c.TopN = 1 + r.Intn(n)
			err = types.SortTopN(rows, &uids, desc, "", c.TopN)
		} else {
			err = types.Sort(rows, &uids, desc, "")
		}
		if err != nil {
			c.Err = err.Error()
		} else {
			c.Uids = uids
		}
		cases = append(cases, c)
	}
	return cases, nil
}

type langSortCase struct {
	Lang string   `json:"lang"`
	In   []string `json:"in"` // as encoding/json writes them: invalid UTF-8 as U+FFFD
	Uids []uint64 `json:"uids"`
	Out  []string `json:"out"`
	// InHex and OutHex hold the strings' bytes, which may be invalid UTF-8, as Dgraph's string values may.
	InHex  []string `json:"in_hex"`
	OutHex []string `json:"out_hex"`
}

// genTypesSortLang sorts strings with a language collator (golang.org/x/text/collate, as ultragraphdb_core_text
// ports it): tailored languages, tags whose -u extension sets collation options (where CompareString and a
// comparison of collation keys disagree), and tags language.Parse refuses, which sort by bytes. Strings holding
// invalid UTF-8 sort as CompareString weighs them: each invalid sequence as U+FFFD's implicit weight.
func genTypesSortLang() (any, error) {
	// mixed exercises case, accents, ignorables, punctuation, digits and scripts under one tag.
	mixed := []string{"a b", "ab", "a-b", "a\x00b", "ab ", "a  b", "", "-", "côte", "coté", "cote", "côté",
		"1", "10", "2", "a\u0301", "á", "Dž", "ǅ", "ﬁ", "fi", "x\u200by", "xy", "中", "日本", "한국", "Ω", "ω",
		"B", "b", "A", "a", "Ä", "ä"}
	sets := []struct {
		lang string
		in   []string
	}{
		{"de", []string{"öffnen", "zumachen", "Äpfel", "Apfel", "apfel", "Zebra", "Straße", "Strasse", "über",
			"Ufer", "Müller", "Mueller", "Mull", "Öl", "Ofen", "ähnlich", "Ahnung", "ß", "ss", "Ü"}},
		{"sv", []string{"öppna", "zon", "åka", "äta", "ost", "zebra", "Ödla", "ära"}},
		{"en", []string{"b", "B", "a", "A", "ä", "Ä", "z", "Z", "é", "e", "E"}},
		{"xx-invalid-tag-!", []string{"b", "a", "ä", "Z"}},
		{"de-u-co-phonebk", []string{"Müller", "Mueller", "Mull", "Mütze", "Muff", "Ofen", "Öl", "Oel"}},
		{"da", []string{"Aarhus", "Ålborg", "Zeeland", "Æble", "Øre", "Odense", "aa", "zz", "ø", "æ"}},
		{"es", []string{"ñu", "nube", "oso", "llama", "luz", "chico", "cosa", "Ñandú", "nada"}},
		{"tr", []string{"ılık", "ilik", "Işık", "İzmir", "ırmak", "izci", "hz", "jale", "çay", "cam", "dal"}},
		{"fr-CA", []string{"cote", "côte", "coté", "côté", "Cote", "COTE"}},
		{"ja", []string{"か", "が", "カ", "ガ", "あ", "ア", "ん", "漢字", "かな", "カナ"}},
		{"zh", []string{"中", "国", "人", "大", "小", "山", "水", "火"}},
		{"und", mixed},
		{"fr", mixed},
		{"en-u-kn-true", mixed},
		{"en-u-ks-level1", mixed},
		{"en-u-ks-level2", mixed},
		{"en-u-ks-identic", mixed},
		{"en-u-ka-shifted", mixed},
		{"en-u-ka-blanked", mixed},
		{"en-u-kc-true-ks-level1", mixed},
		{"xx", []string{"b", "a", "ä", "Z"}},
		{"en_US", []string{"b", "a", "ä", "Z"}},
	}
	// invalid mixes invalid UTF-8 (lone continuation bytes, truncated and broken sequences, overlong encodings,
	// surrogates, 0xFE and 0xFF, at the start, in the middle and at the end) with valid text and U+FFFD itself.
	invalid := []string{"a", "b", "z", "ä", "\uFFFD", "a\uFFFD", "\x80", "\xbf", "\xff", "\xfe", "\xff\xfe",
		"\xfe\xff", "\xc0\x80", "\xe0\x80\x80", "\xed\xa0\x80", "\xf4\x90\x80\x80", "\xc3", "\xe2\x82",
		"\xe0\xa0x", "a\x80", "\x80a", "a\x80b", "a\xffb", "ab\xff", "\xc3\xa4\xff", "ch\x80", "c\x80h",
		"a\u0301\x80", "1\x802", "12\xff", "\u4E2D\xff", "e\xcc", "", "\x00\xff"}
	for _, lang := range []string{"en", "hu", "da", "cs", "ja", "fr-CA", "en-u-kn-true", "en-u-ks-identic",
		"en-u-ka-shifted", "en-u-ks-level1", "xx-invalid-tag-!"} {
		sets = append(sets, struct {
			lang string
			in   []string
		}{lang, invalid})
	}
	var cases []langSortCase
	for _, s := range sets {
		var rows [][]types.Val
		uids := make([]uint64, len(s.in))
		for i, v := range s.in {
			rows = append(rows, []types.Val{{Tid: types.StringID, Value: v}})
			uids[i] = uint64((i + 1) * 100)
		}
		if err := types.Sort(rows, &uids, []bool{false}, s.lang); err != nil {
			return nil, err
		}
		out := make([]string, len(rows))
		for i, r := range rows {
			out[i] = r[0].Value.(string)
		}
		hexes := func(xs []string) []string {
			var hs []string
			for _, x := range xs {
				hs = append(hs, hex.EncodeToString([]byte(x)))
			}
			return hs
		}
		cases = append(cases, langSortCase{Lang: s.lang, In: s.in, Uids: uids, Out: out, InHex: hexes(s.in),
			OutHex: hexes(out)})
	}
	return cases, nil
}
