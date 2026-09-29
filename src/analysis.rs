//! Terrain analysis on the 30 m grid: what you can see, where the sun lands, how long the
//! walking takes, and where the water runs.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use rayon::prelude::*;

use crate::view::Terrain;

const EARTH_R: f32 = 6371000.0;
/// Standard atmospheric refraction bends the line of sight down by about a seventh of the
/// earth's curvature, which lets you see a little further than geometry alone.
const REFRACTION: f32 = 0.13;

#[inline]
fn bilinear(t: &Terrain, x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, t.w as f32 - 1.001);
    let y = y.clamp(0.0, t.h as f32 - 1.001);
    let (x0, y0) = (x as usize, y as usize);
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let i = y0 * t.w + x0;
    (t.z[i] * (1.0 - tx) + t.z[i + 1] * tx) * (1.0 - ty) + (t.z[i + t.w] * (1.0 - tx) + t.z[i + t.w + 1] * tx) * ty
}

/// Is the ground at cell (tx, ty) visible from an eye `eye_h` metres above cell (ox, oy)?
/// `target_h` lifts the target, for an animal standing rather than the bare ground.
pub fn sees(t: &Terrain, ox: f32, oy: f32, eye_h: f32, tx: f32, ty: f32, target_h: f32) -> bool {
    let cell = t.cell[(oy as usize).min(t.h - 1)];
    let (dx, dy) = (tx - ox, ty - oy);
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 1.5 {
        return true;
    }
    let z0 = bilinear(t, ox, oy) + eye_h;
    let z1 = bilinear(t, tx, ty) + target_h;
    let curv = |d: f32| (d * cell).powi(2) / (2.0 * EARTH_R) * (1.0 - REFRACTION);
    let steps = dist.ceil() as usize;
    for s in 1..steps {
        let f = s as f32 / steps as f32;
        let d = dist * f;
        let ground = bilinear(t, ox + dx * f, oy + dy * f) - curv(d);
        let line = z0 + (z1 - curv(dist) - z0) * f;
        if ground > line {
            return false;
        }
    }
    true
}

/// Visibility of every cell within `radius_m` of the observer. 1 = seen, 0 = hidden,
/// NaN = beyond the radius.
pub fn viewshed(t: &Terrain, ox: f32, oy: f32, eye_h: f32, target_h: f32, radius_m: f32) -> Vec<f32> {
    let cell = t.cell[(oy as usize).min(t.h - 1)];
    let r = radius_m / cell;
    let mut out = vec![f32::NAN; t.w * t.h];
    out.par_chunks_mut(t.w).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            let (fx, fy) = (x as f32, y as f32);
            if (fx - ox).powi(2) + (fy - oy).powi(2) > r * r {
                continue;
            }
            *v = if sees(t, ox, oy, eye_h, fx, fy, target_h) { 1.0 } else { 0.0 };
        }
    });
    out
}

pub struct Vantage {
    pub x: usize,
    pub y: usize,
    pub elev: f32,
    /// Square kilometres of ground in view between the near and far range.
    pub seen_km2: f32,
    /// Share of the ground in range that is in view.
    pub share: f32,
}

/// Score candidate vantage points by how much ground they command.
/// Candidates are taken every `stride` cells inside the window (x0, y0, x1, y1);
/// targets every `tstride` cells between `near_m` and `far_m`.
#[allow(clippy::too_many_arguments)]
pub fn vantage_scores(
    t: &Terrain,
    win: (usize, usize, usize, usize),
    stride: usize,
    tstride: usize,
    near_m: f32,
    far_m: f32,
    eye_h: f32,
    target_h: f32,
) -> Vec<Vantage> {
    let mut cands = Vec::new();
    let mut y = win.1;
    while y < win.3 {
        let mut x = win.0;
        while x < win.2 {
            cands.push((x, y));
            x += stride;
        }
        y += stride;
    }
    cands
        .par_iter()
        .map(|&(cx, cy)| {
            // Let the glasser shuffle to the best spot within the stride: take the local high point.
            let (mut bx, mut by, mut bz) = (cx, cy, f32::MIN);
            for yy in cy.saturating_sub(stride / 2)..(cy + stride / 2 + 1).min(t.h) {
                for xx in cx.saturating_sub(stride / 2)..(cx + stride / 2 + 1).min(t.w) {
                    if t.z[yy * t.w + xx] > bz {
                        bz = t.z[yy * t.w + xx];
                        bx = xx;
                        by = yy;
                    }
                }
            }
            let cell = t.cell[by];
            let r = (far_m / cell) as i64;
            let rn = near_m / cell;
            let (mut seen, mut total) = (0u32, 0u32);
            let mut ty = -r;
            while ty <= r {
                let mut tx = -r;
                while tx <= r {
                    let d2 = (tx * tx + ty * ty) as f32;
                    let (px, py) = (bx as i64 + tx, by as i64 + ty);
                    if d2 <= (r * r) as f32 && d2 >= rn * rn && px >= 0 && py >= 0 && px < t.w as i64 && py < t.h as i64 {
                        total += 1;
                        if sees(t, bx as f32, by as f32, eye_h, px as f32, py as f32, target_h) {
                            seen += 1;
                        }
                    }
                    tx += tstride as i64;
                }
                ty += tstride as i64;
            }
            let a = (tstride as f32 * cell).powi(2) / 1e6;
            Vantage { x: bx, y: by, elev: bz, seen_km2: seen as f32 * a, share: if total > 0 { seen as f32 / total as f32 } else { 0.0 } }
        })
        .collect()
}

