//! Sun and moon. The solar position follows the NOAA solar calculator (after Meeus),
//! good to about a minute for rise and set times.

use std::f64::consts::PI;

/// Days since the civil epoch 1970-01-01 for a calendar date.
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

pub fn weekday(days: i64) -> &'static str {
    ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize]
}

pub fn month_name(m: u32) -> &'static str {
    ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][m as usize]
}

/// Julian day for days since 1970 plus a UTC hour.
pub fn julian(days: i64, utc_hours: f64) -> f64 {
    days as f64 + 2440587.5 + utc_hours / 24.0
}

struct Solar {
    decl: f64,
    eqtime: f64,
}

fn solar(jd: f64) -> Solar {
    let t = (jd - 2451545.0) / 36525.0;
    let l0 = (280.46646 + t * (36000.76983 + t * 0.0003032)).rem_euclid(360.0);
    let m = 357.52911 + t * (35999.05029 - 0.0001537 * t);
    let e = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
    let mr = m.to_radians();
    let c = mr.sin() * (1.914602 - t * (0.004817 + 0.000014 * t)) + (2.0 * mr).sin() * (0.019993 - 0.000101 * t) + (3.0 * mr).sin() * 0.000289;
    let true_long = l0 + c;
    let omega = 125.04 - 1934.136 * t;
    let lambda = true_long - 0.00569 - 0.00478 * omega.to_radians().sin();
    let eps0 = 23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60.0) / 60.0;
    let eps = eps0 + 0.00256 * omega.to_radians().cos();
    let decl = (eps.to_radians().sin() * lambda.to_radians().sin()).asin();
    let y = (eps.to_radians() / 2.0).tan().powi(2);
    let l0r = l0.to_radians();
    let eqtime = 4.0
        * (y * (2.0 * l0r).sin() - 2.0 * e * mr.sin() + 4.0 * e * y * mr.sin() * (2.0 * l0r).cos()
            - 0.5 * y * y * (4.0 * l0r).sin()
            - 1.25 * e * e * (2.0 * mr).sin())
        .to_degrees();
    Solar { decl, eqtime }
}

/// Azimuth (degrees from north) and altitude (degrees, refraction corrected) of the sun.
pub fn position(days: i64, utc_hours: f64, lon: f64, lat: f64) -> (f64, f64) {
    let s = solar(julian(days, utc_hours));
    let tst = (utc_hours * 60.0 + s.eqtime + 4.0 * lon).rem_euclid(1440.0);
    let ha = (tst / 4.0 - 180.0).to_radians();
    let p = lat.to_radians();
    let cosz = p.sin() * s.decl.sin() + p.cos() * s.decl.cos() * ha.cos();
    let zen = cosz.clamp(-1.0, 1.0).acos();
    let mut alt = 90.0 - zen.to_degrees();
    let az = ((ha.sin()).atan2(ha.cos() * p.sin() - s.decl.tan() * p.cos()).to_degrees() + 180.0).rem_euclid(360.0);
    // Atmospheric refraction, NOAA's piecewise fit.
    let te = alt.to_radians().tan();
    let refr = if alt > 85.0 {
        0.0
    } else if alt > 5.0 {
        (58.1 / te - 0.07 / te.powi(3) + 0.000086 / te.powi(5)) / 3600.0
    } else if alt > -0.575 {
        (1735.0 + alt * (-518.2 + alt * (103.4 + alt * (-12.79 + alt * 0.711)))) / 3600.0
    } else {
        -20.774 / te / 3600.0
    };
    alt += refr;
    (az, alt)
}

