//! The elevation model as it is stored on disk, and the sampler that every map reads from.

use std::fs;
use std::path::Path;

use crate::cog::{Cog, Res};
use crate::config::BBox;
use crate::geo::Lcc;

/// A window of the national 30 m model, still in its native Lambert grid.
pub struct SrcDem {
    pub w: usize,
    pub h: usize,
    /// Metres above sea level (CGVD2013), NaN where there is no data.
    pub z: Vec<f32>,
    /// Lambert x of the left edge and y of the top edge of pixel (0, 0).
    pub x0: f64,
    pub y0: f64,
    pub res: f64,
    pub proj: Lcc,
}

impl SrcDem {
    /// Download the window that covers `bbox` plus a margin, straight out of the COG.
    pub fn fetch(url: &str, bbox: BBox, margin_m: f64) -> Res<SrcDem> {
        let proj = Lcc::canada_atlas();
        let cog = Cog::open(url)?;
        let (tx, ty, sx, sy) = cog.geo();
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        // The box is curved in Lambert space, so walk its edge instead of trusting corners.
        for i in 0..=40 {
            let t = i as f64 / 40.0;
            for (lon, lat) in [
                (bbox.w + t * (bbox.e - bbox.w), bbox.s),
                (bbox.w + t * (bbox.e - bbox.w), bbox.n),
                (bbox.w, bbox.s + t * (bbox.n - bbox.s)),
                (bbox.e, bbox.s + t * (bbox.n - bbox.s)),
            ] {
                let (x, y) = proj.forward(lon, lat);
                xs.push(x);
                ys.push(y);
            }
        }
        let minx = xs.iter().cloned().fold(f64::MAX, f64::min) - margin_m;
        let maxx = xs.iter().cloned().fold(f64::MIN, f64::max) + margin_m;
        let miny = ys.iter().cloned().fold(f64::MAX, f64::min) - margin_m;
        let maxy = ys.iter().cloned().fold(f64::MIN, f64::max) + margin_m;
        let px0 = ((minx - tx) / sx).floor() as i64;
        let px1 = ((maxx - tx) / sx).ceil() as i64;
        let py0 = ((ty - maxy) / sy).floor() as i64;
        let py1 = ((ty - miny) / sy).ceil() as i64;
        let (w, h) = ((px1 - px0) as usize, (py1 - py0) as usize);
        eprintln!("  window {w} x {h} cells at {sx} m");
        let z = cog.read_window(0, px0, py0, w, h)?;
        Ok(SrcDem { w, h, z, x0: tx + px0 as f64 * sx, y0: ty - py0 as f64 * sy, res: sx, proj })
    }

    /// Stored as a 16 bit PNG in decimetres above a base height: small, lossless to 10 cm,
    /// and it opens in any image viewer.
    pub fn save(&self, dir: &Path, name: &str, source: &str) -> Res<()> {
        let base = -100.0f32;
        let mut px = Vec::with_capacity(self.w * self.h);
        for v in &self.z {
            px.push(if v.is_nan() { 0u16 } else { (((v - base) * 10.0).round() as i64).clamp(1, 65535) as u16 });
        }
        let img: image::ImageBuffer<image::Luma<u16>, Vec<u16>> =
            image::ImageBuffer::from_raw(self.w as u32, self.h as u32, px).unwrap();
        img.save(dir.join(format!("{name}.png")))?;
        let meta = serde_json::json!({
            "crs": "EPSG:3979 NAD83(CSRS) / Canada Atlas Lambert",
            "encoding": "value = pixel / 10 - 100, 0 = no data",
            "x0": self.x0, "y0": self.y0, "res": self.res, "w": self.w, "h": self.h,
            "source": source
        });
        fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&meta)?)?;
        Ok(())
    }

    pub fn load(dir: &Path, name: &str) -> Res<SrcDem> {
        let meta: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("{name}.json")))?)?;
        let img = image::open(dir.join(format!("{name}.png")))?.into_luma16();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let z = img.into_raw().iter().map(|&v| if v == 0 { f32::NAN } else { v as f32 / 10.0 - 100.0 }).collect();
        Ok(SrcDem {
            w,
            h,
            z,
            x0: meta["x0"].as_f64().unwrap(),
            y0: meta["y0"].as_f64().unwrap(),
            res: meta["res"].as_f64().unwrap(),
            proj: Lcc::canada_atlas(),
        })
    }

    #[inline]
    fn at(&self, x: i64, y: i64) -> f32 {
        let x = x.clamp(0, self.w as i64 - 1) as usize;
        let y = y.clamp(0, self.h as i64 - 1) as usize;
        self.z[y * self.w + x]
    }

    /// Nearest cell, for rasters that hold classes or dates rather than a surface.
    pub fn nearest(&self, lon: f64, lat: f64) -> f32 {
        let (x, y) = self.proj.forward(lon, lat);
        let fx = ((x - self.x0) / self.res).floor() as i64;
        let fy = ((self.y0 - y) / self.res).floor() as i64;
        if fx < 0 || fy < 0 || fx >= self.w as i64 || fy >= self.h as i64 {
            return f32::NAN;
        }
        self.z[fy as usize * self.w + fx as usize]
    }

    /// Elevation at a lon/lat by bicubic (Catmull-Rom) interpolation between cell centres.
    pub fn sample(&self, lon: f64, lat: f64) -> f32 {
        let (x, y) = self.proj.forward(lon, lat);
        let fx = (x - self.x0) / self.res - 0.5;
        let fy = (self.y0 - y) / self.res - 0.5;
        if fx < -1.0 || fy < -1.0 || fx > self.w as f64 || fy > self.h as f64 {
            return f32::NAN;
        }
        let ix = fx.floor() as i64;
        let iy = fy.floor() as i64;
        let tx = (fx - ix as f64) as f32;
        let ty = (fy - iy as f64) as f32;
        let cr = |p0: f32, p1: f32, p2: f32, p3: f32, t: f32| -> f32 {
            p1 + 0.5 * t * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
        };
        let mut rows = [0f32; 4];
        for (j, row) in rows.iter_mut().enumerate() {
            let yy = iy - 1 + j as i64;
            *row = cr(self.at(ix - 1, yy), self.at(ix, yy), self.at(ix + 1, yy), self.at(ix + 2, yy), tx);
        }
        cr(rows[0], rows[1], rows[2], rows[3], ty)
    }
}
