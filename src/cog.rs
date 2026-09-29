//! A small Cloud Optimized GeoTIFF reader that works over HTTP range requests.
//!
//! The national elevation model is one file of several hundred gigabytes. A COG keeps an
//! index of 512 x 512 tiles at the front of the file, so reading a window means: fetch the
//! header, look up the byte ranges of the tiles that touch the window, fetch only those.

use std::io::Read;
use std::time::Duration;

use rayon::prelude::*;

pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const UA: &str = "cawridge-sitegen/0.1 (personal trip planning site generator)";

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(300)))
        .user_agent(UA)
        .build()
        .into()
}

pub fn get_range(url: &str, start: u64, len: u64) -> Res<Vec<u8>> {
    let mut last: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for attempt in 0..4 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(800 * attempt));
        }
        let r = agent()
            .get(url)
            .header("Range", &format!("bytes={}-{}", start, start + len - 1))
            .call();
        match r {
            Ok(mut resp) => match resp.body_mut().with_config().limit(1 << 30).read_to_vec() {
                Ok(v) => {
                    if v.len() as u64 >= len {
                        return Ok(v[..len as usize].to_vec());
                    }
                    // Short read near the end of the file is fine for header probes.
                    return Ok(v);
                }
                Err(e) => last = Some(Box::new(e)),
            },
            Err(e) => last = Some(Box::new(e)),
        }
    }
    Err(last.unwrap())
}

pub fn get(url: &str) -> Res<Vec<u8>> {
    let mut last: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for attempt in 0..4 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(1500 * attempt));
        }
        match agent().get(url).call() {
            Ok(mut resp) => match resp.body_mut().with_config().limit(1 << 30).read_to_vec() {
                Ok(v) => return Ok(v),
                Err(e) => last = Some(Box::new(e)),
            },
            Err(e) => last = Some(Box::new(e)),
        }
    }
    Err(last.unwrap())
}

#[derive(Debug, Clone)]
pub struct Ifd {
    pub width: u64,
    pub height: u64,
    pub tile_w: u64,
    pub tile_h: u64,
    pub bits: u16,
    pub sample_format: u16,
    pub compression: u16,
    pub predictor: u16,
    pub samples: u16,
    pub nodata: Option<f64>,
    offsets_at: u64,
    counts_at: u64,
    offsets_type: u16,
    counts_type: u16,
    n_tiles: u64,
    /// ModelPixelScale and ModelTiepoint, present on the full resolution image.
    pub pixel_scale: Option<(f64, f64)>,
    pub tiepoint: Option<(f64, f64)>,
}

pub struct Cog {
    pub url: String,
    pub ifds: Vec<Ifd>,
    little: bool,
}

struct Cur<'a> {
    b: &'a [u8],
    little: bool,
}

impl<'a> Cur<'a> {
    fn u16(&self, o: usize) -> u16 {
        let v = [self.b[o], self.b[o + 1]];
        if self.little { u16::from_le_bytes(v) } else { u16::from_be_bytes(v) }
    }
    fn u32(&self, o: usize) -> u32 {
        let v: [u8; 4] = self.b[o..o + 4].try_into().unwrap();
        if self.little { u32::from_le_bytes(v) } else { u32::from_be_bytes(v) }
    }
    fn u64(&self, o: usize) -> u64 {
        let v: [u8; 8] = self.b[o..o + 8].try_into().unwrap();
        if self.little { u64::from_le_bytes(v) } else { u64::from_be_bytes(v) }
    }
    fn f64(&self, o: usize) -> f64 {
        f64::from_bits(self.u64(o))
    }
}

fn type_size(t: u16) -> u64 {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16 | 17 | 18 => 8,
        _ => 1,
    }
}

