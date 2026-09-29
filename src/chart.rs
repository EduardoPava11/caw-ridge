//! Charts as inline SVG. Colour comes from CSS custom properties so that the light and
//! dark themes each get their own validated steps; the hover layer lives in app.js and
//! reads the `data-tip` attributes written here. Every chart is followed by its table.

use std::fmt::Write as _;

use crate::draw::esc;

const W: f32 = 760.0;
const H: f32 = 300.0;
const ML: f32 = 56.0;
const MR: f32 = 18.0;
const MT: f32 = 26.0;
const MB: f32 = 40.0;

fn nice_step(span: f32, target: f32) -> f32 {
    let raw = span / target;
    let mag = 10f32.powf(raw.log10().floor());
    let n = raw / mag;
    mag * if n < 1.5 { 1.0 } else if n < 3.5 { 2.0 } else if n < 7.5 { 5.0 } else { 10.0 }
}

pub fn thousands(v: f32) -> String {
    let n = v.round() as i64;
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

fn frame(label: &str, body: &str, extra_class: &str) -> String {
    format!(
        "<svg class=\"chart {extra_class}\" viewBox=\"0 0 {W} {H}\" role=\"img\" aria-label=\"{}\" preserveAspectRatio=\"xMidYMid meet\">{body}</svg>",
        esc(label)
    )
}

pub struct Mark {
    /// Position along x in the data's own unit.
    pub x: f64,
    pub label: String,
}

/// A single series as a line with a light wash under it, a crosshair on hover.
pub fn area(label: &str, pts: &[(f64, f32)], x_unit: &str, y_unit: &str, marks: &[Mark]) -> String {
    let x1 = pts.last().map(|p| p.0).unwrap_or(1.0).max(0.001);
    let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    let step = nice_step(hi - lo, 4.0);
    let y0 = (lo / step).floor() * step;
    let y1 = (hi / step).ceil() * step;
    let px = |x: f64| ML + (x / x1) as f32 * (W - ML - MR);
    let py = |y: f32| H - MB - (y - y0) / (y1 - y0) * (H - MT - MB);
    let mut s = String::new();
    let mut y = y0;
    while y <= y1 + 0.01 {
        let _ = write!(s, "<line class=\"grid\" x1=\"{ML}\" x2=\"{}\" y1=\"{:.1}\" y2=\"{:.1}\"/>", W - MR, py(y), py(y));
        let _ = write!(s, "<text class=\"tick\" x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>", ML - 8.0, py(y) + 4.0, thousands(y));
        y += step;
    }
    let xs = nice_step(x1 as f32, 7.0) as f64;
    let mut x = 0.0;
    while x <= x1 + 1e-6 {
        let lab = if xs < 1.0 { format!("{x:.1}") } else { format!("{x:.0}") };
        let _ = write!(s, "<text class=\"tick\" x=\"{:.1}\" y=\"{}\" text-anchor=\"middle\">{lab}</text>", px(x), H - MB + 18.0);
        x += xs;
    }
    let _ = write!(s, "<text class=\"axis\" x=\"{}\" y=\"{}\" text-anchor=\"end\">{}</text>", W - MR, H - 6.0, esc(x_unit));
    let _ = write!(s, "<text class=\"axis\" x=\"{ML}\" y=\"14\" text-anchor=\"start\">{}</text>", esc(y_unit));
    for m in marks {
        let _ = write!(s, "<line class=\"rule\" x1=\"{0:.1}\" x2=\"{0:.1}\" y1=\"{1}\" y2=\"{2}\"/>", px(m.x), MT, H - MB);
        let _ = write!(s, "<text class=\"note\" x=\"{:.1}\" y=\"{}\" text-anchor=\"start\">{}</text>", px(m.x) + 5.0, MT + 11.0, esc(&m.label));
    }
    // Thin the line to about one vertex per pixel.
    let stride = (pts.len() / 700).max(1);
    let mut d = String::new();
    let mut tips = String::new();
    for (i, p) in pts.iter().enumerate().filter(|(i, _)| i % stride == 0 || *i == pts.len() - 1) {
        let _ = write!(d, "{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, px(p.0), py(p.1));
        let _ = write!(tips, "{:.1},{:.1},{:.2},{:.0};", px(p.0), py(p.1), p.0, p.1);
    }
    let base = H - MB;
    let _ = write!(s, "<path class=\"wash\" d=\"{d}L{:.1} {base}L{ML} {base}Z\"/>", px(x1));
    let _ = write!(s, "<path class=\"line\" d=\"{d}\"/>");
    let _ = write!(s, "<line class=\"base\" x1=\"{ML}\" x2=\"{}\" y1=\"{base}\" y2=\"{base}\"/>", W - MR);
    let _ = write!(
        s,
        "<g class=\"hover\" data-points=\"{tips}\" data-x-unit=\"{}\" data-y-unit=\"{}\"><line class=\"cross\" y1=\"{MT}\" y2=\"{base}\" x1=\"0\" x2=\"0\"/><circle class=\"dot\" r=\"5\" cx=\"0\" cy=\"0\"/><rect class=\"hit\" x=\"{ML}\" y=\"{MT}\" width=\"{}\" height=\"{}\" tabindex=\"0\"/></g>",
        esc(x_unit),
        esc(y_unit),
        W - ML - MR,
        H - MT - MB
    );
    frame(label, &s, "chart-area")
}

pub struct Span {
    pub label: String,
    pub lo: f32,
    pub hi: f32,
    pub tip: String,
}

/// Floating columns, one per period, from a low to a high value.
pub fn ranges(label: &str, spans: &[Span], y_unit: &str, zero_note: &str) -> String {
    let (lo, hi) = spans.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.lo), a.1.max(p.hi)));
    let step = nice_step(hi - lo, 5.0);
    let y0 = (lo / step).floor() * step;
    let y1 = (hi / step).ceil() * step;
    let py = |y: f32| H - MB - (y - y0) / (y1 - y0) * (H - MT - MB);
    let band = (W - ML - MR) / spans.len() as f32;
    let bw = (band * 0.62).min(24.0);
    let mut s = String::new();
    let mut y = y0;
    while y <= y1 + 0.01 {
        let class = if y.abs() < 0.01 { "zero" } else { "grid" };
        let _ = write!(s, "<line class=\"{class}\" x1=\"{ML}\" x2=\"{}\" y1=\"{:.1}\" y2=\"{:.1}\"/>", W - MR, py(y), py(y));
        let _ = write!(s, "<text class=\"tick\" x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>", ML - 8.0, py(y) + 4.0, thousands(y));
        y += step;
    }
    if y0 < 0.0 && y1 > 0.0 && !zero_note.is_empty() {
        let _ = write!(s, "<text class=\"note\" x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>", W - MR, py(0.0) - 5.0, esc(zero_note));
    }
    let _ = write!(s, "<text class=\"axis\" x=\"{ML}\" y=\"14\" text-anchor=\"start\">{}</text>", esc(y_unit));
    for (i, sp) in spans.iter().enumerate() {
        let cx = ML + band * (i as f32 + 0.5);
        let (top, bot) = (py(sp.hi), py(sp.lo));
        let _ = write!(
            s,
            "<g class=\"mark\" tabindex=\"0\" data-tip=\"{}\"><rect class=\"hit\" x=\"{:.1}\" y=\"{MT}\" width=\"{:.1}\" height=\"{}\"/><rect class=\"bar\" x=\"{:.1}\" y=\"{top:.1}\" width=\"{bw:.1}\" height=\"{:.1}\" rx=\"4\"/></g>",
            esc(&sp.tip),
            cx - band / 2.0,
            band,
            H - MT - MB,
            cx - bw / 2.0,
            (bot - top).max(2.0)
        );
        let _ = write!(s, "<text class=\"tick\" x=\"{cx:.1}\" y=\"{}\" text-anchor=\"middle\">{}</text>", H - MB + 18.0, esc(&sp.label));
    }
    // Direct labels on the first and last column only.
    for i in [0, spans.len() - 1] {
        let sp = &spans[i];
        let cx = ML + band * (i as f32 + 0.5);
        let _ = write!(s, "<text class=\"value\" x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{:.0}</text>", py(sp.hi) - 6.0, sp.hi);
        let _ = write!(s, "<text class=\"value\" x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{:.0}</text>", py(sp.lo) + 15.0, sp.lo);
    }
    frame(label, &s, "chart-ranges")
}

