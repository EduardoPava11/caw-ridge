//! Drawing: raster bases, an SVG builder for the line work and lettering, and the sheet
//! that carries both. The SVG is rasterised by resvg, so the PNGs need no browser.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use rayon::prelude::*;
use resvg::{tiny_skia, usvg};

use crate::cog::Res;

pub type Rgb = [f32; 3];

pub fn hex(s: &str) -> Rgb {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s, 16).unwrap_or(0);
    [((v >> 16) & 255) as f32, ((v >> 8) & 255) as f32, (v & 255) as f32]
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// A colour ramp through (value, colour) stops.
pub struct Ramp(pub Vec<(f32, Rgb)>);

impl Ramp {
    pub fn new(stops: &[(f32, &str)]) -> Ramp {
        Ramp(stops.iter().map(|(v, c)| (*v, hex(c))).collect())
    }
    #[allow(dead_code)]
    pub fn at(&self, v: f32) -> Rgb {
        let s = &self.0;
        if v <= s[0].0 {
            return s[0].1;
        }
        for w in s.windows(2) {
            if v <= w[1].0 {
                return mix(w[0].1, w[1].1, (v - w[0].0) / (w[1].0 - w[0].0));
            }
        }
        s[s.len() - 1].1
    }
    /// The colour of the step that holds `v`, for classed maps.
    pub fn step(&self, v: f32) -> Rgb {
        let s = &self.0;
        let mut c = s[0].1;
        for st in s {
            if v >= st.0 {
                c = st.1;
            }
        }
        c
    }
}

/// An RGB raster in 0..255 floats.
#[derive(Clone)]
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Rgb>,
}

impl Raster {
    pub fn new(w: usize, h: usize, c: Rgb) -> Raster {
        Raster { w, h, px: vec![c; w * h] }
    }

    pub fn from_fn<F: Fn(usize, usize) -> Rgb + Sync>(w: usize, h: usize, f: F) -> Raster {
        let mut px = vec![[0f32; 3]; w * h];
        px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, p) in row.iter_mut().enumerate() {
                *p = f(x, y);
            }
        });
        Raster { w, h, px }
    }

    /// Light the raster with a relief shade. `flat` is the shade value of level ground.
    pub fn shaded(mut self, shade: &[f32], flat: f32, depth: f32) -> Raster {
        self.px.par_iter_mut().zip(shade.par_iter()).for_each(|(p, s)| {
            let f = s / flat;
            if f < 1.0 {
                let k = 1.0 - depth * (1.0 - f);
                *p = [p[0] * k, p[1] * k, p[2] * k];
            } else {
                let k = ((f - 1.0) * depth * 0.9).min(0.6);
                *p = mix(*p, [255.0, 255.0, 250.0], k);
            }
        });
        self
    }

    pub fn resized(&self, dw: usize, dh: usize) -> Raster {
        let (sw, sh) = (self.w, self.h);
        let mut px = vec![[0f32; 3]; dw * dh];
        px.par_chunks_mut(dw).enumerate().for_each(|(y, row)| {
            let fy = ((y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5).clamp(0.0, sh as f32 - 1.0);
            let y0 = fy.floor() as usize;
            let y1 = (y0 + 1).min(sh - 1);
            let ty = fy - y0 as f32;
            for (x, v) in row.iter_mut().enumerate() {
                let fx = ((x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5).clamp(0.0, sw as f32 - 1.0);
                let x0 = fx.floor() as usize;
                let x1 = (x0 + 1).min(sw - 1);
                let tx = fx - x0 as f32;
                let a = mix(self.px[y0 * sw + x0], self.px[y0 * sw + x1], tx);
                let b = mix(self.px[y1 * sw + x0], self.px[y1 * sw + x1], tx);
                *v = mix(a, b, ty);
            }
        });
        Raster { w: dw, h: dh, px }
    }

    /// Box blur, used to soften 30 m class boundaries before they are enlarged.
    pub fn blurred(&self, r: usize) -> Raster {
        if r == 0 {
            return self.clone();
        }
        let (w, h) = (self.w, self.h);
        let mut tmp = vec![[0f32; 3]; w * h];
        tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
                let mut s = [0f32; 3];
                for i in a..=b {
                    let p = self.px[y * w + i];
                    s = [s[0] + p[0], s[1] + p[1], s[2] + p[2]];
                }
                let n = (b - a + 1) as f32;
                row[x] = [s[0] / n, s[1] / n, s[2] / n];
            }
        });
        let mut out = vec![[0f32; 3]; w * h];
        out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
            let n = (b - a + 1) as f32;
            for x in 0..w {
                let mut s = [0f32; 3];
                for j in a..=b {
                    let p = tmp[j * w + x];
                    s = [s[0] + p[0], s[1] + p[1], s[2] + p[2]];
                }
                row[x] = [s[0] / n, s[1] / n, s[2] / n];
            }
        });
        Raster { w, h, px: out }
    }

    pub fn blit(&mut self, src: &Raster, ox: usize, oy: usize) {
        for y in 0..src.h.min(self.h.saturating_sub(oy)) {
            let n = src.w.min(self.w.saturating_sub(ox));
            self.px[(oy + y) * self.w + ox..(oy + y) * self.w + ox + n].copy_from_slice(&src.px[y * src.w..y * src.w + n]);
        }
    }

    /// Composite a premultiplied RGBA pixmap over the raster.
    pub fn over(&mut self, pm: &tiny_skia::Pixmap) {
        let d = pm.data();
        self.px.par_iter_mut().enumerate().for_each(|(i, p)| {
            let a = d[i * 4 + 3] as f32 / 255.0;
            if a > 0.0 {
                *p = [d[i * 4] as f32 + p[0] * (1.0 - a), d[i * 4 + 1] as f32 + p[1] * (1.0 - a), d[i * 4 + 2] as f32 + p[2] * (1.0 - a)];
            }
        });
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.px.par_iter().flat_map_iter(|p| [p[0].clamp(0.0, 255.0).round() as u8, p[1].clamp(0.0, 255.0).round() as u8, p[2].clamp(0.0, 255.0).round() as u8]).collect()
    }

    pub fn save_png(&self, path: &Path) -> Res<()> {
        let f = std::fs::File::create(path)?;
        let enc = image::codecs::png::PngEncoder::new_with_quality(
            std::io::BufWriter::new(f),
            image::codecs::png::CompressionType::Best,
            image::codecs::png::FilterType::Adaptive,
        );
        image::ImageEncoder::write_image(enc, &self.bytes(), self.w as u32, self.h as u32, image::ExtendedColorType::Rgb8)?;
        Ok(())
    }

    pub fn save_jpg(&self, path: &Path, quality: u8) -> Res<()> {
        let f = std::fs::File::create(path)?;
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(f), quality);
        image::ImageEncoder::write_image(enc, &self.bytes(), self.w as u32, self.h as u32, image::ExtendedColorType::Rgb8)?;
        Ok(())
    }
}

