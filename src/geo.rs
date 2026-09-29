//! Map projections, written out by hand so the generator has no native dependencies.
//! Everything is on the GRS80 ellipsoid (NAD83 and WGS84 agree to within about 1.5 m here,
//! far below the 30 m cell of the elevation model).

use std::f64::consts::PI;

pub const A: f64 = 6378137.0;
pub const F: f64 = 1.0 / 298.257222101;

pub fn e2() -> f64 {
    2.0 * F - F * F
}

/// Lambert conformal conic with two standard parallels (the elevation model's native grid).
#[derive(Clone, Copy, Debug)]
pub struct Lcc {
    n: f64,
    af: f64,
    rho0: f64,
    lon0: f64,
    e: f64,
}

impl Lcc {
    pub fn new(lat0: f64, lon0: f64, sp1: f64, sp2: f64) -> Lcc {
        let e = e2().sqrt();
        let m = |p: f64| p.cos() / (1.0 - e * e * p.sin().powi(2)).sqrt();
        let t = |p: f64| {
            (PI / 4.0 - p / 2.0).tan() / ((1.0 - e * p.sin()) / (1.0 + e * p.sin())).powf(e / 2.0)
        };
        let (p0, p1, p2) = (lat0.to_radians(), sp1.to_radians(), sp2.to_radians());
        let n = (m(p1).ln() - m(p2).ln()) / (t(p1).ln() - t(p2).ln());
        let f = m(p1) / (n * t(p1).powf(n));
        Lcc { n, af: A * f, rho0: A * f * t(p0).powf(n), lon0: lon0.to_radians(), e }
    }

    /// EPSG:3979, NAD83(CSRS) / Canada Atlas Lambert.
    pub fn canada_atlas() -> Lcc {
        Lcc::new(49.0, -95.0, 49.0, 77.0)
    }

    pub fn forward(&self, lon: f64, lat: f64) -> (f64, f64) {
        let p = lat.to_radians();
        let e = self.e;
        let t = (PI / 4.0 - p / 2.0).tan() / ((1.0 - e * p.sin()) / (1.0 + e * p.sin())).powf(e / 2.0);
        let rho = self.af * t.powf(self.n);
        let th = self.n * (lon.to_radians() - self.lon0);
        (rho * th.sin(), self.rho0 - rho * th.cos())
    }

    #[allow(dead_code)]
    pub fn inverse(&self, x: f64, y: f64) -> (f64, f64) {
        let e = self.e;
        let dy = self.rho0 - y;
        let rho = self.n.signum() * (x * x + dy * dy).sqrt();
        let t = (rho / self.af).powf(1.0 / self.n);
        let th = x.atan2(dy);
        let lon = th / self.n + self.lon0;
        let mut p = PI / 2.0 - 2.0 * t.atan();
        for _ in 0..8 {
            p = PI / 2.0 - 2.0 * (t * ((1.0 - e * p.sin()) / (1.0 + e * p.sin())).powf(e / 2.0)).atan();
        }
        (lon.to_degrees(), p.to_degrees())
    }
}

/// Universal Transverse Mercator by the Krueger series (Karney 2011, sixth order in n).
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct Utm {
    pub zone: u32,
    lon0: f64,
    aa: f64,
    alpha: [f64; 6],
    beta: [f64; 6],
    e: f64,
}

pub const K0: f64 = 0.9996;