pub struct Bar {
    pub label: String,
    pub value: f32,
    pub text: String,
    pub tip: String,
}

/// Horizontal bars, one colour, value at the tip.
pub fn bars(label: &str, items: &[Bar]) -> String {
    let row = 30.0;
    let h = items.len() as f32 * row + 12.0;
    let left = 176.0;
    let right = 96.0;
    let max = items.iter().map(|b| b.value).fold(0.0, f32::max).max(1e-6);
    let mut s = String::new();
    let _ = write!(s, "<line class=\"base\" x1=\"{left}\" x2=\"{left}\" y1=\"4\" y2=\"{}\"/>", h - 4.0);
    for (i, b) in items.iter().enumerate() {
        let y = 6.0 + i as f32 * row;
        let w = (b.value / max * (W - left - right)).max(2.0);
        let _ = write!(
            s,
            "<g class=\"mark\" tabindex=\"0\" data-tip=\"{}\"><rect class=\"hit\" x=\"0\" y=\"{y:.1}\" width=\"{W}\" height=\"{row}\"/><text class=\"cat\" x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text><path class=\"bar\" d=\"M{left} {:.1}h{:.1}a4 4 0 0 1 4 4v10a4 4 0 0 1 -4 4h-{:.1}z\"/><text class=\"value\" x=\"{:.1}\" y=\"{:.1}\">{}</text></g>",
            esc(&b.tip),
            left - 10.0,
            y + 19.0,
            esc(&b.label),
            y + 6.0,
            (w - 4.0).max(0.0),
            (w - 4.0).max(0.0),
            left + w + 8.0,
            y + 19.0,
            esc(&b.text)
        );
    }
    format!("<svg class=\"chart chart-bars\" viewBox=\"0 0 {W} {h}\" role=\"img\" aria-label=\"{}\">{s}</svg>", esc(label))
}

