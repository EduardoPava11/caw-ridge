//! Sentinel-2 imagery: find the most recent clear pass over the ridge and read the four
//! 10 m bands (red, green, blue, near infrared) for a box, mosaicking neighbouring tiles.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::cog::{agent, Cog, Res};
use crate::config::BBox;
use crate::geo::Utm;

pub const STAC: &str = "https://earth-search.aws.element84.com/v1/search";

/// Four bands on a UTM grid. Values are surface reflectance times 10000, 0 = no data.
pub struct Sat {
    pub w: usize,
    pub h: usize,
    pub x0: f64,
    pub y0: f64,
    pub res: f64,
    pub zone: u32,
    /// red, green, blue, nir
    pub bands: [Vec<u16>; 4],
    pub date: String,
    pub scenes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Scene {
    pub id: String,
    pub datetime: String,
    pub cloud: f64,
    pub zone: u32,
    pub tile: String,
    pub offset_applied: bool,
    pub hrefs: [String; 4],
}

pub fn search(lon: f64, lat: f64, from: &str, to: &str, max_cloud: f64) -> Res<Vec<Scene>> {
    let body = serde_json::json!({
        "collections": ["sentinel-2-l2a"],
        "intersects": {"type": "Point", "coordinates": [lon, lat]},
        "datetime": format!("{from}T00:00:00Z/{to}T23:59:59Z"),
        "limit": 200,
        "query": {"eo:cloud_cover": {"lt": max_cloud}}
    });
    let mut resp = agent().post(STAC).header("Content-Type", "application/json").send(body.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&resp.body_mut().read_to_string()?)?;
    let mut out = Vec::new();
    for f in v["features"].as_array().cloned().unwrap_or_default() {
        let p = &f["properties"];
        let a = &f["assets"];
        let href = |k: &str| a[k]["href"].as_str().unwrap_or("").to_string();
        out.push(Scene {
            id: f["id"].as_str().unwrap_or("").to_string(),
            datetime: p["datetime"].as_str().unwrap_or("").to_string(),
            cloud: p["eo:cloud_cover"].as_f64().unwrap_or(100.0),
            zone: p["mgrs:utm_zone"].as_u64().unwrap_or(0) as u32,
            tile: format!(
                "{}{}{}",
                p["mgrs:utm_zone"].as_u64().unwrap_or(0),
                p["mgrs:latitude_band"].as_str().unwrap_or(""),
                p["mgrs:grid_square"].as_str().unwrap_or("")
            ),
            offset_applied: p["earthsearch:boa_offset_applied"].as_bool().unwrap_or(false),
            hrefs: [href("red"), href("green"), href("blue"), href("nir")],
        });
    }
    Ok(out)
}

/// Pick the newest day on which every tile that touches the box was clear, then list the
/// scenes of that day first and older clear scenes after them as gap fillers.
pub fn choose(scenes: &[Scene], zone: u32, tiles: &[&str]) -> Vec<Scene> {
    let mut by_day: BTreeMap<String, Vec<Scene>> = BTreeMap::new();
    for s in scenes.iter().filter(|s| s.zone == zone && tiles.contains(&s.tile.as_str())) {
        by_day.entry(s.datetime[..10].to_string()).or_default().push(s.clone());
    }
    let mut order: Vec<Scene> = Vec::new();
    let mut days: Vec<&String> = by_day.keys().collect();
    days.reverse();
    let mut started = false;
    for d in days {
        let v = &by_day[d];
        let complete = tiles.iter().all(|t| v.iter().any(|s| &s.tile == t));
        if !started && !complete {
            continue;
        }
        started = true;
        let mut v = v.clone();
        v.sort_by(|a, b| a.cloud.partial_cmp(&b.cloud).unwrap());
        order.extend(v);
        if order.len() >= 8 {
            break;
        }
    }
    order
}

impl Sat {
    pub fn fetch(bbox: BBox, res: f64, scenes: &[Scene]) -> Res<Sat> {
        let zone = scenes[0].zone;
        let utm = Utm::new(zone);
        let mut es = Vec::new();
        let mut ns = Vec::new();
        for i in 0..=20 {
            let t = i as f64 / 20.0;
            for (lon, lat) in [
                (bbox.w + t * (bbox.e - bbox.w), bbox.s),
                (bbox.w + t * (bbox.e - bbox.w), bbox.n),
                (bbox.w, bbox.s + t * (bbox.n - bbox.s)),
                (bbox.e, bbox.s + t * (bbox.n - bbox.s)),
            ] {
                let (e, n) = utm.forward(lon, lat);
                es.push(e);
                ns.push(n);
            }
        }
        let snap = |v: f64, up: bool| if up { (v / res).ceil() * res } else { (v / res).floor() * res };
        let x0 = snap(es.iter().cloned().fold(f64::MAX, f64::min), false);
        let x1 = snap(es.iter().cloned().fold(f64::MIN, f64::max), true);
        let y0 = snap(ns.iter().cloned().fold(f64::MIN, f64::max), true);
        let y1 = snap(ns.iter().cloned().fold(f64::MAX, f64::min), false);
        let w = ((x1 - x0) / res) as usize;
        let h = ((y0 - y1) / res) as usize;
        let mut bands: [Vec<u16>; 4] = [vec![0; w * h], vec![0; w * h], vec![0; w * h], vec![0; w * h]];
        let mut used = Vec::new();
        let mut date = String::new();
        for s in scenes {
            let missing = bands[0].iter().filter(|v| **v == 0).count();
            if missing == 0 {
                break;
            }
            eprintln!("  {} ({} cells still empty)", s.id, missing);
            let mut got = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
            let mut geo = (0.0, 0.0, 0.0, 0usize, 0usize);
            let mut ok = true;
            for b in 0..4 {
                let cog = Cog::open(&s.hrefs[b])?;
                let (tx, ty, sx, _) = cog.geo();
                // The coarsest pyramid level that is still at least as fine as the target.
                let mut level = 0;
                for (i, ifd) in cog.ifds.iter().enumerate() {
                    let r = sx * cog.ifds[0].width as f64 / ifd.width as f64;
                    if r <= res + 1e-6 {
                        level = i;
                    }
                }
                let lw = cog.ifds[level].width as f64;
                let lres = sx * cog.ifds[0].width as f64 / lw;
                let px0 = (((x0 - tx) / lres).floor() as i64).max(0);
                let py0 = (((ty - y0) / lres).floor() as i64).max(0);
                let px1 = (((x1 - tx) / lres).ceil() as i64).min(cog.ifds[level].width as i64);
                let py1 = (((ty - y1) / lres).ceil() as i64).min(cog.ifds[level].height as i64);
                if px1 <= px0 || py1 <= py0 {
                    ok = false;
                    break;
                }
                let (ww, hh) = ((px1 - px0) as usize, (py1 - py0) as usize);
                got[b] = cog.read_window(level, px0, py0, ww, hh)?;
                geo = (tx + px0 as f64 * lres, ty - py0 as f64 * lres, lres, ww, hh);
            }
            if !ok {
                continue;
            }
            let (gx0, gy0, gres, gw, gh) = geo;
            let off = if s.offset_applied { 0.0 } else { -1000.0 };
            let mut filled = 0usize;
            for y in 0..h {
                let n = y0 - (y as f64 + 0.5) * res;
                let sy = ((gy0 - n) / gres).floor() as i64;
                if sy < 0 || sy >= gh as i64 {
                    continue;
                }
                for x in 0..w {
                    let i = y * w + x;
                    if bands[0][i] != 0 {
                        continue;
                    }
                    let e = x0 + (x as f64 + 0.5) * res;
                    let sx = ((e - gx0) / gres).floor() as i64;
                    if sx < 0 || sx >= gw as i64 {
                        continue;
                    }
                    let j = sy as usize * gw + sx as usize;
                    if got[0][j].is_nan() || got[0][j] <= 0.0 || got[1][j] <= 0.0 {
                        continue;
                    }
                    for b in 0..4 {
                        bands[b][i] = (got[b][j] + off).clamp(1.0, 65535.0) as u16;
                    }
                    filled += 1;
                }
            }
            if filled > 0 {
                used.push(s.id.clone());
                if date.is_empty() {
                    date = s.datetime.clone();
                }
            }
        }
        Ok(Sat { w, h, x0, y0, res, zone, bands, date, scenes: used })
    }

