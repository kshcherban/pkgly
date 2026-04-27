#![allow(clippy::expect_used)]
use super::*;
use chrono::TimeZone;

#[test]
fn repository_without_previous_run_is_due() {
    let now = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap();

    assert!(is_due(now, None));
}

#[test]
fn repository_is_not_due_before_twenty_four_hours() {
    let now = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap();
    let previous = (now - chrono::Duration::hours(23)).fixed_offset();

    assert!(!is_due(now, Some(previous)));
}

#[test]
fn repository_is_due_at_twenty_four_hours() {
    let now = Utc.with_ymd_and_hms(2026, 4, 27, 12, 0, 0).unwrap();
    let previous = (now - chrono::Duration::hours(24)).fixed_offset();

    assert!(is_due(now, Some(previous)));
}
