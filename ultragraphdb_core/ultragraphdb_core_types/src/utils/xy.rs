/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1, xy/cga.go (IsPointInRing), xy/internal/raycrossing/ray-crossing-counter.go and
 * xy/internal/robustdeterminate/robust_determinate.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// go-geom's `xy.IsPointInRing`: whether `p` lies in the interior or on the boundary of a ring of flat
/// coordinates with the given stride.
pub(crate) fn is_point_in_ring(stride: usize, p: [f64; 2], ring: &[f64]) -> bool {
    let mut counter = RayCrossingCounter {
        p,
        crossing_count: 0,
        on_segment: false,
    };
    let mut i = stride;
    while i < ring.len() {
        let p1 = [ring[i], ring[i + 1]];
        let p2 = [ring[i - stride], ring[i - stride + 1]];
        counter.count_segment(p1, p2);
        if counter.on_segment {
            return true;
        }
        i += stride;
    }
    counter.on_segment || counter.crossing_count % 2 == 1
}

struct RayCrossingCounter {
    p: [f64; 2],
    crossing_count: usize,
    on_segment: bool,
}

impl RayCrossingCounter {
    fn count_segment(&mut self, p1: [f64; 2], p2: [f64; 2]) {
        let p = self.p;
        if p1[0] < p[0] && p2[0] < p[0] {
            return;
        }
        if p[0] == p2[0] && p[1] == p2[1] {
            self.on_segment = true;
            return;
        }
        if p1[1] == p[1] && p2[1] == p[1] {
            let (minx, maxx) = if p1[0] > p2[0] {
                (p2[0], p1[0])
            } else {
                (p1[0], p2[0])
            };
            if p[0] >= minx && p[0] <= maxx {
                self.on_segment = true;
            }
            return;
        }
        if (p1[1] > p[1] && p2[1] <= p[1]) || (p2[1] > p[1] && p1[1] <= p[1]) {
            let (x1, y1) = (p1[0] - p[0], p1[1] - p[1]);
            let (x2, y2) = (p2[0] - p[0], p2[1] - p[1]);
            let mut sign = sign_of_det2x2(x1, y1, x2, y2);
            if sign == 0 {
                self.on_segment = true;
                return;
            }
            if y2 < y1 {
                sign = -sign;
            }
            if sign > 0 {
                self.crossing_count += 1;
            }
        }
    }
}

/// go-geom's `robustdeterminate.SignOfDet2x2`: the exact sign of `x1*y2 - y1*x2`.
fn sign_of_det2x2(x1: f64, y1: f64, x2: f64, y2: f64) -> i32 {
    let (mut x1, mut y1, mut x2, mut y2) = (x1, y1, x2, y2);
    if x1 == 0.0 || y2 == 0.0 {
        if y1 == 0.0 || x2 == 0.0 {
            return 0;
        } else if y1 > 0.0 {
            return if x2 > 0.0 { -1 } else { 1 };
        }
        return if x2 > 0.0 { 1 } else { -1 };
    }
    if y1 == 0.0 || x2 == 0.0 {
        if y2 > 0.0 {
            return if x1 > 0.0 { 1 } else { -1 };
        }
        return if x1 > 0.0 { -1 } else { 1 };
    }
    let mut sign = 1;
    if 0.0 < y1 {
        if 0.0 < y2 {
            if y1 > y2 {
                sign = -1;
                std::mem::swap(&mut x1, &mut x2);
                std::mem::swap(&mut y1, &mut y2);
            }
        } else if y1 <= -y2 {
            sign = -1;
            x2 = -x2;
            y2 = -y2;
        } else {
            let swap = x1;
            x1 = -x2;
            x2 = swap;
            let swap = y1;
            y1 = -y2;
            y2 = swap;
        }
    } else if 0.0 < y2 {
        if -y1 <= y2 {
            sign = -1;
            x1 = -x1;
            y1 = -y1;
        } else {
            let swap = -x1;
            x1 = x2;
            x2 = swap;
            let swap = -y1;
            y1 = y2;
            y2 = swap;
        }
    } else if y1 >= y2 {
        x1 = -x1;
        y1 = -y1;
        x2 = -x2;
        y2 = -y2;
    } else {
        sign = -1;
        let swap = -x1;
        x1 = -x2;
        x2 = swap;
        let swap = -y1;
        y1 = -y2;
        y2 = swap;
    }
    if 0.0 < x1 {
        if 0.0 < x2 {
            if x1 > x2 {
                return sign;
            }
        } else {
            return sign;
        }
    } else {
        if 0.0 < x2 {
            return -sign;
        }
        if x1 >= x2 {
            sign = -sign;
            x1 = -x1;
            x2 = -x2;
        } else {
            return -sign;
        }
    }
    loop {
        let k = (x2 / x1).floor();
        x2 -= k * x1;
        y2 -= k * y1;
        if y2 < 0.0 {
            return -sign;
        }
        if y2 > y1 {
            return sign;
        }
        if x1 > x2 + x2 {
            if y1 < y2 + y2 {
                return sign;
            }
        } else {
            if y1 > y2 + y2 {
                return -sign;
            }
            x2 = x1 - x2;
            y2 = y1 - y2;
            sign = -sign;
        }
        if y2 == 0.0 {
            return if x2 == 0.0 { 0 } else { -sign };
        }
        if x2 == 0.0 {
            return sign;
        }
        let k = (x1 / x2).floor();
        x1 -= k * x2;
        y1 -= k * y2;
        if y1 < 0.0 {
            return sign;
        }
        if y1 > y2 {
            return -sign;
        }
        if x2 > x1 + x1 {
            if y2 < y1 + y1 {
                return -sign;
            }
        } else {
            if y2 > y1 + y1 {
                return sign;
            }
            x1 = x2 - x1;
            y1 = y2 - y1;
            sign = -sign;
        }
        if y1 == 0.0 {
            return if x1 == 0.0 { 0 } else { sign };
        }
        if x1 == 0.0 {
            return -sign;
        }
    }
}
