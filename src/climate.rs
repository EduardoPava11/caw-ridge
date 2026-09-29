//! Weather history and forecast from Open-Meteo. The history is the ERA5 reanalysis scaled
//! to the height of the waypoint: a model of the past, not a weather station on the ridge.

use std::path::Path;

use serde_json::Value;

use crate::cog::Res;
use crate::config;
use crate::sun;

pub fn fetch_history() -> Res<String> {
    let url = format!(
        "https://archive-api.open-meteo.com/v1/archive?latitude={}&longitude={}&elevation={}&start_date=2016-08-15&end_date=2025-12-15&daily=temperature_2m_max,temperature_2m_min,precipitation_sum,snowfall_sum,wind_speed_10m_max,wind_gusts_10m_max,wind_direction_10m_dominant&timezone=America%2FEdmonton&wind_speed_unit=kmh",
        config::WPT_LAT, config::WPT_LON, 1991
    );
    Ok(String::from_utf8(crate::cog::get(&url)?)?)
}

pub fn forecast_url() -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&elevation={}&daily=temperature_2m_max,temperature_2m_min,precipitation_sum,snowfall_sum,wind_speed_10m_max,wind_gusts_10m_max,wind_direction_10m_dominant,weather_code,precipitation_probability_max&hourly=temperature_2m,wind_speed_10m,wind_gusts_10m,wind_direction_10m,freezing_level_height,precipitation,snowfall,weather_code&timezone=America%2FEdmonton&forecast_days=10&wind_speed_unit=kmh",
        config::WPT_LAT, config::WPT_LON, 1991
    )
}

pub fn fetch_forecast() -> Res<String> {
    Ok(String::from_utf8(crate::cog::get(&forecast_url())?)?)
}

#[allow(dead_code)]
pub struct Week {
    /// First day of the week as (month, day).
    pub start: (u32, u32),
    pub mean_max: f32,
    pub mean_min: f32,
    pub lowest: f32,
    pub highest: f32,
    /// Share of days with at least 1 cm of new snow.
    pub snow_days: f32,
    /// Share of days with at least 1 mm of precipitation of any kind.
    pub wet_days: f32,
    pub mean_wind: f32,
    pub mean_gust: f32,
    pub top_gust: f32,
    pub days: usize,
}

#[allow(dead_code)]
pub struct Rose {
    /// Eight sectors from north clockwise: share of days, mean of the daily top wind speed.
    pub sectors: [(f32, f32); 8],
    pub days: usize,
}

pub struct Climate {
    pub weeks: Vec<Week>,
    pub rose: Rose,
    pub years: (i32, i32),
}

#[allow(dead_code)]
pub struct Day {
    pub date: String,
    pub tmax: f32,
    pub tmin: f32,
    pub precip: f32,
    pub snow: f32,
    pub wind: f32,
    pub gust: f32,
    pub dir: f32,
    pub code: i64,
    pub pop: f32,
}

pub struct Forecast {
    pub days: Vec<Day>,
    pub fetched: String,
}

fn arr(v: &Value, k: &str) -> Vec<f32> {
    v["daily"][k].as_array().map(|a| a.iter().map(|x| x.as_f64().map(|f| f as f32).unwrap_or(f32::NAN)).collect()).unwrap_or_default()
}