/// UTC hours at which the sun's centre crosses `zenith` degrees, (morning, evening),
/// and solar noon. 90.833 is sunrise and sunset; 96 is civil twilight.
pub fn crossing(days: i64, lon: f64, lat: f64, zenith: f64) -> Option<(f64, f64, f64)> {
    let mut rise = 12.0 - lon / 15.0 - 6.0;
    let mut set = 12.0 - lon / 15.0 + 6.0;
    let mut noon = 12.0 - lon / 15.0;
    for _ in 0..4 {
        for (k, tv) in [(0usize, rise), (1, set), (2, noon)] {
            let s = solar(julian(days, tv));
            let p = lat.to_radians();
            let cosh = (zenith.to_radians().cos() - p.sin() * s.decl.sin()) / (p.cos() * s.decl.cos());
            if cosh.abs() > 1.0 {
                return None;
            }
            let h = cosh.acos().to_degrees();
            let n = (720.0 - 4.0 * lon - s.eqtime) / 60.0;
            match k {
                0 => rise = n - h * 4.0 / 60.0,
                1 => set = n + h * 4.0 / 60.0,
                _ => noon = n,
            }
        }
    }
    Some((rise, set, noon))
}

/// Offset of Alberta clock time from UTC on a date: daylight time from the second Sunday of
/// March to the first Sunday of November.
pub fn alberta_offset(days: i64) -> (f64, &'static str) {
    let (y, _, _) = civil_from_days(days);
    let first_sunday = |m: u32| -> i64 {
        let d1 = days_from_civil(y, m, 1);
        d1 + (3 - d1.rem_euclid(7)).rem_euclid(7)
    };
    let start = first_sunday(3) + 7;
    let end = first_sunday(11);
    if days >= start && days < end { (-6.0, "MDT") } else { (-7.0, "MST") }
}

pub fn hm(hours: f64) -> String {
    let m = (hours.rem_euclid(24.0) * 60.0).round() as i64;
    format!("{}:{:02}", (m / 60) % 24, m % 60)
}

/// Moon: illuminated fraction and age in days, from a short series (Meeus, low precision).
pub fn moon(days: i64, utc_hours: f64) -> (f64, f64, &'static str) {
    let jd = julian(days, utc_hours);
    let t = (jd - 2451545.0) / 36525.0;
    let d = (297.8501921 + 445267.1114034 * t).rem_euclid(360.0).to_radians();
    let m = (357.5291092 + 35999.0502909 * t).rem_euclid(360.0).to_radians();
    let mp = (134.9633964 + 477198.8675055 * t).rem_euclid(360.0).to_radians();
    // Phase angle.
    let i = 180.0 - d.to_degrees() - 6.289 * mp.sin() + 2.100 * m.sin() - 1.274 * (2.0 * d - mp).sin() - 0.658 * (2.0 * d).sin()
        - 0.214 * (2.0 * mp).sin()
        - 0.110 * d.sin();
    let frac = (1.0 + i.to_radians().cos()) / 2.0;
    // Elongation with the main corrections tells waxing from waning.
    let elong = (d.to_degrees() + 6.289 * mp.sin() - 2.100 * m.sin() + 1.274 * (2.0 * d - mp).sin() + 0.658 * (2.0 * d).sin()).rem_euclid(360.0);
    let age = elong / 360.0 * 29.530588;
    let name = match elong {
        e if !(11.25..348.75).contains(&e) => "new",
        e if e < 78.75 => "waxing crescent",
        e if e < 101.25 => "first quarter",
        e if e < 168.75 => "waxing gibbous",
        e if e < 191.25 => "full",
        e if e < 258.75 => "waning gibbous",
        e if e < 281.25 => "last quarter",
        _ => "waning crescent",
    };
    let _ = PI;
    (frac, age, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_round_trip() {
        let d = days_from_civil(2026, 9, 28);
        assert_eq!(civil_from_days(d), (2026, 9, 28));
        assert_eq!(weekday(d), "Mon");
    }

    #[test]
    fn dst_ends_first_sunday_of_november() {
        assert_eq!(alberta_offset(days_from_civil(2026, 10, 31)).1, "MDT");
        assert_eq!(alberta_offset(days_from_civil(2026, 11, 1)).1, "MST");
    }

    #[test]
    fn equinox_day_is_about_twelve_hours() {
        let d = days_from_civil(2026, 9, 25);
        let (r, s, _) = crossing(d, -119.39, 54.06, 90.833).unwrap();
        assert!(((s - r) - 12.0).abs() < 0.2, "{}", s - r);
    }
}
