//! Magnetic declination from the World Magnetic Model 2025 (NOAA and the British Geological
//! Survey, public domain). Spherical harmonic synthesis to degree and order 12.

const COF: &str = include_str!("../assets/wmm/WMM2025.COF");

#[allow(dead_code)]
pub struct Field {
    /// Degrees east of true north.
    pub declination: f64,
    /// Degrees below horizontal.
    pub inclination: f64,
    /// Total intensity, nanotesla.
    pub total: f64,
}

pub fn field(lat: f64, lon: f64, alt_km: f64, year: f64) -> Field {
    const N: usize = 12;
    let mut g = [[0f64; N + 1]; N + 1];
    let mut h = [[0f64; N + 1]; N + 1];
    let mut epoch = 2025.0;
    for (i, line) in COF.lines().enumerate() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if i == 0 {
            epoch = p[0].parse().unwrap_or(2025.0);
            continue;
        }
        if p.len() < 6 {
            continue;
        }
        let (Ok(n), Ok(m)) = (p[0].parse::<usize>(), p[1].parse::<usize>()) else { continue };
        if n > N {
            continue;
        }
        let v: Vec<f64> = p[2..6].iter().map(|s| s.parse().unwrap_or(0.0)).collect();
        let dt = year - epoch;
        g[n][m] = v[0] + dt * v[2];
        h[n][m] = v[1] + dt * v[3];
    }
    // Geodetic to geocentric on WGS84.
    let (a, f) = (6378.137f64, 1.0 / 298.257223563);
    let e2 = f * (2.0 - f);
    let (phi, lam) = (lat.to_radians(), lon.to_radians());
    let rc = a / (1.0 - e2 * phi.sin().powi(2)).sqrt();
    let p = (rc + alt_km) * phi.cos();
    let z = (rc * (1.0 - e2) + alt_km) * phi.sin();
    let r = (p * p + z * z).sqrt();
    let phi_c = (z / r).asin();
    let re = 6371.2f64;
    // Schmidt semi-normalised associated Legendre functions of sin(latitude).
    let (s, c) = (phi_c.sin(), phi_c.cos());
    let mut pn = [[0f64; N + 2]; N + 2];
    let mut dp = [[0f64; N + 2]; N + 2];
    pn[0][0] = 1.0;
    for n in 1..=N {
        for m in 0..=n {
            if n == m {
                let k = if n == 1 { 1.0 } else { ((2 * n - 1) as f64 / (2 * n) as f64).sqrt() };
                pn[n][n] = k * c * pn[n - 1][n - 1];
                dp[n][n] = k * (c * dp[n - 1][n - 1] - s * pn[n - 1][n - 1]);
            } else {
                let den = ((n * n - m * m) as f64).sqrt();
                let k1 = (2 * n - 1) as f64 / den;
                let k2 = if n >= 2 { ((((n - 1) * (n - 1)) as f64 - (m * m) as f64).max(0.0)).sqrt() / den } else { 0.0 };
                let (p2, d2) = if n >= 2 { (pn[n - 2][m], dp[n - 2][m]) } else { (0.0, 0.0) };
                pn[n][m] = k1 * s * pn[n - 1][m] - k2 * p2;
                // dp here is the derivative with respect to latitude.
                dp[n][m] = k1 * (s * dp[n - 1][m] + c * pn[n - 1][m]) - k2 * d2;
            }
        }
    }
    let (mut bx, mut by, mut bz) = (0f64, 0f64, 0f64);
    for n in 1..=N {
        let rr = (re / r).powi(n as i32 + 2);
        for m in 0..=n {
            let (cm, sm) = ((m as f64 * lam).cos(), (m as f64 * lam).sin());
            let gh = g[n][m] * cm + h[n][m] * sm;
            // North component is minus the latitude derivative of the potential over r.
            bx -= rr * gh * dp[n][m];
            by += rr * m as f64 * (g[n][m] * sm - h[n][m] * cm) * pn[n][m] / c;
            bz -= (n as f64 + 1.0) * rr * gh * pn[n][m];
        }
    }
    // Rotate from geocentric to geodetic axes.
    let d = phi_c - phi;
    let x = bx * d.cos() - bz * d.sin();
    let zz = bx * d.sin() + bz * d.cos();
    let hh = (x * x + by * by).sqrt();
    Field { declination: by.atan2(x).to_degrees(), inclination: zz.atan2(hh).to_degrees(), total: (hh * hh + zz * zz).sqrt() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_noaa_calculator() {
        // NOAA declination calculator, WMM-2025, 2026.7479, sea level.
        let f = field(54.062707, -119.390728, 0.0, 2026.7479);
        assert!((f.declination - 15.66478).abs() < 0.02, "{}", f.declination);
    }

    #[test]
    fn matches_published_test_values() {
        // Rows of WMM2025_TestValues.txt: year, altitude km, lat, lon -> D, I, F.
        for (yr, alt, lat, lon, d, i, f) in [
            (2025.0, 18.0, 0.0, 21.0, 1.29, -26.06, 32594.761714),
            (2025.0, 65.0, 43.0, 93.0, 0.50, 64.10, 55626.621348),
            (2025.0, 94.0, -29.0, -110.0, 15.74, -38.25, 30792.688931),
            (2025.5, 6.0, -36.0, -137.0, 20.28, -52.11, 41280.516301),
        ] {
            let v = field(lat, lon, alt, yr);
            assert!((v.declination - d).abs() < 0.006, "D {} vs {}", v.declination, d);
            assert!((v.inclination - i).abs() < 0.006, "I {} vs {}", v.inclination, i);
            assert!((v.total - f).abs() < 0.5, "F {} vs {}", v.total, f);
        }
    }
}