/// Sun exposure over one day. `sun` lists (hour of day, azimuth, altitude) samples with the
/// sun above the horizon. Returns per cell: first lit hour, last lit hour, hours lit.
/// Cells outside the window are NaN.
pub fn sun_day(t: &Terrain, win: (usize, usize, usize, usize), sun: &[(f32, f32, f32)], step_h: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let zmax = t.z.iter().cloned().fold(f32::MIN, f32::max);
    let n = t.w * t.h;
    let mut first = vec![f32::NAN; n];
    let mut last = vec![f32::NAN; n];
    let mut hours = vec![f32::NAN; n];
    first
        .par_chunks_mut(t.w)
        .zip(last.par_chunks_mut(t.w))
        .zip(hours.par_chunks_mut(t.w))
        .enumerate()
        .for_each(|(y, ((frow, lrow), hrow))| {
            if y < win.1 || y >= win.3 {
                return;
            }
            let cell = t.cell[y];
            for x in win.0..win.2 {
                let z0 = t.z[y * t.w + x] + 1.0;
                let nrm = t.normal[y * t.w + x];
                let mut f = f32::NAN;
                let mut l = f32::NAN;
                let mut hsum = 0.0;
                for &(hr, az, alt) in sun {
                    let (azr, altr) = (az.to_radians(), alt.to_radians());
                    // The slope itself must face the sun.
                    let sv = [azr.sin() * altr.cos(), azr.cos() * altr.cos(), altr.sin()];
                    if nrm[0] * sv[0] + nrm[1] * sv[1] + nrm[2] * sv[2] <= 0.0 {
                        continue;
                    }
                    let (sx, sy) = (azr.sin(), -azr.cos());
                    let tan = altr.tan();
                    let mut d = 1.5f32;
                    let mut lit = true;
                    loop {
                        let (px, py) = (x as f32 + sx * d, y as f32 + sy * d);
                        if px < 0.0 || py < 0.0 || px >= t.w as f32 - 1.0 || py >= t.h as f32 - 1.0 {
                            break;
                        }
                        let ray = z0 + d * cell * tan + (d * cell).powi(2) / (2.0 * EARTH_R) * (1.0 - REFRACTION);
                        if ray > zmax {
                            break;
                        }
                        if bilinear(t, px, py) > ray {
                            lit = false;
                            break;
                        }
                        d += 1.0 + d * 0.02;
                    }
                    if lit {
                        if f.is_nan() {
                            f = hr;
                        }
                        l = hr;
                        hsum += step_h;
                    }
                }
                frow[x] = f;
                lrow[x] = l;
                hrow[x] = hsum;
            }
        });
    (first, last, hours)
}

#[derive(PartialEq)]
struct Node(f32, u32);
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Tobler's hiking function: walking speed in km/h on a slope of `s` (rise over run, signed,
/// positive uphill). It peaks at 6 km/h on a gentle downhill of about 3 degrees.
pub fn tobler(s: f32) -> f32 {
    6.0 * (-3.5 * (s + 0.05).abs()).exp()
}

