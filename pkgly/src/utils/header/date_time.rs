use chrono::FixedOffset;
use http::HeaderValue;

pub fn date_time_for_header(date_time: &chrono::DateTime<FixedOffset>) -> HeaderValue {
    let date_time = date_time.with_timezone(&chrono::Utc);
    let date_time = date_time.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    HeaderValue::from_str(date_time.as_str())
        .unwrap_or_else(|_| panic!("Failed to convert date time to header"))
}