pub fn fontdb() -> Arc<usvg::fontdb::Database> {
    let mut db = usvg::fontdb::Database::new();
    db.load_fonts_dir("assets/fonts");
    Arc::new(db)
}

/// Rasterise an SVG document to a pixmap of its own size.
pub fn rasterise(svg: &str, w: usize, h: usize, fonts: &Arc<usvg::fontdb::Database>) -> Res<tiny_skia::Pixmap> {
    let mut opt = usvg::Options::default();
    opt.fontdb = fonts.clone();
    opt.font_family = "Barlow Semi Condensed".into();
    let tree = usvg::Tree::from_str(svg, &opt)?;
    let mut pm = tiny_skia::Pixmap::new(w as u32, h as u32).ok_or("pixmap too large")?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pm.as_mut());
    Ok(pm)
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Rough advance width of a string, in units of the font size. Good enough to keep labels
/// from colliding; resvg does the real shaping.
pub fn text_width(s: &str, family: Family) -> f32 {
    let per = match family {
        Family::Condensed => 0.40,
        Family::Semi => 0.46,
        Family::Sans => 0.54,
        Family::Serif => 0.50,
        Family::Mono => 0.60,
    };
    s.chars()
        .map(|c| match c {
            'i' | 'l' | 'I' | '.' | ',' | '\'' | ':' | '1' | ' ' => per * 0.55,
            'm' | 'w' | 'M' | 'W' => per * 1.45,
            c if c.is_uppercase() => per * 1.15,
            _ => per,
        })
        .sum()
}

#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum Family {
    Condensed,
    Semi,
    Sans,
    Serif,
    Mono,
}

impl Family {
    pub fn css(&self) -> &'static str {
        match self {
            Family::Condensed => "Barlow Condensed",
            Family::Semi => "Barlow Semi Condensed",
            Family::Sans => "Barlow",
            Family::Serif => "PT Serif",
            Family::Mono => "IBM Plex Mono",
        }
    }
}

#[derive(Clone)]
pub struct TextStyle {
    pub family: Family,
    pub size: f32,
    pub weight: u32,
    pub italic: bool,
    pub fill: &'static str,
    pub halo: &'static str,
    pub halo_w: f32,
    pub spacing: f32,
    pub upper: bool,
}