impl Utm {
    pub fn new(zone: u32) -> Utm {
        let n = F / (2.0 - F);
        let n2 = n * n;
        let n3 = n2 * n;
        let n4 = n3 * n;
        let n5 = n4 * n;
        let n6 = n5 * n;
        let aa = A / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0 + n6 / 256.0);
        let alpha = [
            n / 2.0 - 2.0 / 3.0 * n2 + 5.0 / 16.0 * n3 + 41.0 / 180.0 * n4 - 127.0 / 288.0 * n5
                + 7891.0 / 37800.0 * n6,
            13.0 / 48.0 * n2 - 3.0 / 5.0 * n3 + 557.0 / 1440.0 * n4 + 281.0 / 630.0 * n5
                - 1983433.0 / 1935360.0 * n6,
            61.0 / 240.0 * n3 - 103.0 / 140.0 * n4 + 15061.0 / 26880.0 * n5
                + 167603.0 / 181440.0 * n6,
            49561.0 / 161280.0 * n4 - 179.0 / 168.0 * n5 + 6601661.0 / 7257600.0 * n6,
            34729.0 / 80640.0 * n5 - 3418889.0 / 1995840.0 * n6,
            212378941.0 / 319334400.0 * n6,
        ];
        let beta = [
            n / 2.0 - 2.0 / 3.0 * n2 + 37.0 / 96.0 * n3 - 1.0 / 360.0 * n4 - 81.0 / 512.0 * n5
                + 96199.0 / 604800.0 * n6,
            1.0 / 48.0 * n2 + 1.0 / 15.0 * n3 - 437.0 / 1440.0 * n4 + 46.0 / 105.0 * n5
                - 1118711.0 / 3870720.0 * n6,
            17.0 / 480.0 * n3 - 37.0 / 840.0 * n4 - 209.0 / 4480.0 * n5 + 5569.0 / 90720.0 * n6,
            4397.0 / 161280.0 * n4 - 11.0 / 504.0 * n5 - 830251.0 / 7257600.0 * n6,
            4583.0 / 161280.0 * n5 - 108847.0 / 3991680.0 * n6,
            20648693.0 / 638668800.0 * n6,
        ];
        let lon0 = (zone as f64 * 6.0 - 183.0).to_radians();
        Utm { zone, lon0, aa, alpha, beta, e: e2().sqrt() }
    }

    pub fn zone_for(lon: f64) -> u32 {
        (((lon + 180.0) / 6.0).floor() as i64).rem_euclid(60) as u32 + 1
    }

    /// Returns (easting, northing) in metres, northern hemisphere.
    pub fn forward(&self, lon: f64, lat: f64) -> (f64, f64) {
        let p = lat.to_radians();
        let l = lon.to_radians() - self.lon0;
        let e = self.e;
        let tau = p.tan();
        let sigma = (e * (e * tau / (1.0 + tau * tau).sqrt()).atanh()).sinh();
        let taup = tau * (1.0 + sigma * sigma).sqrt() - sigma * (1.0 + tau * tau).sqrt();
        let xi = taup.atan2(l.cos());
        let eta = (l.sin() / (taup * taup + l.cos().powi(2)).sqrt()).asinh();
        let mut x = eta;
        let mut y = xi;
        for (j, a) in self.alpha.iter().enumerate() {
            let k = 2.0 * (j as f64 + 1.0);
            y += a * (k * xi).sin() * (k * eta).cosh();
            x += a * (k * xi).cos() * (k * eta).sinh();
        }
        (500000.0 + K0 * self.aa * x, K0 * self.aa * y)
    }

    pub fn inverse(&self, easting: f64, northing: f64) -> (f64, f64) {
        let e = self.e;
        let xi = northing / (K0 * self.aa);
        let eta = (easting - 500000.0) / (K0 * self.aa);
        let mut xip = xi;
        let mut etap = eta;
        for (j, b) in self.beta.iter().enumerate() {
            let k = 2.0 * (j as f64 + 1.0);
            xip -= b * (k * xi).sin() * (k * eta).cosh();
            etap -= b * (k * xi).cos() * (k * eta).sinh();
        }
        let taup = xip.sin() / (etap.sinh().powi(2) + xip.cos().powi(2)).sqrt();
        let l = etap.sinh().atan2(xip.cos());
        // Newton iteration from the conformal latitude back to the geodetic one.
        let mut tau = taup;
        for _ in 0..8 {
            let sigma = (e * (e * tau / (1.0 + tau * tau).sqrt()).atanh()).sinh();
            let f = tau * (1.0 + sigma * sigma).sqrt() - sigma * (1.0 + tau * tau).sqrt() - taup;
            let df = ((1.0 + sigma * sigma) * (1.0 + tau * tau)).sqrt() - sigma * tau;
            let df = df * (1.0 - e * e) * (1.0 + tau * tau).sqrt() / (1.0 + (1.0 - e * e) * tau * tau);
            tau -= f / df;
        }
        ((l + self.lon0).to_degrees(), tau.atan().to_degrees())
    }

    /// Grid convergence in degrees: the angle from true north to grid north, positive east.
    pub fn convergence(&self, lon: f64, lat: f64) -> f64 {
        let l = lon.to_radians() - self.lon0;
        (l.tan() * lat.to_radians().sin()).atan().to_degrees()
    }
}