    pub fn save(&self, dir: &Path, name: &str) -> Res<()> {
        let mut px = Vec::with_capacity(self.w * self.h * 4);
        for i in 0..self.w * self.h {
            for b in 0..4 {
                px.push(self.bands[b][i]);
            }
        }
        let img: image::ImageBuffer<image::Rgba<u16>, Vec<u16>> =
            image::ImageBuffer::from_raw(self.w as u32, self.h as u32, px).unwrap();
        img.save(dir.join(format!("{name}.png")))?;
        let meta = serde_json::json!({
            "crs": format!("UTM zone {} north, WGS84", self.zone),
            "encoding": "RGBA16 = red, green, blue, near infrared; reflectance x 10000; 0 = no data",
            "x0": self.x0, "y0": self.y0, "res": self.res, "w": self.w, "h": self.h, "zone": self.zone,
            "datetime": self.date, "scenes": self.scenes,
            "source": "Copernicus Sentinel-2 L2A, via Element 84 Earth Search on AWS"
        });
        fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&meta)?)?;
        Ok(())
    }

    pub fn load(dir: &Path, name: &str) -> Res<Sat> {
        let meta: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join(format!("{name}.json")))?)?;
        let img = image::open(dir.join(format!("{name}.png")))?.into_rgba16();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let raw = img.into_raw();
        let mut bands: [Vec<u16>; 4] = [vec![0; w * h], vec![0; w * h], vec![0; w * h], vec![0; w * h]];
        for i in 0..w * h {
            for b in 0..4 {
                bands[b][i] = raw[i * 4 + b];
            }
        }
        Ok(Sat {
            w,
            h,
            x0: meta["x0"].as_f64().unwrap(),
            y0: meta["y0"].as_f64().unwrap(),
            res: meta["res"].as_f64().unwrap(),
            zone: meta["zone"].as_u64().unwrap() as u32,
            bands,
            date: meta["datetime"].as_str().unwrap_or("").to_string(),
            scenes: meta["scenes"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default(),
        })
    }

    /// Bilinear sample of all four bands at an easting and northing; None off the image.
    pub fn sample(&self, e: f64, n: f64) -> Option<[f32; 4]> {
        let fx = (e - self.x0) / self.res - 0.5;
        let fy = (self.y0 - n) / self.res - 0.5;
        if fx < 0.0 || fy < 0.0 || fx >= (self.w - 1) as f64 || fy >= (self.h - 1) as f64 {
            return None;
        }
        let (ix, iy) = (fx as usize, fy as usize);
        let (tx, ty) = ((fx - ix as f64) as f32, (fy - iy as f64) as f32);
        let mut out = [0f32; 4];
        for b in 0..4 {
            let d = &self.bands[b];
            let (a, bb, c, dd) = (d[iy * self.w + ix], d[iy * self.w + ix + 1], d[(iy + 1) * self.w + ix], d[(iy + 1) * self.w + ix + 1]);
            if a == 0 || bb == 0 || c == 0 || dd == 0 {
                return None;
            }
            out[b] = (a as f32 * (1.0 - tx) + bb as f32 * tx) * (1.0 - ty) + (c as f32 * (1.0 - tx) + dd as f32 * tx) * ty;
        }
        Some(out)
    }
}