impl Cog {
    pub fn open(url: &str) -> Res<Cog> {
        let mut head = get_range(url, 0, 1 << 17)?;
        let little = &head[0..2] == b"II";
        let c = Cur { b: &head, little };
        let magic = c.u16(2);
        let big = magic == 43;
        if magic != 42 && magic != 43 {
            return Err(format!("not a TIFF: magic {magic}").into());
        }
        let mut ifd_off = if big { c.u64(8) } else { c.u32(4) as u64 };
        let mut ifds = Vec::new();
        let mut base: u64 = 0; // file offset of head[0]
        while ifd_off != 0 {
            // Make sure the directory is inside the buffer we hold.
            if ifd_off < base || ifd_off + 4096 > base + head.len() as u64 {
                head = get_range(url, ifd_off, 1 << 16)?;
                base = ifd_off;
            }
            let c = Cur { b: &head, little };
            let o = (ifd_off - base) as usize;
            let (n, esz, hdr) = if big { (c.u64(o), 20usize, 8usize) } else { (c.u16(o) as u64, 12usize, 2usize) };
            let mut ifd = Ifd {
                width: 0,
                height: 0,
                tile_w: 0,
                tile_h: 0,
                bits: 8,
                sample_format: 1,
                compression: 1,
                predictor: 1,
                samples: 1,
                nodata: None,
                offsets_at: 0,
                counts_at: 0,
                offsets_type: 0,
                counts_type: 0,
                n_tiles: 0,
                pixel_scale: None,
                tiepoint: None,
            };
            for i in 0..n as usize {
                let e = o + hdr + i * esz;
                let tag = c.u16(e);
                let typ = c.u16(e + 2);
                let (cnt, vo) = if big { (c.u64(e + 4), e + 12) } else { (c.u32(e + 4) as u64, e + 8) };
                let inline_cap = if big { 8 } else { 4 };
                let inline = cnt * type_size(typ) <= inline_cap;
                let ptr = if big { c.u64(vo) } else { c.u32(vo) as u64 };
                let scalar = || -> u64 {
                    match typ {
                        3 => c.u16(vo) as u64,
                        4 => c.u32(vo) as u64,
                        16 => c.u64(vo),
                        _ => c.u16(vo) as u64,
                    }
                };
                // Read a small out of line value, fetching it if it lies outside the buffer.
                let blob = |len: u64| -> Res<Vec<u8>> {
                    if inline {
                        return Ok(head[vo..vo + len as usize].to_vec());
                    }
                    if ptr >= base && ptr + len <= base + head.len() as u64 {
                        let s = (ptr - base) as usize;
                        Ok(head[s..s + len as usize].to_vec())
                    } else {
                        get_range(url, ptr, len)
                    }
                };
                match tag {
                    256 => ifd.width = scalar(),
                    257 => ifd.height = scalar(),
                    258 => ifd.bits = scalar() as u16,
                    259 => ifd.compression = scalar() as u16,
                    277 => ifd.samples = scalar() as u16,
                    317 => ifd.predictor = scalar() as u16,
                    322 => ifd.tile_w = scalar(),
                    323 => ifd.tile_h = scalar(),
                    339 => ifd.sample_format = scalar() as u16,
                    324 => {
                        ifd.offsets_at = if inline { base + vo as u64 } else { ptr };
                        ifd.offsets_type = typ;
                        ifd.n_tiles = cnt;
                    }
                    325 => {
                        ifd.counts_at = if inline { base + vo as u64 } else { ptr };
                        ifd.counts_type = typ;
                    }
                    33550 => {
                        let b = blob(24)?;
                        let cc = Cur { b: &b, little };
                        ifd.pixel_scale = Some((cc.f64(0), cc.f64(8)));
                    }
                    33922 => {
                        let b = blob(48)?;
                        let cc = Cur { b: &b, little };
                        ifd.tiepoint = Some((cc.f64(24), cc.f64(32)));
                    }
                    42113 => {
                        let b = blob(cnt)?;
                        let s = String::from_utf8_lossy(&b);
                        ifd.nodata = s.trim_matches(char::from(0)).trim().parse::<f64>().ok();
                    }
                    _ => {}
                }
            }
            let next = o + hdr + n as usize * esz;
            ifd_off = if big { c.u64(next) } else { c.u32(next) as u64 };
            if ifd.tile_w > 0 {
                ifds.push(ifd);
            }
        }
        Ok(Cog { url: url.to_string(), ifds, little })
    }

    /// Georeferencing of level 0: x of the left edge, y of the top edge, pixel size.
    pub fn geo(&self) -> (f64, f64, f64, f64) {
        let i = &self.ifds[0];
        let (sx, sy) = i.pixel_scale.expect("no pixel scale");
        let (tx, ty) = i.tiepoint.expect("no tiepoint");
        (tx, ty, sx, sy)
    }