/// Walking time in hours from a start cell to every cell in the window, by Dijkstra over
/// the eight neighbours. `factor` scales the speed per cell (1 on a track, less off it).
/// With `reverse` the times are for walking towards the start instead of away from it.
pub fn travel_time(t: &Terrain, win: (usize, usize, usize, usize), start: (usize, usize), factor: &[f32], reverse: bool) -> (Vec<f32>, Vec<u32>) {
    let mut time = vec![f32::INFINITY; t.w * t.h];
    let mut prev = vec![u32::MAX; t.w * t.h];
    let mut heap = BinaryHeap::new();
    let s = start.1 * t.w + start.0;
    time[s] = 0.0;
    heap.push(Node(0.0, s as u32));
    const NB: [(i64, i64, f32); 16] = [
        (1, 0, 1.0), (-1, 0, 1.0), (0, 1, 1.0), (0, -1, 1.0),
        (1, 1, 1.41421), (1, -1, 1.41421), (-1, 1, 1.41421), (-1, -1, 1.41421),
        // Knight moves soften the eight-direction grid bias.
        (2, 1, 2.23607), (2, -1, 2.23607), (-2, 1, 2.23607), (-2, -1, 2.23607),
        (1, 2, 2.23607), (1, -2, 2.23607), (-1, 2, 2.23607), (-1, -2, 2.23607),
    ];
    while let Some(Node(tm, i)) = heap.pop() {
        let i = i as usize;
        if tm > time[i] {
            continue;
        }
        let (x, y) = ((i % t.w) as i64, (i / t.w) as i64);
        for (dx, dy, dl) in NB {
            let (nx, ny) = (x + dx, y + dy);
            if nx < win.0 as i64 || ny < win.1 as i64 || nx >= win.2 as i64 || ny >= win.3 as i64 {
                continue;
            }
            let j = ny as usize * t.w + nx as usize;
            let run = dl * t.cell[y as usize];
            let rise = if reverse { t.z[i] - t.z[j] } else { t.z[j] - t.z[i] };
            let f = 0.5 * (factor[i] + factor[j]);
            if f <= 0.0 {
                continue;
            }
            let v = tobler(rise / run) * f;
            let nt = tm + (run / 1000.0) / v;
            if nt < time[j] {
                time[j] = nt;
                prev[j] = i as u32;
                heap.push(Node(nt, j as u32));
            }
        }
    }
    (time, prev)
}

/// Drainage: fill the pits (priority flood), send each cell's water to its lowest neighbour,
/// and count the upstream area. Returns (downstream index or u32::MAX, upstream km2).
pub fn drainage(t: &Terrain) -> (Vec<u32>, Vec<f32>) {
    let (w, h) = (t.w, t.h);
    let n = w * h;
    let mut filled = t.z.clone();
    let mut done = vec![false; n];
    let mut down = vec![u32::MAX; n];
    let mut heap = BinaryHeap::new();
    for y in 0..h {
        for x in 0..w {
            if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
                let i = y * w + x;
                done[i] = true;
                heap.push(Node(filled[i], i as u32));
            }
        }
    }
    let mut order = Vec::with_capacity(n);
    while let Some(Node(zc, i)) = heap.pop() {
        let i = i as usize;
        order.push(i as u32);
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if done[j] {
                    continue;
                }
                done[j] = true;
                // Water in a pit leaves by the cell that reached it first.
                down[j] = i as u32;
                filled[j] = filled[j].max(zc + 1e-3);
                heap.push(Node(filled[j], j as u32));
            }
        }
    }
    // Outside pits, prefer true steepest descent over the flood order.
    for i in 0..n {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        let mut best = (0f32, u32::MAX);
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                let drop = (t.z[i] - t.z[j]) / ((dx * dx + dy * dy) as f32).sqrt();
                if drop > best.0 {
                    best = (drop, j as u32);
                }
            }
        }
        if best.1 != u32::MAX && (filled[i] - t.z[i]).abs() < 1e-2 {
            down[i] = best.1;
        }
    }
    // Accumulate from the top of the catchment down: sort by filled height, highest first.
    let mut idx: Vec<u32> = (0..n as u32).collect();
    idx.par_sort_unstable_by(|a, b| filled[*b as usize].partial_cmp(&filled[*a as usize]).unwrap_or(Ordering::Equal));
    let mut acc: Vec<f32> = (0..n).map(|i| (t.cell[i / w] * t.cell[i / w]) / 1e6).collect();
    for &i in &idx {
        let d = down[i as usize];
        if d != u32::MAX {
            let a = acc[i as usize];
            acc[d as usize] += a;
        }
    }
    let _ = order;
    (down, acc)
}

/// Turn the drainage grid into stream lines: every cell with more than `min_km2` upstream.
/// Returns polylines in grid coordinates with the upstream area at their lower end.
pub fn streams(t: &Terrain, down: &[u32], acc: &[f32], min_km2: f32) -> Vec<(Vec<(f32, f32)>, f32)> {
    let (w, n) = (t.w, t.w * t.h);
    let is = |i: usize| acc[i] >= min_km2;
    let mut inflow = vec![0u8; n];
    for i in 0..n {
        if is(i) && down[i] != u32::MAX && is(down[i] as usize) {
            inflow[down[i] as usize] = inflow[down[i] as usize].saturating_add(1);
        }
    }
    let mut out = Vec::new();
    for i in 0..n {
        // A reach starts at a channel head or just below a confluence.
        if !is(i) || inflow[i] == 1 {
            continue;
        }
        let mut pts = vec![((i % w) as f32 + 0.5, (i / w) as f32 + 0.5)];
        let mut c = i;
        loop {
            let d = down[c];
            if d == u32::MAX {
                break;
            }
            let d = d as usize;
            pts.push(((d % w) as f32 + 0.5, (d / w) as f32 + 0.5));
            c = d;
            if inflow[c] != 1 {
                break;
            }
        }
        if pts.len() >= 2 {
            out.push((pts, acc[c]));
        }
    }
    out
}
