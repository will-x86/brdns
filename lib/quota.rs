//! Quota window arithmetic.
//!
//! Limits count queries per rule per window (hour/day/week/month). A window's
//! start is a unix timestamp (seconds) aligned to that window boundary, used as
//! the counter key. Pure calendar math — no time-library dependency.

use crate::model::Window;

/// Days since 1970-01-01 for a civil (proleptic Gregorian) date.
/// Howard Hinnant's `days_from_civil` algorithm.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (m as i64 + if m > 2 { -3 } else { 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Civil date from days since 1970-01-01 (inverse of [`days_from_civil`]).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Start of the window containing `now` (unix seconds).
///
/// Weeks start on Monday (UTC).
pub fn window_start(now: i64, window: Window) -> i64 {
    match window {
        Window::Hour => (now / 3600) * 3600,
        Window::Day => {
            let (y, m, d) = civil_from_days(now / 86400);
            days_from_civil(y, m, d) * 86400
        }
        Window::Week => {
            let days = now / 86400;
            let weekday = (days + 3).rem_euclid(7); // 0 = Monday
            (days - weekday) * 86400
        }
        Window::Month => {
            let (y, m, _) = civil_from_days(now / 86400);
            days_from_civil(y, m, 1) * 86400
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_anchors() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 1, 1), 10957);
        assert_eq!(days_from_civil(2024, 1, 1), 19723);
        // Round-trips.
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2024, 1, 1), (2023, 12, 31)] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d));
        }
    }

    #[test]
    fn hour_window() {
        assert_eq!(window_start(3_600, Window::Hour), 3_600);
        assert_eq!(window_start(3_600 + 59, Window::Hour), 3_600);
        assert_eq!(window_start(7_200 - 1, Window::Hour), 3_600);
    }

    #[test]
    fn day_window() {
        // 2024-01-01 00:00:00 UTC = 1704067200.
        let day = 1704067200;
        assert_eq!(window_start(day, Window::Day), day);
        assert_eq!(window_start(day + 86399, Window::Day), day);
    }

    #[test]
    fn week_window_starts_monday() {
        // 2024-01-01 was a Monday; its week starts there.
        let monday = 1704067200;
        assert_eq!(window_start(monday, Window::Week), monday);
        // Wednesday 2024-01-03 still in that week.
        assert_eq!(window_start(monday + 2 * 86400, Window::Week), monday);
    }

    #[test]
    fn month_window() {
        // 2024-03-15 is inside March 2024; month starts 2024-03-01.
        let mar1 = 1709251200; // 2024-03-01 00:00:00 UTC
        assert_eq!(window_start(mar1 + 14 * 86400, Window::Month), mar1);
    }
}
