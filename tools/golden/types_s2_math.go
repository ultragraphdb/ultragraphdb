// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"math"
	"math/rand"
)

type s2gMathGolden struct {
	Sin       [][2]string    `json:"sin"`
	Cos       [][2]string    `json:"cos"`
	Atan2     [][3]string    `json:"atan2"`
	Asin      [][2]string    `json:"asin"`
	Remainder [][3]string    `json:"remainder"`
	Ldexp     []s2gLdexpCase `json:"ldexp"`
	Frexp     []s2gFrexpCase `json:"frexp"`
	Ilogb     []s2gIlogbCase `json:"ilogb"`
	Nextafter [][3]string    `json:"nextafter"`
	Max       [][3]string    `json:"max"`
	Min       [][3]string    `json:"min"`
	Sqrt      [][2]string    `json:"sqrt"`
}

type s2gLdexpCase struct {
	Frac string `json:"frac"`
	Exp  int    `json:"exp"`
	Out  string `json:"out"`
}

type s2gFrexpCase struct {
	In   string `json:"in"`
	Frac string `json:"frac"`
	Exp  int    `json:"exp"`
}

type s2gIlogbCase struct {
	In  string `json:"in"`
	Out int    `json:"out"`
}

func s2gSpecials() []float64 {
	return []float64{0, math.Copysign(0, -1), 1, -1, 0.5, -0.5, 2, math.Pi, -math.Pi, math.Pi / 2, -math.Pi / 2,
		math.Pi / 4, 3 * math.Pi / 4, 2 * math.Pi, math.Inf(1), math.Inf(-1), math.NaN(),
		math.SmallestNonzeroFloat64, -math.SmallestNonzeroFloat64, math.MaxFloat64, -math.MaxFloat64,
		0x1p-1022, 0x1p-1030, 1e-300, 1e300, 1 << 29, 1<<29 + 0.5, 1e10, 1e22, 0x1p-52, 0.66, 0.7,
		2.41421356237309504880, 0.414213562373095, 1e-8, 1e-16}
}

