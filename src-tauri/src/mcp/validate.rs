use crate::store::types::EntryInput;

/// Checks a `YYYY-MM` date with a month in 01..=12.
fn check_month(field: &str, v: &str) -> Result<(), String> {
    let ok = v.len() == 7
        && v.as_bytes()[4] == b'-'
        && v[..4].chars().all(|c| c.is_ascii_digit())
        && matches!(v[5..].parse::<u8>(), Ok(1..=12));
    if ok {
        Ok(())
    } else {
        Err(format!("{field} must be YYYY-MM (e.g. 2022-01), got {v:?}"))
    }
}

/// Trims every string, drops blank bullets/skills and validates dates.
pub fn normalize_entry(mut e: EntryInput) -> Result<EntryInput, String> {
    for s in [
        &mut e.title,
        &mut e.org,
        &mut e.location,
        &mut e.start_date,
        &mut e.description,
    ] {
        *s = s.trim().to_string();
    }
    e.end_date = e
        .end_date
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());
    e.bullets = e
        .bullets
        .iter()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .collect();
    e.skills = e
        .skills
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if e.title.is_empty() {
        return Err("title must not be empty".into());
    }
    check_month("start_date", &e.start_date)?;
    if let Some(end) = &e.end_date {
        check_month("end_date", end)?;
        // YYYY-MM sorts lexicographically.
        if end.as_str() < e.start_date.as_str() {
            return Err(format!(
                "end_date {end} is before start_date {}",
                e.start_date
            ));
        }
    }
    Ok(e)
}
