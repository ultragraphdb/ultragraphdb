// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_types' big floats: Dgraph parses a bigfloat with big.Float.UnmarshalText at
// 200 bits and writes it with MarshalText (binary and JSON form) and String (Text('g', 10)). The inputs are
// hand-picked syntax cases, random decimals, exact midpoints between adjacent 200-bit values (parse stress) and
// powers of two written as hex literals (formatting stress), all through types.Convert.

import (
	"fmt"
	"math/big"
	"math/rand"

	"github.com/dgraph-io/dgraph/v25/types"
)

func init() {
	register("types_bigfloat", "ultragraphdb_core_types", genTypesBigFloat)
}

type bigFloatCase struct {
	Kind string `json:"kind"`
	In   string `json:"in"`
	Err  string `json:"err,omitempty"`
	P    string `json:"p,omitempty"`    // exact value, Text('p', 0)
	Text string `json:"text,omitempty"` // MarshalText, Dgraph's binary form
	Str  string `json:"str,omitempty"`  // String(), Dgraph's string form
}

func bigFloatInputs() (kinds, ins []string) {
	add := func(kind string, s ...string) {
		for _, v := range s {
			kinds = append(kinds, kind)
			ins = append(ins, v)
		}
	}
	add("hand", "1", "0", "-0", "0.1", "1.5", "-42.125", "100000", "1000000", "1234567", "0.0001", "0.00001",
		"2.5e-7", "1e21", "1e300", "1e-300", "1e1000", "-1e-1000", "123456.789", "123456789012345678901234567890",
		"3.14159265358979323846264338327950288419716939937510582097494459",
		"0.30000000000000000000000000000000000000000000000000000000000001",
		"9999999999999999999999999999999999999999999999999999999999999999",
		"+5", "1_000", "0x1.8p1", "0b101", "Inf", "-Inf", "inf", "NaN", "abc", " 1", "1e", "", "1.", ".5")
	add("syntax", "0x1p-2", "0X1P+2", "0x.8", "0x1e5", "0xa.bp3", "0b1.1", "0B11p-1", "0o17", "0O7.4", "017",
		"0o", "0x", "0b", "0x_1", "0_1", "1_0", "1__0", "_1", "1_", "1_.5", "1._5", "0x_", "1e1_0", "1e_10",
		"1e10_", "1p3", "1P-3", "1e+5", "1E-5", "1e", "1e+", "e5", ".", "-", "+", "+Inf", "-inf", "+inf", "INF",
		"Infinity", "nan", "-NaN", "1.5.5", "1e5e5", "0x1.8", "0x1.8e1", "0b102", "0o8", "12a", "1 ", "1,5",
		"1e-3000000000", "1e2147483648", "-0x1p-1", "0.000000000000000000000000000000000000000000000001",
		"١", "1\x00")
	// Values whose binary exponent is near Go's int32 limits parse in Go, but formatting them takes Go hours
	// (decimal expansion of 2^±2^31); only their parse outcome is recorded.
	add("extreme", "1e1000000000", "1e2147483647", "0x1p2147483647", "0x1p-2147483648", "1e-646456993",
		"1e-646456994", "1e646456993")
	r := rand.New(rand.NewSource(42))
	digits := "0123456789"
	for i := 0; i < 2000; i++ {
		n := 1 + r.Intn(70)
		b := make([]byte, n)
		for j := range b {
			b[j] = digits[r.Intn(10)]
		}
		s := string(b)
		if r.Intn(2) == 0 && n > 1 {
			k := 1 + r.Intn(n-1)
			s = s[:k] + "." + s[k:]
		}
		if r.Intn(3) == 0 {
			s = fmt.Sprintf("%se%d", s, r.Intn(800)-400)
		}
		if r.Intn(4) == 0 {
			s = "-" + s
		}
		add("random", s)
	}
	// Exact midpoints between adjacent 200-bit values, written as exact decimals.
	for i := 0; i < 400; i++ {
		m := new(big.Int).Rand(r, new(big.Int).Lsh(big.NewInt(1), 199))
		m.SetBit(m, 199, 1)
		e := -r.Intn(1100) - 1
		num := new(big.Int).Add(new(big.Int).Lsh(m, 1), big.NewInt(1))
		den := new(big.Int).Lsh(big.NewInt(1), uint(1-e))
		s := new(big.Rat).SetFrac(num, den).FloatString(1 - e)
		if r.Intn(2) == 0 {
			s = "-" + s
		}
		add("midpoint", s)
	}
	// Powers of two, where Go's shortest formatting uses a symmetric interval.
	for k := -1100; k <= 1100; k += 7 {
		add("pow2", fmt.Sprintf("0x1p%d", k))
	}
	add("pow2", "0x1p-1100", "0x1p-1", "0x1p0", "0x1p1", "0x1p64", "0x1p200", "0x1p-200")
	// Values just beside powers of two and at Go's rounding boundaries for 'g' with 10 digits.
	add("near", "0.99999999995", "0.999999999949", "9999999999.5", "99999.999995", "0.000099999999995",
		"123456789.05", "123456789.15", "1234567890.5", "12345678905", "12345678915", "1.0000000005",
		"1.0000000015", "0.00012345678905")
	return
}

func genTypesBigFloat() (any, error) {
	kinds, ins := bigFloatInputs()
	var cases []bigFloatCase
	for i, in := range ins {
		c := bigFloatCase{Kind: kinds[i], In: in}
		out, err := types.Convert(types.Val{Tid: types.StringID, Value: []byte(in)}, types.BigFloatID)
		if err != nil {
			c.Err = err.Error()
			cases = append(cases, c)
			continue
		}
		f := out.Value.(big.Float)
		if f.Prec() != types.BigFloatPrecision {
			return nil, fmt.Errorf("precision %d for %q", f.Prec(), in)
		}
		c.P = f.Text('p', 0)
		if c.Kind == "extreme" {
			cases = append(cases, c)
			continue
		}
		bin, err := types.Convert(types.Val{Tid: types.BigFloatID, Value: []byte(in)}, types.BinaryID)
		if err != nil {
			return nil, err
		}
		c.Text = string(bin.Value.([]byte))
		str, err := types.Convert(types.Val{Tid: types.BigFloatID, Value: []byte(in)}, types.StringID)
		if err != nil {
			return nil, err
		}
		c.Str = str.Value.(string)
		cases = append(cases, c)
	}
	return cases, nil
}