/// Spherical Web Mercator (EPSG:3857), the projection of every web map tile.
pub fn merc_forward(lon: f64, lat: f64) -> (f64, f64) {
    let x = A * lon.to_radians();
    let y = A * (PI / 4.0 + lat.to_radians() / 2.0).tan().ln();
    (x, y)
}

pub fn merc_inverse(x: f64, y: f64) -> (f64, f64) {
    let lon = (x / A).to_degrees();
    let lat = (2.0 * (y / A).exp().atan() - PI / 2.0).to_degrees();
    (lon, lat)
}

/// Great circle distance in metres on the mean sphere.
pub fn haversine(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> f64 {
    let r = 6371008.8;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

/// Initial bearing in degrees clockwise from true north.
pub fn bearing(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dl = (lon2 - lon1).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

pub fn compass(b: f64) -> &'static str {
    const N: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    N[(((b % 360.0) + 11.25) / 22.5) as usize % 16]
}

/// A bearing in plain words, to eight points.
pub fn compass_words(b: f64) -> &'static str {
    const N: [&str; 8] = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
    N[(((b % 360.0) + 22.5) / 45.0) as usize % 8]
}

/// Degrees to degrees, minutes, decimal seconds.
pub fn dms(v: f64, pos: char, neg: char) -> String {
    let h = if v < 0.0 { neg } else { pos };
    let a = v.abs();
    let d = a.floor();
    let m = ((a - d) * 60.0).floor();
    let s = (a - d - m / 60.0) * 3600.0;
    format!("{}\u{b0} {:02}' {:04.1}\" {}", d as i32, m as i32, s, h)
}

/// Military grid style 100 km square letters are skipped on purpose: the site prints
/// full UTM coordinates, which is what a handheld GPS shows in Canada.
pub fn utm_string(lon: f64, lat: f64) -> String {
    let z = Utm::zone_for(lon);
    let (e, n) = Utm::new(z).forward(lon, lat);
    format!("{}U {:06.0} E {:07.0} N", z, e, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lcc_matches_proj() {
        // Reference values from PROJ for EPSG:4617 -> EPSG:3979 (same datum, so pure projection).
        let (x, y) = Lcc::canada_atlas().forward(-119.390728, 54.062707);
        assert!((x - -1532407.2453185585).abs() < 0.001, "{x}");
        assert!((y - 855786.0437462413).abs() < 0.001, "{y}");
        let (lon, lat) = Lcc::canada_atlas().inverse(x, y);
        assert!((lon - -119.390728).abs() < 1e-9);
        assert!((lat - 54.062707).abs() < 1e-9);
    }

    #[test]
    fn utm_round_trip() {
        let u = Utm::new(11);
        let (e, n) = u.forward(-119.390728, 54.062707);
        // PROJ, EPSG:4269 -> EPSG:26911.
        assert!((e - 343538.58429679496).abs() < 0.001, "{e}");
        assert!((n - 5993142.099820714).abs() < 0.001, "{n}");
        let (lon, lat) = u.inverse(e, n);
        assert!((lon - -119.390728).abs() < 1e-9, "{lon}");
        assert!((lat - 54.062707).abs() < 1e-9, "{lat}");
    }
}