    /// Read a pixel window (x0, y0, w, h) of one directory as f32. Pixels outside the image
    /// or equal to the nodata value come back as NaN.
    pub fn read_window(&self, level: usize, x0: i64, y0: i64, w: usize, h: usize) -> Res<Vec<f32>> {
        let ifd = &self.ifds[level];
        let (tw, th) = (ifd.tile_w as i64, ifd.tile_h as i64);
        let across = (ifd.width as i64 + tw - 1) / tw;
        let down = (ifd.height as i64 + th - 1) / th;
        let tx0 = (x0.max(0)) / tw;
        let ty0 = (y0.max(0)) / th;
        let tx1 = ((x0 + w as i64 - 1).min(ifd.width as i64 - 1)) / tw;
        let ty1 = ((y0 + h as i64 - 1).min(ifd.height as i64 - 1)) / th;
        let mut out = vec![f32::NAN; w * h];
        if tx1 < tx0 || ty1 < ty0 {
            return Ok(out);
        }
        let first = (ty0 * across + tx0) as u64;
        let last = (ty1 * across + tx1) as u64;
        let _ = down;
        let osz = type_size(ifd.offsets_type);
        let csz = type_size(ifd.counts_type);
        let ob = get_range(&self.url, ifd.offsets_at + first * osz, (last - first + 1) * osz)?;
        let cb = get_range(&self.url, ifd.counts_at + first * csz, (last - first + 1) * csz)?;
        let little = self.little;
        let rd = |b: &[u8], i: usize, sz: u64| -> u64 {
            let c = Cur { b, little };
            match sz {
                2 => c.u16(i * 2) as u64,
                4 => c.u32(i * 4) as u64,
                _ => c.u64(i * 8),
            }
        };
        let mut jobs = Vec::new();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let idx = (ty * across + tx) as u64 - first;
                let off = rd(&ob, idx as usize, osz);
                let cnt = rd(&cb, idx as usize, csz);
                jobs.push((tx, ty, off, cnt));
            }
        }
        let tiles: Vec<(i64, i64, Vec<f32>)> = jobs
            .par_iter()
            .map(|&(tx, ty, off, cnt)| -> Res<(i64, i64, Vec<f32>)> {
                if cnt == 0 {
                    return Ok((tx, ty, vec![f32::NAN; (tw * th) as usize]));
                }
                let raw = get_range(&self.url, off, cnt)?;
                Ok((tx, ty, decode_tile(ifd, &raw, little)?))
            })
            .collect::<Res<Vec<_>>>()?;
        for (tx, ty, px) in tiles {
            for r in 0..th {
                let gy = ty * th + r - y0;
                if gy < 0 || gy >= h as i64 {
                    continue;
                }
                for c in 0..tw {
                    let gx = tx * tw + c - x0;
                    if gx < 0 || gx >= w as i64 {
                        continue;
                    }
                    out[gy as usize * w + gx as usize] = px[(r * tw + c) as usize];
                }
            }
        }
        Ok(out)
    }
}

fn decode_tile(ifd: &Ifd, raw: &[u8], little: bool) -> Res<Vec<f32>> {
    let (tw, th) = (ifd.tile_w as usize, ifd.tile_h as usize);
    let bps = (ifd.bits / 8) as usize;
    let want = tw * th * bps;
    let mut buf: Vec<u8> = match ifd.compression {
        1 => raw.to_vec(),
        5 => {
            let mut d = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
            let mut out = Vec::with_capacity(want);
            let r = d.into_vec(&mut out).decode_all(raw);
            r.status?;
            out
        }
        8 | 32946 => {
            let mut out = Vec::with_capacity(want);
            flate2::read::ZlibDecoder::new(raw).read_to_end(&mut out)?;
            out
        }
        c => return Err(format!("unsupported compression {c}").into()),
    };
    if buf.len() < want {
        buf.resize(want, 0);
    }
    let nodata = ifd.nodata;
    let mut out = vec![0f32; tw * th];
    match ifd.predictor {
        3 => {
            // Floating point predictor: bytes are stored plane by plane, most significant
            // first, and differenced along the row.
            let mut row = vec![0u8; tw * bps];
            for r in 0..th {
                let src = &mut buf[r * tw * bps..(r + 1) * tw * bps];
                for i in 1..src.len() {
                    src[i] = src[i].wrapping_add(src[i - 1]);
                }
                row.copy_from_slice(src);
                for c in 0..tw {
                    let v = [row[c], row[tw + c], row[2 * tw + c], row[3 * tw + c]];
                    out[r * tw + c] = f32::from_be_bytes(v);
                }
            }
        }
        p => {
            let rdv = |b: &[u8], i: usize| -> f64 {
                match (bps, ifd.sample_format) {
                    (1, _) => b[i] as f64,
                    (2, 2) => {
                        let v = [b[i * 2], b[i * 2 + 1]];
                        (if little { i16::from_le_bytes(v) } else { i16::from_be_bytes(v) }) as f64
                    }
                    (2, _) => {
                        let v = [b[i * 2], b[i * 2 + 1]];
                        (if little { u16::from_le_bytes(v) } else { u16::from_be_bytes(v) }) as f64
                    }
                    (4, 3) => {
                        let v: [u8; 4] = b[i * 4..i * 4 + 4].try_into().unwrap();
                        (if little { f32::from_le_bytes(v) } else { f32::from_be_bytes(v) }) as f64
                    }
                    _ => 0.0,
                }
            };
            for r in 0..th {
                let mut acc: i64 = 0;
                for c in 0..tw {
                    let i = r * tw + c;
                    let v = rdv(&buf, i);
                    if p == 2 {
                        // Horizontal differencing on integer samples, wrapping at the sample width.
                        acc = (acc + v as i64) & ((1i64 << (bps * 8)) - 1);
                        let s = if ifd.sample_format == 2 && bps == 2 { acc as u16 as i16 as f64 } else { acc as f64 };
                        out[i] = s as f32;
                    } else {
                        out[i] = v as f32;
                    }
                }
            }
        }
    }
    if let Some(nd) = nodata {
        for v in out.iter_mut() {
            if (*v as f64 - nd).abs() < 1e-6 {
                *v = f32::NAN;
            }
        }
    }
    Ok(out)
}
