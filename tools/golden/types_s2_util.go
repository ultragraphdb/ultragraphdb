// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"fmt"
	"math"
	"math/rand"

	"github.com/golang/geo/r1"
	"github.com/golang/geo/s1"
	"github.com/golang/geo/s2"
)

// F renders a float64 as the 16 hex digits of its IEEE-754 bits.
func s2gF(x float64) string { return fmt.Sprintf("%016x", math.Float64bits(x)) }

// P renders a point as three hex floats.
func s2gP(p s2.Point) [3]string { return [3]string{s2gF(p.X), s2gF(p.Y), s2gF(p.Z)} }

// C renders a cell id as 16 hex digits.
func s2gC(c s2.CellID) string { return fmt.Sprintf("%016x", uint64(c)) }

// R renders a lat/lng rect as [latLo, latHi, lngLo, lngHi].
func s2gR(r s2.Rect) [4]string {
	return [4]string{s2gF(r.Lat.Lo), s2gF(r.Lat.Hi), s2gF(r.Lng.Lo), s2gF(r.Lng.Hi)}
}

// I renders an r1 interval.
func s2gI(i r1.Interval) [2]string { return [2]string{s2gF(i.Lo), s2gF(i.Hi)} }

// S1 renders an s1 interval.
func s2gS1(i s1.Interval) [2]string { return [2]string{s2gF(i.Lo), s2gF(i.Hi)} }

// Cap is the JSON form of a cap.
type s2gCap struct {
	Center [3]string `json:"center"`
	Chord  string    `json:"chord"`
	Radius string    `json:"radius"`
	Height string    `json:"height"`
	Empty  bool      `json:"empty"`
	Full   bool      `json:"full"`
}

func s2gCapOf(c s2.Cap) s2gCap {
	// The chord angle is recovered from the height: Height() = 0.5 * radius, exact.
	return s2gCap{
		Center: s2gP(c.Center()),
		Chord:  s2gF(2 * c.Height()),
		Radius: s2gF(c.Radius().Radians()),
		Height: s2gF(c.Height()),
		Empty:  c.IsEmpty(),
		Full:   c.IsFull(),
	}
}

func s2gTokens(cu s2.CellUnion) []string {
	out := make([]string, 0, len(cu))
	for _, c := range cu {
		out = append(out, c.ToToken())
	}
	return out
}

func s2gWriteJSON(dir, name string, v any) {
	s2gOutputs[name] = v
}

// randomPoint mirrors golang/geo's test helper: a uniformly random unit vector.
func s2gRandomPoint(r *rand.Rand) s2.Point {
	return s2.PointFromCoords(r.Float64()*2-1, r.Float64()*2-1, r.Float64()*2-1)
}

// randomCellID returns a random valid cell at the given level, or at a random level if level < 0.
func s2gRandomCellID(r *rand.Rand, level int) s2.CellID {
	if level < 0 {
		level = r.Intn(s2.MaxLevel + 1)
	}
	face := r.Intn(s2.NumFaces)
	pos := r.Uint64() & ((1 << s2.PosBits) - 1)
	return s2.CellIDFromFacePosLevel(face, pos, level)
}

// perturb moves each coordinate by up to n ulps.
func s2gPerturb(r *rand.Rand, p s2.Point, n int) s2.Point {
	f := func(x float64) float64 {
		k := r.Intn(2*n+1) - n
		if k >= -4 && k <= 4 {
			for ; k > 0; k-- {
				x = math.Nextafter(x, math.Inf(1))
			}
			for ; k < 0; k++ {
				x = math.Nextafter(x, math.Inf(-1))
			}
			return x
		}
		return x + float64(k)*math.Ldexp(math.Abs(x)+0x1p-60, -52)
	}
	p.X, p.Y, p.Z = f(p.X), f(p.Y), f(p.Z)
	return p
}

// interpolate returns the normalized point at fraction t on the chord from a to b.
func s2gInterpolate(a, b s2.Point, t float64) s2.Point {
	v := a.Mul(1 - t).Add(b.Mul(t))
	return s2.Point{Vector: v.Normalize()}
}
