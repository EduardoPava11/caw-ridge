//! A map view: a north-up rectangle in Web Mercator with an elevation grid sampled into it.
//! Web Mercator is conformal, so a pixel is square on the ground; only its size changes with
//! latitude, by the cosine of the latitude.

use rayon::prelude::*;

use crate::config::BBox;
use crate::dem::SrcDem;
use crate::geo::{merc_forward, merc_inverse};

#[derive(Clone)]
pub struct View {
    pub bbox: BBox,
    pub w: usize,
    pub h: usize,
    /// Mercator x of the left edge, y of the top edge, metres per pixel in Mercator units.
    pub mx0: f64,
    pub my0: f64,
    pub mres: f64,
}

impl View {
    /// `ground_res` is metres per pixel on the ground at the middle of the box.
    pub fn new(bbox: BBox, ground_res: f64) -> View {
        let (mx0, my1) = merc_forward(bbox.w, bbox.s);
        let (mx1, my0) = merc_forward(bbox.e, bbox.n);
        let lat_mid = (bbox.s + bbox.n) / 2.0;
        let mres = ground_res / lat_mid.to_radians().cos();
        let w = ((mx1 - mx0) / mres).round() as usize;
        let h = ((my0 - my1) / mres).round() as usize;
        // Keep the box exact and let the pixel be very slightly non-square instead.
        View { bbox, w, h, mx0, my0, mres: (mx1 - mx0) / w as f64 }
    }

    pub fn yres(&self) -> f64 {
        let (_, my1) = merc_forward(self.bbox.w, self.bbox.s);
        (self.my0 - my1) / self.h as f64
    }

    /// Pixel coordinates (fractional, origin at the top left corner) of a lon/lat.
    pub fn px(&self, lon: f64, lat: f64) -> (f64, f64) {
        let (x, y) = merc_forward(lon, lat);
        ((x - self.mx0) / self.mres, (self.my0 - y) / self.yres())
    }

    pub fn lonlat(&self, px: f64, py: f64) -> (f64, f64) {
        merc_inverse(self.mx0 + px * self.mres, self.my0 - py * self.yres())
    }

    /// Ground metres per pixel on a given pixel row.
    pub fn ground_res(&self, row: f64) -> f64 {
        let (_, lat) = self.lonlat(0.0, row);
        self.mres * lat.to_radians().cos()
    }

    pub fn mid_ground_res(&self) -> f64 {
        self.ground_res(self.h as f64 / 2.0)
    }

    pub fn elevation(&self, dem: &SrcDem) -> Vec<f32> {
        let mut z = vec![0f32; self.w * self.h];
        z.par_chunks_mut(self.w).enumerate().for_each(|(y, row)| {
            for (x, v) in row.iter_mut().enumerate() {
                let (lon, lat) = self.lonlat(x as f64 + 0.5, y as f64 + 0.5);
                *v = dem.sample(lon, lat);
            }
        });
        z
    }
}

/// Everything derived from elevation that the maps draw.
pub struct Terrain {
    pub w: usize,
    pub h: usize,
    pub z: Vec<f32>,
    /// Slope in degrees.
    pub slope: Vec<f32>,
    /// Downslope direction in degrees clockwise from north; -1 where flat.
    pub aspect: Vec<f32>,
    /// Unit surface normal (east, north, up).
    pub normal: Vec<[f32; 3]>,
    /// Ground size of a cell on each row.
    pub cell: Vec<f32>,
}