pub fn load_history(dir: &Path) -> Res<Climate> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("climate.json"))?)?;
    let times: Vec<String> = v["daily"]["time"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let (tmax, tmin, pr, sn, wd, gu, di) = (
        arr(&v, "temperature_2m_max"),
        arr(&v, "temperature_2m_min"),
        arr(&v, "precipitation_sum"),
        arr(&v, "snowfall_sum"),
        arr(&v, "wind_speed_10m_max"),
        arr(&v, "wind_gusts_10m_max"),
        arr(&v, "wind_direction_10m_dominant"),
    );
    let mut weeks = Vec::new();
    // Thirteen weeks from 1 September.
    for k in 0..13 {
        let mut idx = Vec::new();
        for (i, t) in times.iter().enumerate() {
            let (y, m, d): (i32, u32, u32) = (t[..4].parse()?, t[5..7].parse()?, t[8..10].parse()?);
            let off = sun::days_from_civil(y, m, d) - sun::days_from_civil(y, 9, 1);
            if off >= k * 7 && off < (k + 1) * 7 && !tmax[i].is_nan() {
                idx.push(i);
            }
        }
        if idx.is_empty() {
            continue;
        }
        let n = idx.len() as f32;
        let mean = |a: &Vec<f32>| idx.iter().map(|&i| a[i]).sum::<f32>() / n;
        let (_, m, d) = sun::civil_from_days(sun::days_from_civil(2026, 9, 1) + k * 7);
        weeks.push(Week {
            start: (m, d),
            mean_max: mean(&tmax),
            mean_min: mean(&tmin),
            lowest: idx.iter().map(|&i| tmin[i]).fold(f32::MAX, f32::min),
            highest: idx.iter().map(|&i| tmax[i]).fold(f32::MIN, f32::max),
            snow_days: idx.iter().filter(|&&i| sn[i] >= 1.0).count() as f32 / n,
            wet_days: idx.iter().filter(|&&i| pr[i] >= 1.0).count() as f32 / n,
            mean_wind: mean(&wd),
            mean_gust: mean(&gu),
            top_gust: idx.iter().map(|&i| gu[i]).fold(f32::MIN, f32::max),
            days: idx.len(),
        });
    }
    let mut sectors = [(0f32, 0f32); 8];
    let mut days = 0usize;
    for (i, t) in times.iter().enumerate() {
        let (m, d): (u32, u32) = (t[5..7].parse()?, t[8..10].parse()?);
        let in_season = (m == 9 && d >= 15) || m == 10 || m == 11;
        if !in_season || di[i].is_nan() {
            continue;
        }
        let s = (((di[i] + 22.5) / 45.0) as usize) % 8;
        sectors[s].0 += 1.0;
        sectors[s].1 += wd[i];
        days += 1;
    }
    for s in sectors.iter_mut() {
        if s.0 > 0.0 {
            s.1 /= s.0;
        }
        s.0 /= days.max(1) as f32;
    }
    let y0: i32 = times.first().map(|t| t[..4].parse().unwrap_or(0)).unwrap_or(0);
    let y1: i32 = times.last().map(|t| t[..4].parse().unwrap_or(0)).unwrap_or(0);
    Ok(Climate { weeks, rose: Rose { sectors, days }, years: (y0, y1) })
}

pub fn load_forecast(dir: &Path) -> Res<Forecast> {
    let text = std::fs::read_to_string(dir.join("forecast.json"))?;
    let v: Value = serde_json::from_str(&text)?;
    let times: Vec<String> = v["daily"]["time"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let (tmax, tmin, pr, sn, wd, gu, di, pop) = (
        arr(&v, "temperature_2m_max"),
        arr(&v, "temperature_2m_min"),
        arr(&v, "precipitation_sum"),
        arr(&v, "snowfall_sum"),
        arr(&v, "wind_speed_10m_max"),
        arr(&v, "wind_gusts_10m_max"),
        arr(&v, "wind_direction_10m_dominant"),
        arr(&v, "precipitation_probability_max"),
    );
    let codes: Vec<i64> = v["daily"]["weather_code"].as_array().map(|a| a.iter().map(|x| x.as_i64().unwrap_or(0)).collect()).unwrap_or_default();
    let days = times
        .iter()
        .enumerate()
        .map(|(i, t)| Day { date: t.clone(), tmax: tmax[i], tmin: tmin[i], precip: pr[i], snow: sn[i], wind: wd[i], gust: gu[i], dir: di[i], code: codes.get(i).cloned().unwrap_or(0), pop: pop.get(i).cloned().unwrap_or(f32::NAN) })
        .collect();
    let fetched = v["fetched"].as_str().unwrap_or("").to_string();
    Ok(Forecast { days, fetched })
}

/// WMO weather interpretation codes, in plain words.
pub fn code_words(c: i64) -> &'static str {
    match c {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 | 81 | 82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm, hail",
        _ => "Mixed",
    }
}