func s2gGenMath(dir string) {
	r := rand.New(rand.NewSource(1))
	var g s2gMathGolden

	unary := func(f func(float64) float64, xs []float64) [][2]string {
		out := make([][2]string, 0, len(xs))
		for _, x := range xs {
			out = append(out, [2]string{s2gF(x), s2gF(f(x))})
		}
		return out
	}

	// Real argument ranges: latitudes and longitudes in radians, regular-loop angles
	// in [0, 2*pi), small cap radii, plus a sweep over magnitudes.
	var trig []float64
	trig = append(trig, s2gSpecials()...)
	for i := 0; i < 4000; i++ {
		trig = append(trig, (r.Float64()*2-1)*math.Pi)
	}
	for i := 0; i < 1000; i++ {
		trig = append(trig, r.Float64()*2*math.Pi)
	}
	for i := 0; i < 1000; i++ {
		trig = append(trig, (r.Float64()*2-1)*90*(math.Pi/180))
	}
	for i := 0; i < 1000; i++ {
		trig = append(trig, math.Ldexp(r.Float64(), -r.Intn(60)))
	}
	for i := 0; i < 500; i++ {
		trig = append(trig, (r.Float64()*2-1)*math.Ldexp(1, r.Intn(40)))
	}
	for i := 0; i < 200; i++ {
		trig = append(trig, (r.Float64()*2-1)*math.Ldexp(1, 29+r.Intn(900)))
	}
	// Multiples of the regular-loop step used by Dgraph's near query (100 vertices).
	step := 2 * math.Pi / float64(100)
	for i := 0; i < 100; i++ {
		trig = append(trig, float64(i)*step)
	}
	g.Sin = unary(math.Sin, trig)
	g.Cos = unary(math.Cos, trig)

	// Atan2 over the octants, signed zeros, infinities, and point-like inputs.
	sp := s2gSpecials()
	for _, y := range sp {
		for _, x := range sp {
			g.Atan2 = append(g.Atan2, [3]string{s2gF(y), s2gF(x), s2gF(math.Atan2(y, x))})
		}
	}
	for i := 0; i < 6000; i++ {
		var y, x float64
		switch i % 4 {
		case 0:
			y, x = r.Float64()*2-1, r.Float64()*2-1
		case 1:
			y, x = r.NormFloat64(), r.NormFloat64()*1e-6
		case 2:
			p := s2gRandomPoint(r)
			y, x = p.Z, math.Sqrt(float64(p.X*p.X)+float64(p.Y*p.Y))
		default:
			y, x = math.Ldexp(r.Float64()-0.5, -r.Intn(50)), math.Ldexp(r.Float64()-0.5, -r.Intn(50))
		}
		g.Atan2 = append(g.Atan2, [3]string{s2gF(y), s2gF(x), s2gF(math.Atan2(y, x))})
	}

	var asin []float64
	asin = append(asin, s2gSpecials()...)
	for i := 0; i < 4000; i++ {
		asin = append(asin, r.Float64()*2-1)
	}
	for i := 0; i < 1000; i++ {
		asin = append(asin, math.Ldexp(r.Float64(), -r.Intn(60)))
	}
	for i := 0; i < 500; i++ {
		asin = append(asin, 1-math.Ldexp(r.Float64(), -r.Intn(52)))
	}
	g.Asin = unary(math.Asin, asin)

	var sq []float64
	sq = append(sq, s2gSpecials()...)
	for i := 0; i < 1000; i++ {
		sq = append(sq, math.Ldexp(r.Float64(), r.Intn(200)-100))
	}
	g.Sqrt = unary(math.Sqrt, sq)

	ys := []float64{2 * math.Pi, math.Pi, -2 * math.Pi, 1, 0.5, 3, 1e-310, math.Inf(1), 0, math.NaN(), 1e300}
	for _, y := range ys {
		for _, x := range sp {
			g.Remainder = append(g.Remainder, [3]string{s2gF(x), s2gF(y), s2gF(math.Remainder(x, y))})
		}
	}
	for i := 0; i < 4000; i++ {
		var x, y float64
		switch i % 4 {
		case 0:
			x, y = (r.Float64()*2-1)*4*math.Pi, 2*math.Pi
		case 1:
			x, y = (r.Float64()*2-1)*math.Ldexp(1, r.Intn(1000)), 2*math.Pi
		case 2:
			x, y = r.NormFloat64()*100, r.Float64()*10
		default:
			x, y = math.Ldexp(r.Float64(), -1060+r.Intn(30)), math.Ldexp(r.Float64(), -1070+r.Intn(30))
		}
		g.Remainder = append(g.Remainder, [3]string{s2gF(x), s2gF(y), s2gF(math.Remainder(x, y))})
	}

	for _, x := range sp {
		for _, e := range []int{0, 1, -1, 52, -52, 563, -511, -1074, -1075, -1080, 1023, 1024, 2000, -2000} {
			g.Ldexp = append(g.Ldexp, s2gLdexpCase{s2gF(x), e, s2gF(math.Ldexp(x, e))})
		}
	}
	for i := 0; i < 2000; i++ {
		x := math.Ldexp(r.Float64()*2-1, r.Intn(2100)-1080)
		e := r.Intn(2200) - 1100
		g.Ldexp = append(g.Ldexp, s2gLdexpCase{s2gF(x), e, s2gF(math.Ldexp(x, e))})
	}

	var fr []float64
	fr = append(fr, sp...)
	for i := 0; i < 2000; i++ {
		fr = append(fr, math.Ldexp(r.Float64()*2-1, r.Intn(2150)-1100))
	}
	for _, x := range fr {
		f, e := math.Frexp(x)
		g.Frexp = append(g.Frexp, s2gFrexpCase{s2gF(x), s2gF(f), e})
		g.Ilogb = append(g.Ilogb, s2gIlogbCase{s2gF(x), math.Ilogb(x)})
	}

	for _, x := range sp {
		for _, y := range sp {
			g.Nextafter = append(g.Nextafter, [3]string{s2gF(x), s2gF(y), s2gF(math.Nextafter(x, y))})
			g.Max = append(g.Max, [3]string{s2gF(x), s2gF(y), s2gF(math.Max(x, y))})
			g.Min = append(g.Min, [3]string{s2gF(x), s2gF(y), s2gF(math.Min(x, y))})
		}
	}
	s2gWriteJSON(dir, "gomath.json", g)
}