pub struct LightDay {
    pub label: String,
    pub tick: bool,
    /// Clock hours: legal start, sunrise, sunset, legal end.
    pub t: [f32; 4],
    pub tip: String,
}

/// Daylight through the season: the sun-up band inside the legal shooting band.
pub fn daylight(label: &str, days: &[LightDay], note_at: Option<(usize, &str)>) -> String {
    let (y0, y1) = (5.0f32, 22.0f32);
    // Clock time runs down the chart, morning at the top.
    let py = |h: f32| MT + (h - y0) / (y1 - y0) * (H - MT - MB);
    let band = (W - ML - MR) / days.len() as f32;
    let px = |i: usize| ML + band * (i as f32 + 0.5);
    let mut s = String::new();
    let mut h = 6.0;
    while h <= 21.0 {
        let _ = write!(s, "<line class=\"grid\" x1=\"{ML}\" x2=\"{}\" y1=\"{:.1}\" y2=\"{:.1}\"/>", W - MR, py(h), py(h));
        let _ = write!(s, "<text class=\"tick\" x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{:.0}:00</text>", ML - 8.0, py(h) + 4.0, h);
        h += 3.0;
    }
    let poly = |a: usize, b: usize| -> String {
        let mut d = String::new();
        for (i, day) in days.iter().enumerate() {
            let _ = write!(d, "{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, px(i), py(day.t[a]));
        }
        for (i, day) in days.iter().enumerate().rev() {
            let _ = write!(d, "L{:.1} {:.1}", px(i), py(day.t[b]));
        }
        d.push('Z');
        d
    };
    let _ = write!(s, "<path class=\"wash\" d=\"{}\"/>", poly(0, 3));
    let _ = write!(s, "<path class=\"wash strong\" d=\"{}\"/>", poly(1, 2));
    for k in [1usize, 2] {
        let mut d = String::new();
        for (i, day) in days.iter().enumerate() {
            let _ = write!(d, "{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, px(i), py(day.t[k]));
        }
        let _ = write!(s, "<path class=\"line\" d=\"{d}\"/>");
    }
    let last = days.len() - 1;
    let _ = write!(s, "<text class=\"value\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">Sunrise</text>", px(last), py(days[last].t[1]) + 16.0);
    let _ = write!(s, "<text class=\"value\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">Sunset</text>", px(last), py(days[last].t[2]) - 8.0);
    let _ = write!(s, "<text class=\"note\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\">Legal light starts</text>", px(0) + 4.0, py(days[0].t[0]) - 7.0);
    let _ = write!(s, "<text class=\"note\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\">Legal light ends</text>", px(0) + 4.0, py(days[0].t[3]) + 15.0);
    if let Some((i, text)) = note_at {
        let _ = write!(s, "<line class=\"rule\" x1=\"{0:.1}\" x2=\"{0:.1}\" y1=\"{MT}\" y2=\"{1}\"/>", px(i) - band / 2.0, H - MB);
        let _ = write!(s, "<text class=\"note\" x=\"{:.1}\" y=\"{}\" text-anchor=\"start\">{}</text>", px(i) - band / 2.0 + 5.0, MT + 11.0, esc(text));
    }
    for (i, day) in days.iter().enumerate() {
        if day.tick {
            let _ = write!(s, "<text class=\"tick\" x=\"{:.1}\" y=\"{}\" text-anchor=\"middle\">{}</text>", px(i), H - MB + 18.0, esc(&day.label));
        }
        let _ = write!(
            s,
            "<g class=\"mark quiet\" tabindex=\"-1\" data-tip=\"{}\"><rect class=\"hit\" x=\"{:.1}\" y=\"{MT}\" width=\"{:.2}\" height=\"{}\"/></g>",
            esc(&day.tip),
            px(i) - band / 2.0,
            band,
            H - MT - MB
        );
    }
    frame(label, &s, "chart-daylight")
}

/// The table that stands behind a chart.
pub fn table(caption: &str, head: &[&str], rows: &[Vec<String>]) -> String {
    let mut s = format!("<details class=\"data\"><summary>{}</summary><div class=\"scroll\"><table><thead><tr>", esc(caption));
    for h in head {
        let _ = write!(s, "<th>{}</th>", esc(h));
    }
    s.push_str("</tr></thead><tbody>");
    for r in rows {
        s.push_str("<tr>");
        for c in r {
            let _ = write!(s, "<td>{}</td>", esc(c));
        }
        s.push_str("</tr>");
    }
    s.push_str("</tbody></table></div></details>");
    s
}
