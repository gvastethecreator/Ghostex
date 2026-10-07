//! Temporal values: ordering ISO-like dates and labelling them.

/// A parsed `YYYY[-MM[-DD[THH:MM[:SS]]]]` value. `parts` counts how much of it was given
/// (1 = year, 2 = month, 3 = day, 4 = time).
struct Stamp {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    parts: u8,
}

fn digits(bytes: &[u8], at: usize, count: usize) -> Option<u32> {
    let slice = bytes.get(at..at + count)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return None;
    }
    slice
        .iter()
        .try_fold(0u32, |acc, d| Some(acc * 10 + u32::from(d - b'0')))
}

fn parse(text: &str) -> Option<Stamp> {
    let b = text.trim().as_bytes();
    let year = i64::from(digits(b, 0, 4)?);
    let mut stamp = Stamp {
        year,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
        parts: 1,
    };
    if b.len() == 4 {
        return Some(stamp);
    }
    let sep = *b.get(4)?;
    if sep != b'-' && sep != b'/' {
        return None;
    }
    stamp.month = digits(b, 5, 2)?;
    stamp.parts = 2;
    if !(1..=12).contains(&stamp.month) {
        return None;
    }
    if b.len() == 7 {
        return Some(stamp);
    }
    if *b.get(7)? != sep {
        return None;
    }
    stamp.day = digits(b, 8, 2)?;
    stamp.parts = 3;
    if !(1..=31).contains(&stamp.day) {
        return None;
    }
    if b.len() == 10 {
        return Some(stamp);
    }
    if !matches!(b.get(10), Some(b'T') | Some(b't') | Some(b' ')) {
        return None;
    }
    stamp.hour = digits(b, 11, 2)?;
    if *b.get(13)? != b':' {
        return None;
    }
    stamp.minute = digits(b, 14, 2)?;
    if b.get(16) == Some(&b':') {
        stamp.second = digits(b, 17, 2)?;
    }
    stamp.parts = 4;
    Some(stamp)
}

/// A key that sorts chronologically, or `None` when the text isn't a date this crate reads.
pub(crate) fn sort_key(text: &str) -> Option<String> {
    let s = parse(text)?;
    Some(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        s.year, s.month, s.day, s.hour, s.minute, s.second
    ))
}

/// The label for a temporal string: the original text, shortened to `YYYY-MM-DD` when it
/// carries a midnight time.
pub(crate) fn label(text: &str) -> String {
    let trimmed = text.trim();
    match parse(trimmed) {
        Some(s) if s.parts == 4 && s.hour == 0 && s.minute == 0 && s.second == 0 => {
            format!("{:04}-{:02}-{:02}", s.year, s.month, s.day)
        }
        _ => trimmed.to_string(),
    }
}

/// The label for a temporal number: a year when it looks like one, else epoch milliseconds.
pub(crate) fn number_label(ms: f64) -> String {
    if ms.fract() == 0.0 && (1000.0..=9999.0).contains(&ms) {
        return format!("{}", ms as i64);
    }
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        return crate::text::format_plain(ms);
    }
    let total_seconds = (ms / 1000.0).floor() as i64;
    let days = total_seconds.div_euclid(86_400);
    let secs = total_seconds.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    if secs == 0 {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!(
            "{y:04}-{m:02}-{d:02} {:02}:{:02}",
            secs / 3600,
            (secs % 3600) / 60
        )
    }
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
