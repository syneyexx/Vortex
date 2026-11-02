//! Minimale cron ("m h dom mon dow", lokale tijd): `*`, `*/n`, `a-b`, `a-b/n`, lijsten.
//! Zondag = 0 of 7.

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike};

#[derive(Debug, Clone, PartialEq)]
pub struct Cron {
    min: Vec<bool>,
    hour: Vec<bool>,
    dom: Vec<bool>,
    mon: Vec<bool>,
    dow: Vec<bool>,
    dom_any: bool,
    dow_any: bool,
}

fn field(s: &str, lo: u32, hi: u32) -> Result<Vec<bool>, String> {
    let mut v = vec![false; hi as usize + 1];
    for part in s.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, st)) => (r, st.parse::<u32>().map_err(|_| format!("stap {st}"))?.max(1)),
            None => (part, 1),
        };
        let (a, b) = if range == "*" {
            (lo, hi)
        } else if let Some((a, b)) = range.split_once('-') {
            (a.parse().map_err(|_| format!("waarde {a}"))?, b.parse().map_err(|_| format!("waarde {b}"))?)
        } else {
            let x: u32 = range.parse().map_err(|_| format!("waarde {range}"))?;
            (x, if part.contains('/') { hi } else { x })
        };
        if a < lo || b > hi || a > b {
            return Err(format!("buiten bereik: {part} ({lo}-{hi})"));
        }
        let mut x = a;
        while x <= b {
            v[x as usize] = true;
            x += step;
        }
    }
    Ok(v)
}

impl Cron {
    pub fn parse(s: &str) -> Result<Self, String> {
        let f: Vec<&str> = s.split_whitespace().collect();
        if f.len() != 5 {
            return Err("cron = 5 velden: minuut uur dag maand weekdag".into());
        }
        let mut dow = field(f[4], 0, 7)?;
        if dow[7] {
            dow[0] = true;
        }
        Ok(Self {
            min: field(f[0], 0, 59)?,
            hour: field(f[1], 0, 23)?,
            dom: field(f[2], 1, 31)?,
            mon: field(f[3], 1, 12)?,
            dow,
            dom_any: f[2] == "*",
            dow_any: f[4] == "*",
        })
    }

    pub fn matches<Tz: TimeZone>(&self, t: &DateTime<Tz>) -> bool {
        let day = match (self.dom_any, self.dow_any) {
            (true, true) => true,
            (true, false) => self.dow[t.weekday().num_days_from_sunday() as usize],
            (false, true) => self.dom[t.day() as usize],
            // Klassieke cron: dag-van-maand OF weekdag.
            (false, false) => self.dom[t.day() as usize] || self.dow[t.weekday().num_days_from_sunday() as usize],
        };
        day && self.min[t.minute() as usize] && self.hour[t.hour() as usize] && self.mon[t.month() as usize]
    }

    /// Eerstvolgende moment ná `after` (minuutprecisie, max ~1 jaar vooruit).
    pub fn next_after(&self, after: DateTime<Local>) -> Option<DateTime<Local>> {
        let mut t = after.with_second(0)?.with_nanosecond(0)? + Duration::minutes(1);
        let end = after + Duration::days(370);
        while t <= end {
            if !self.mon[t.month() as usize] || !self.hour[t.hour() as usize] {
                // Snel door naar het volgende uur.
                t = (t + Duration::hours(1)).with_minute(0)?;
                continue;
            }
            if self.matches(&t) {
                return Some(t);
            }
            t += Duration::minutes(1);
        }
        None
    }

    /// Was er een moment in (`since`, `now`]? (inhalen na een pauze: één keer vuren.)
    pub fn due(&self, since: DateTime<Local>, now: DateTime<Local>) -> bool {
        self.next_after(since).is_some_and(|n| n <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).single().unwrap_or_else(Local::now)
    }

    #[test]
    fn parse_and_next() {
        let c = Cron::parse("30 7 * * *").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(c.next_after(at(2026, 11, 2, 7, 29)), Some(at(2026, 11, 2, 7, 30)));
        assert_eq!(c.next_after(at(2026, 11, 2, 7, 30)), Some(at(2026, 11, 3, 7, 30)));
        let w = Cron::parse("0 9 * * 1-5").unwrap_or_else(|e| panic!("{e}"));
        // 2026-11-07 = zaterdag → maandag 9 november.
        assert_eq!(w.next_after(at(2026, 11, 6, 10, 0)), Some(at(2026, 11, 9, 9, 0)));
        let s = Cron::parse("*/15 * * * *").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.next_after(at(2026, 11, 2, 10, 1)), Some(at(2026, 11, 2, 10, 15)));
        assert!(Cron::parse("61 * * * *").is_err());
        assert!(Cron::parse("* *").is_err());
        // Inhalen: PC stond uit tijdens 07:30 → bij opstarten om 10:00 is de trigger due.
        assert!(c.due(at(2026, 11, 2, 6, 0), at(2026, 11, 2, 10, 0)));
        assert!(!c.due(at(2026, 11, 2, 8, 0), at(2026, 11, 2, 10, 0)));
    }
}
