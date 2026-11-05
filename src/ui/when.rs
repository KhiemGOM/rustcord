//! What `@time` in the composer understands: "5pm", "tomorrow 17:30", "friday", "in 2 hours",
//! "2026-12-25 18:00", and the like, read in the local time zone.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};

/// The styles `<t:unix:style>` takes, in the order discord lists them, with their names.
pub const STYLES: [(char, &str); 7] = [
    ('t', "Short Time"),
    ('T', "Long Time"),
    ('d', "Short Date"),
    ('D', "Long Date"),
    ('f', "Short Date/Time"),
    ('F', "Long Date/Time"),
    ('R', "Relative Time"),
];

fn clock(w: &str) -> Option<NaiveTime> {
    match w {
        "noon" => return NaiveTime::from_hms_opt(12, 0, 0),
        "midnight" => return NaiveTime::from_hms_opt(0, 0, 0),
        _ => {}
    }
    let (body, pm) = match (w.strip_suffix("pm"), w.strip_suffix("am")) {
        (Some(b), _) => (b, Some(true)),
        (_, Some(b)) => (b, Some(false)),
        _ => (w, None),
    };
    let (h, m) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None => (body.parse::<u32>().ok().filter(|_| pm.is_some())?, 0),
    };
    let h = match pm {
        Some(true) if (1..=12).contains(&h) => h % 12 + 12,
        Some(false) if (1..=12).contains(&h) => h % 12,
        Some(_) => return None,
        None => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

fn day(w: &str, today: NaiveDate) -> Option<NaiveDate> {
    match w {
        "today" => return Some(today),
        "tomorrow" => return Some(today + Duration::days(1)),
        "yesterday" => return Some(today - Duration::days(1)),
        _ => {}
    }
    if let Ok(wd) = w.parse::<chrono::Weekday>() {
        let ahead = (wd.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
        return Some(today + Duration::days(ahead as i64));
    }
    if let Ok(d) = NaiveDate::parse_from_str(w, "%Y-%m-%d") {
        return Some(d);
    }
    let (m, d) = w.split_once('/')?;
    NaiveDate::from_ymd_opt(today.year(), m.parse().ok()?, d.parse().ok()?)
}

fn span(n: &str, unit: &str) -> Option<Duration> {
    let n: i64 = n.parse().ok()?;
    Some(match unit.trim_end_matches('s') {
        "sec" | "second" => Duration::seconds(n),
        "min" | "minute" => Duration::minutes(n),
        "h" | "hr" | "hour" => Duration::hours(n),
        "d" | "day" => Duration::days(n),
        "w" | "wk" | "week" => Duration::weeks(n),
        _ => return None,
    })
}

pub fn parse(q: &str, now: DateTime<Local>) -> Option<DateTime<Local>> {
    let q = q.trim().to_lowercase().replace(" pm", "pm").replace(" am", "am");
    if q.is_empty() || q == "now" {
        return Some(now);
    }
    let w: Vec<&str> = q.split_whitespace().filter(|w| !matches!(*w, "at" | "on")).collect();
    match w[..] {
        ["in", n, unit] => return Some(now + span(n, unit)?),
        [n, unit, "ago"] => return Some(now - span(n, unit)?),
        _ => {}
    }
    let today = now.date_naive();
    let (mut date, mut time) = (None, None);
    for t in &w {
        if let Some(d) = day(t, today) {
            date = Some(d);
        } else if let Some(c) = clock(t) {
            time = Some(c);
        } else {
            return None;
        }
    }
    let t = time.or_else(|| date.and_then(|_| NaiveTime::from_hms_opt(12, 0, 0)))?;
    Local.from_local_datetime(&date.unwrap_or(today).and_time(t)).single()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(q: &str) -> Option<String> {
        let now = Local.with_ymd_and_hms(2026, 11, 6, 10, 0, 0).single()?;
        parse(q, now).map(|t| t.format("%Y-%m-%d %H:%M").to_string())
    }

    #[test]
    fn reads_what_people_type() {
        assert_eq!(at("").as_deref(), Some("2026-11-06 10:00"));
        assert_eq!(at("5pm").as_deref(), Some("2026-11-06 17:00"));
        assert_eq!(at("5 pm").as_deref(), Some("2026-11-06 17:00"));
        assert_eq!(at("12am").as_deref(), Some("2026-11-06 00:00"));
        assert_eq!(at("tomorrow at 7:30am").as_deref(), Some("2026-11-07 07:30"));
        assert_eq!(at("tomorrow").as_deref(), Some("2026-11-07 12:00"));
        assert_eq!(at("monday 9:00").as_deref(), Some("2026-11-09 09:00"));
        assert_eq!(at("in 2 hours").as_deref(), Some("2026-11-06 12:00"));
        assert_eq!(at("3 days ago").as_deref(), Some("2026-11-03 10:00"));
        assert_eq!(at("2026-12-25 18:00").as_deref(), Some("2026-12-25 18:00"));
        assert_eq!(at("12/25 noon").as_deref(), Some("2026-12-25 12:00"));
    }

    #[test]
    fn rejects_the_rest() {
        assert_eq!(at("banana"), None);
        assert_eq!(at("13pm"), None);
        assert_eq!(at("25:00"), None);
    }
}
