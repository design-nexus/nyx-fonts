//! Number formatting for readouts. Everything here is shown in the mono font.

/// "1 song", "12 songs".
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", thousands(n), if n == 1 { one } else { many })
}

/// 1,204
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A calendar date from Unix time: 2 Jan 2024.
pub fn date(unix: i64) -> String {
    let days = unix.div_euclid(86400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}

/// A version for a list: Google's `2025-09-10` as `10 Sep 2025` (`10 Sep` with
/// `year: false`); anything else as it is.
pub fn version(v: &str, year: bool) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let parts: Vec<&str> = v.split('-').collect();
    match parts.as_slice() {
        [y, m, d] if y.len() == 4 => match (m.parse::<usize>(), d.parse::<u32>()) {
            (Ok(m @ 1..=12), Ok(d)) if year => format!("{d} {} {y}", MONTHS[m - 1]),
            (Ok(m @ 1..=12), Ok(d)) => format!("{d} {}", MONTHS[m - 1]),
            _ => v.to_string(),
        },
        _ => v.to_string(),
    }
}

/// A file size: 812 KB, 3.4 MB.
pub fn bytes(n: u64) -> String {
    let n = n as f64;
    if n >= 1e9 {
        format!("{:.1} GB", n / 1e9)
    } else if n >= 1e6 {
        format!("{:.1} MB", n / 1e6)
    } else {
        format!("{:.0} KB", (n / 1e3).max(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(count(1, "font", "fonts"), "1 font");
        assert_eq!(count(1204, "font", "fonts"), "1,204 fonts");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(date(1704209400), "2 Jan 2024");
        assert_eq!(bytes(812_000), "812 KB");
        assert_eq!(bytes(3_400_000), "3.4 MB");
        assert_eq!(version("2025-09-10", true), "10 Sep 2025");
        assert_eq!(version("2025-09-01", false), "1 Sep");
        assert_eq!(version("v3.5.1", false), "v3.5.1");
    }
}
