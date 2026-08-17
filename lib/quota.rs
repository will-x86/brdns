use crate::model::Window;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};

fn start_of_day(dt: DateTime<Utc>) -> DateTime<Utc> {
    dt.with_hour(0)
        .unwrap()
        .with_minute(0)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap()
}

pub fn window_start(now: i64, window: Window) -> i64 {
    let dt = DateTime::<Utc>::from_timestamp(now, 0).expect("unix timestamp out of range");
    match window {
        Window::Hour => (start_of_day(dt) + Duration::hours(dt.hour() as i64)).timestamp(),
        Window::Day => start_of_day(dt).timestamp(),
        Window::Week => start_of_day(dt - Duration::days(dt.weekday().num_days_from_monday() as i64)).timestamp(),
        Window::Month => start_of_day(dt.with_day(1).unwrap()).timestamp(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hour_window() {
        assert_eq!(window_start(3_600, Window::Hour), 3_600);
        assert_eq!(window_start(3_600 + 59, Window::Hour), 3_600);
        assert_eq!(window_start(7_200 - 1, Window::Hour), 3_600);
    }

    #[test]
    fn day_window() {
        let day = 1704067200;
        assert_eq!(window_start(day, Window::Day), day);
        assert_eq!(window_start(day + 86399, Window::Day), day);
    }

    #[test]
    fn week_window_starts_monday() {
        let monday = 1704067200;
        assert_eq!(window_start(monday, Window::Week), monday);
        assert_eq!(window_start(monday + 2 * 86400, Window::Week), monday);
    }

    #[test]
    fn month_window() {
        let mar1 = 1709251200;
        assert_eq!(window_start(mar1 + 14 * 86400, Window::Month), mar1);
    }
}