impl TextStyle {
    pub fn new(family: Family, size: f32, weight: u32, fill: &'static str) -> TextStyle {
        TextStyle { family, size, weight, italic: false, fill, halo: "#ffffff", halo_w: 0.0, spacing: 0.0, upper: false }
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub fn halo(mut self, c: &'static str, w: f32) -> Self {
        self.halo = c;
        self.halo_w = w;
        self
    }
    pub fn spaced(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }
    pub fn upper(mut self) -> Self {
        self.upper = true;
        self
    }
    #[allow(dead_code)]
    pub fn scaled(mut self, k: f32) -> Self {
        self.size *= k;
        self.halo_w *= k;
        self.spacing *= k;
        self
    }
    pub fn width(&self, s: &str) -> f32 {
        let n = s.chars().count() as f32;
        text_width(&if self.upper { s.to_uppercase() } else { s.to_string() }, self.family) * self.size + self.spacing * n
    }
}

/// Keeps labels from landing on each other.
#[derive(Default)]
pub struct Placer {
    boxes: Vec<[f32; 4]>,
}

impl Placer {
    pub fn free(&self, b: [f32; 4]) -> bool {
        !self.boxes.iter().any(|o| b[0] < o[2] && b[2] > o[0] && b[1] < o[3] && b[3] > o[1])
    }
    pub fn claim(&mut self, b: [f32; 4]) {
        self.boxes.push(b);
    }
    pub fn try_claim(&mut self, b: [f32; 4]) -> bool {
        if self.free(b) {
            self.boxes.push(b);
            true
        } else {
            false
        }
    }
}

/// An SVG document under construction.
pub struct Svg {
    pub w: f32,
    pub h: f32,
    pub body: String,
    pub defs: String,
}

impl Svg {
    pub fn new(w: f32, h: f32) -> Svg {
        Svg { w, h, body: String::new(), defs: String::new() }
    }

    pub fn finish(&self) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><defs>{d}</defs>{b}</svg>",
            w = self.w,
            h = self.h,
            d = self.defs,
            b = self.body
        )
    }

    pub fn raw(&mut self, s: &str) {
        self.body.push_str(s);
    }

    pub fn open(&mut self, attrs: &str) {
        let _ = write!(self.body, "<g {attrs}>");
    }

    pub fn close(&mut self) {
        self.body.push_str("</g>");
    }

    pub fn path_d(pts: &[(f32, f32)], closed: bool) -> String {
        let mut d = String::with_capacity(pts.len() * 12);
        for (i, p) in pts.iter().enumerate() {
            let _ = write!(d, "{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, p.0, p.1);
        }
        if closed {
            d.push('Z');
        }
        d
    }

    pub fn path(&mut self, d: &str, attrs: &str) {
        let _ = write!(self.body, "<path d=\"{d}\" {attrs}/>");
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), attrs: &str) {
        let _ = write!(self.body, "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" {attrs}/>", a.0, a.1, b.0, b.1);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, attrs: &str) {
        let _ = write!(self.body, "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" {attrs}/>");
    }

    pub fn circle(&mut self, x: f32, y: f32, r: f32, attrs: &str) {
        let _ = write!(self.body, "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r:.1}\" {attrs}/>");
    }

    /// Text anchored at (x, y) on its baseline. `anchor` is start, middle or end.
    pub fn text(&mut self, x: f32, y: f32, s: &str, st: &TextStyle, anchor: &str, rotate: f32) {
        let s = if st.upper { s.to_uppercase() } else { s.to_string() };
        let mut a = format!(
            "font-family=\"{}\" font-size=\"{:.1}\" font-weight=\"{}\" text-anchor=\"{}\"",
            st.family.css(),
            st.size,
            st.weight,
            anchor
        );
        if st.italic {
            a.push_str(" font-style=\"italic\"");
        }
        if st.spacing != 0.0 {
            let _ = write!(a, " letter-spacing=\"{:.2}\"", st.spacing);
        }
        let tr = if rotate != 0.0 { format!(" transform=\"rotate({rotate:.1} {x:.1} {y:.1})\"") } else { String::new() };
        if st.halo_w > 0.0 {
            let _ = write!(
                self.body,
                "<text x=\"{x:.1}\" y=\"{y:.1}\" {a}{tr} fill=\"none\" stroke=\"{}\" stroke-width=\"{:.1}\" stroke-linejoin=\"round\" stroke-opacity=\"0.85\">{}</text>",
                st.halo,
                st.halo_w,
                esc(&s)
            );
        }
        let _ = write!(self.body, "<text x=\"{x:.1}\" y=\"{y:.1}\" {a}{tr} fill=\"{}\">{}</text>", st.fill, esc(&s));
    }
}
