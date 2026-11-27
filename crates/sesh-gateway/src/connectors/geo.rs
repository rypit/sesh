//! `geo` connector: forecasts (Open-Meteo), geocoding and nearby places
//! (Nominatim), defaulting to the household's home location.

use super::search::Query;
use serde_json::{Value, json};
use sesh_core::Home;

pub fn forecast_request(lat: f64, lon: f64, days: u64) -> (&'static str, Query) {
    (
        "/v1/forecast",
        vec![
            ("latitude", format!("{lat:.4}")),
            ("longitude", format!("{lon:.4}")),
            (
                "daily",
                "temperature_2m_max,temperature_2m_min,precipitation_sum,weather_code".into(),
            ),
            ("forecast_days", days.to_string()),
            ("timezone", "auto".into()),
        ],
    )
}

fn summary(code: i64) -> &'static str {
    match code {
        0 => "clear",
        1..=3 => "partly cloudy",
        45 | 48 => "fog",
        51..=57 => "drizzle",
        61..=67 => "rain",
        71..=77 => "snow",
        80..=82 => "showers",
        85 | 86 => "snow showers",
        95..=99 => "thunderstorm",
        _ => "unknown",
    }
}

pub fn parse_forecast(body: &Value, lat: f64, lon: f64) -> Value {
    let d = &body["daily"];
    let dates = d["time"].as_array().cloned().unwrap_or_default();
    let days: Vec<Value> = dates
        .iter()
        .enumerate()
        .map(|(i, date)| {
            json!({
                "date": date,
                "max_c": d["temperature_2m_max"][i],
                "min_c": d["temperature_2m_min"][i],
                "precipitation_mm": d["precipitation_sum"][i],
                "summary": summary(d["weather_code"][i].as_i64().unwrap_or(-1)),
            })
        })
        .collect();
    json!({"latitude": lat, "longitude": lon, "days": days})
}

pub fn geocode_request(query: &str) -> (&'static str, Query) {
    (
        "/search",
        vec![
            ("q", query.to_string()),
            ("format", "jsonv2".into()),
            ("limit", "5".into()),
        ],
    )
}

/// Only plain amenity words: Nominatim's `amenity` filter takes one.
pub fn valid_kind(kind: &str) -> bool {
    !kind.is_empty() && kind.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

pub fn nearby_request(kind: &str, home: &Home, radius_km: f64) -> (&'static str, Query) {
    let dlat = radius_km / 111.0;
    let dlon = radius_km / (111.0 * home.latitude.to_radians().cos().max(0.01));
    let viewbox = format!(
        "{:.4},{:.4},{:.4},{:.4}",
        home.longitude - dlon,
        home.latitude + dlat,
        home.longitude + dlon,
        home.latitude - dlat
    );
    (
        "/search",
        vec![
            ("amenity", kind.to_string()),
            ("viewbox", viewbox),
            ("bounded", "1".into()),
            ("format", "jsonv2".into()),
            ("limit", "10".into()),
        ],
    )
}

pub fn parse_places(body: &Value, max: usize) -> Value {
    let items = body.as_array().cloned().unwrap_or_default();
    Value::Array(
        items
            .iter()
            .take(max)
            .map(|p| {
                let num = |k: &str| {
                    p[k].as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .or(p[k].as_f64())
                };
                json!({
                    "name": p["display_name"].as_str().or(p["name"].as_str()).unwrap_or(""),
                    "latitude": num("lat"),
                    "longitude": num("lon"),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forecast_parses_days() {
        let body = json!({"daily": {"time": ["2026-11-27"], "temperature_2m_max": [5.0],
            "temperature_2m_min": [1.0], "precipitation_sum": [2.5], "weather_code": [61]}});
        let f = parse_forecast(&body, 59.9, 10.7);
        assert_eq!(f["days"][0]["summary"], "rain");
        assert_eq!(f["days"][0]["max_c"], 5.0);
        let (path, q) = forecast_request(59.91234, 10.75, 3);
        assert_eq!(path, "/v1/forecast");
        assert!(q.contains(&("latitude", "59.9123".to_string())));
    }

    #[test]
    fn places_parse_string_coordinates() {
        let body = json!([{"display_name": "Cafe", "lat": "59.9", "lon": "10.7"}]);
        assert_eq!(
            parse_places(&body, 5),
            json!([{"name": "Cafe", "latitude": 59.9, "longitude": 10.7}])
        );
    }

    #[test]
    fn nearby_builds_a_bounded_box_and_rejects_odd_kinds() {
        let home = Home {
            latitude: 60.0,
            longitude: 10.0,
        };
        let (_, q) = nearby_request("cafe", &home, 1.11);
        assert!(q.contains(&("bounded", "1".to_string())));
        assert!(
            q.iter()
                .any(|(k, v)| *k == "viewbox" && v.starts_with("9.98"))
        );
        assert!(valid_kind("fast_food"));
        assert!(!valid_kind("cafe&x=1"));
        assert!(!valid_kind(""));
    }
}