impl Terrain {
    pub fn new(view: &View, z: Vec<f32>) -> Terrain {
        let (w, h) = (view.w, view.h);
        let cell: Vec<f32> = (0..h).map(|y| view.ground_res(y as f64 + 0.5) as f32).collect();
        let mut slope = vec![0f32; w * h];
        let mut aspect = vec![0f32; w * h];
        let mut normal = vec![[0f32; 3]; w * h];
        let zr = &z;
        let at = |x: i64, y: i64| -> f32 { zr[(y.clamp(0, h as i64 - 1) as usize) * w + x.clamp(0, w as i64 - 1) as usize] };
        slope
            .par_chunks_mut(w)
            .zip(aspect.par_chunks_mut(w))
            .zip(normal.par_chunks_mut(w))
            .enumerate()
            .for_each(|(y, ((srow, arow), nrow))| {
                let c = cell[y];
                for x in 0..w {
                    let (xi, yi) = (x as i64, y as i64);
                    // Horn's third order finite difference.
                    let dzdx = ((at(xi + 1, yi - 1) + 2.0 * at(xi + 1, yi) + at(xi + 1, yi + 1))
                        - (at(xi - 1, yi - 1) + 2.0 * at(xi - 1, yi) + at(xi - 1, yi + 1)))
                        / (8.0 * c);
                    // Rows run south, so north is towards smaller y.
                    let dzdy = ((at(xi - 1, yi - 1) + 2.0 * at(xi, yi - 1) + at(xi + 1, yi - 1))
                        - (at(xi - 1, yi + 1) + 2.0 * at(xi, yi + 1) + at(xi + 1, yi + 1)))
                        / (8.0 * c);
                    let g = (dzdx * dzdx + dzdy * dzdy).sqrt();
                    srow[x] = g.atan().to_degrees();
                    arow[x] = if g < 1e-4 { -1.0 } else { ((-dzdx).atan2(-dzdy).to_degrees() + 360.0) % 360.0 };
                    let l = (dzdx * dzdx + dzdy * dzdy + 1.0).sqrt();
                    nrow[x] = [-dzdx / l, -dzdy / l, 1.0 / l];
                }
            });
        Terrain { w, h, z, slope, aspect, normal, cell }
    }

    /// Lambert shading for a light at the given azimuth and altitude, 0..1.
    pub fn shade(&self, azimuth: f64, altitude: f64, exaggeration: f32) -> Vec<f32> {
        let (az, al) = (azimuth.to_radians(), altitude.to_radians());
        let l = [(az.sin() * al.cos()) as f32, (az.cos() * al.cos()) as f32, al.sin() as f32];
        self.normal
            .par_iter()
            .map(|n| {
                // Exaggerate by stretching the horizontal part of the normal.
                let (nx, ny, nz) = (n[0] * exaggeration, n[1] * exaggeration, n[2]);
                let k = (nx * nx + ny * ny + nz * nz).sqrt();
                ((nx * l[0] + ny * l[1] + nz * l[2]) / k).max(0.0)
            })
            .collect()
    }

    /// A relief shade that reads well everywhere: the classic north-west light, softened by
    /// three more lights so that slopes facing away from the main light keep their form.
    pub fn relief(&self, exaggeration: f32) -> Vec<f32> {
        let a = self.shade(315.0, 45.0, exaggeration);
        let b = self.shade(270.0, 40.0, exaggeration);
        let c = self.shade(0.0, 40.0, exaggeration);
        let d = self.shade(225.0, 35.0, exaggeration);
        (0..a.len()).into_par_iter().map(|i| 0.55 * a[i] + 0.18 * b[i] + 0.18 * c[i] + 0.09 * d[i]).collect()
    }
}

/// Bilinear resize of a single channel grid.
pub fn resize(src: &[f32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f32> {
    let mut out = vec![0f32; dw * dh];
    out.par_chunks_mut(dw).enumerate().for_each(|(y, row)| {
        let fy = ((y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5).clamp(0.0, sh as f32 - 1.0);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let ty = fy - y0 as f32;
        for (x, v) in row.iter_mut().enumerate() {
            let fx = ((x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5).clamp(0.0, sw as f32 - 1.0);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let tx = fx - x0 as f32;
            let a = src[y0 * sw + x0] * (1.0 - tx) + src[y0 * sw + x1] * tx;
            let b = src[y1 * sw + x0] * (1.0 - tx) + src[y1 * sw + x1] * tx;
            *v = a * (1.0 - ty) + b * ty;
        }
    });
    out
}
