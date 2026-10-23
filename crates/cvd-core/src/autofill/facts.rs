use serde::Serialize;

use super::ApplyDefaults;
use crate::store::types::{Entry, EntryKind, Profile};

/// One thing cvd knows about the user, in every spelling a form might want.
/// The harness offers `variants` as candidates; the model picks one or none.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[ts(rename = "ApplyFact")]
pub struct Fact {
    pub key: &'static str,
    /// Non-empty, deduplicated, preferred spelling first.
    pub variants: Vec<String>,
}

/// Builds every fact that has a value. `today` is `(year, month)`.
pub fn build_facts(
    profile: &Profile,
    entries: &[Entry],
    defaults: &ApplyDefaults,
    today: (i32, u32),
) -> Vec<Fact> {
    let mut out = Vec::new();
    let mut push = |key: &'static str, variants: Vec<String>| {
        let mut v: Vec<String> = Vec::new();
        for s in variants.into_iter().map(|s| s.trim().to_string()) {
            if !s.is_empty() && !v.contains(&s) {
                v.push(s);
            }
        }
        if !v.is_empty() {
            out.push(Fact { key, variants: v });
        }
    };

    // Identity. "Diego Hernández Herrera": first token is the first name, the rest the
    // last name, which keeps both Spanish surnames together. Defaults override the split.
    let full = profile
        .name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let (split_first, split_last) = match full.split_once(' ') {
        Some((f, l)) => (f.to_string(), l.to_string()),
        None => (full.clone(), String::new()),
    };
    let first = or(&defaults.first_name, &split_first);
    let last = or(&defaults.last_name, &split_last);
    let full = if defaults.first_name.is_empty() && defaults.last_name.is_empty() {
        full
    } else {
        format!("{first} {last}").trim().to_string()
    };
    push("first_name", with_ascii(&first));
    push("last_name", with_ascii(&last));
    push("full_name", with_ascii(&full));

    // Contact.
    push("email", vec![profile.email.clone()]);
    push("phone", phone_variants(&profile.phone));

    // Location: structured defaults first, the free-text profile location as a fallback.
    push("city", with_ascii(&defaults.city));
    push("region", with_ascii(&defaults.region));
    push("country", with_ascii(&defaults.country));
    // "Madrid, Madrid, Spain" reads as a typo; repeated parts are dropped.
    let mut parts: Vec<&str> = Vec::new();
    for p in [&defaults.city, &defaults.region, &defaults.country] {
        if !p.is_empty() && !parts.iter().any(|q| q.eq_ignore_ascii_case(p)) {
            parts.push(p);
        }
    }
    let joined = parts.join(", ");
    let mut loc = with_ascii(&joined);
    loc.extend(with_ascii(&profile.location));
    push("location_full", loc);

    // Links, classified by host.
    let mut website = Vec::new();
    for link in &profile.links {
        let link = link.trim();
        if link.is_empty() {
            continue;
        }
        let host = host_of(link);
        let key = if host.ends_with("linkedin.com") {
            "linkedin"
        } else if host.ends_with("github.com") {
            "github"
        } else {
            website.extend(url_variants(link));
            continue;
        };
        push(key, url_variants(link));
    }
    push("website", website);

    // Experience.
    let mut jobs: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.kind == EntryKind::Experience && !e.archived)
        .collect();
    jobs.sort_by(|a, b| b.start_date.cmp(&a.start_date));
    if let Some(current) = jobs.iter().find(|e| e.end_date.is_none()) {
        push("current_title", vec![current.title.clone()]);
        push("current_company", vec![current.org.clone()]);
    }
    if let Some(years) = years_experience(&jobs, today) {
        push("years_experience", vec![years.to_string()]);
    }

    // Preferences.
    push("work_authorization", yes_no(defaults.authorized_to_work));
    push("needs_sponsorship", yes_no(defaults.needs_sponsorship));
    push("willing_to_relocate", yes_no(defaults.willing_to_relocate));
    push("notice_period", vec![defaults.notice_period.clone()]);
    push("earliest_start", vec![defaults.earliest_start.clone()]);
    push(
        "salary_expectation",
        vec![defaults.salary_expectation.clone()],
    );

    out
}

fn or(preferred: &str, fallback: &str) -> String {
    if preferred.trim().is_empty() {
        fallback.to_string()
    } else {
        preferred.trim().to_string()
    }
}

fn yes_no(v: Option<bool>) -> Vec<String> {
    match v {
        Some(true) => vec!["Yes".into()],
        Some(false) => vec!["No".into()],
        None => vec![],
    }
}

/// The value, then its accent-free spelling when that differs.
fn with_ascii(s: &str) -> Vec<String> {
    vec![s.to_string(), fold_accents(s)]
}

/// Strips Latin diacritics (á → a, ñ → n). Other characters pass through.
pub(crate) fn fold_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'O',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ç' => 'c',
            'Ç' => 'C',
            other => other,
        })
        .collect()
}

/// As written, `+<digits>` when it had a country code, and digits only.
/// No national-format guessing: that needs per-country rules cvd doesn't have.
fn phone_variants(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return vec![];
    }
    let mut v = vec![raw.to_string()];
    if raw.starts_with('+') {
        v.push(format!("+{digits}"));
    }
    v.push(digits);
    v
}

fn host_of(link: &str) -> String {
    let rest = link.split_once("://").map_or(link, |(_, r)| r);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_ascii_lowercase()
}

/// URL fields usually reject a bare host, so the `https://` form comes first.
fn url_variants(link: &str) -> Vec<String> {
    if link.contains("://") {
        vec![link.to_string()]
    } else {
        vec![format!("https://{link}"), link.to_string()]
    }
}

/// Whole years covered by the union of experience periods, overlaps counted once.
fn years_experience(jobs: &[&Entry], today: (i32, u32)) -> Option<i64> {
    let now = today.0 as i64 * 12 + today.1 as i64 - 1;
    let mut spans: Vec<(i64, i64)> = jobs
        .iter()
        .filter_map(|e| {
            let start = month_index(&e.start_date)?;
            let end = e.end_date.as_deref().and_then(month_index).unwrap_or(now);
            // Inclusive end month: Jan..Dec of one year is 12 months.
            (end >= start).then_some((start, end + 1))
        })
        .collect();
    if spans.is_empty() {
        return None;
    }
    spans.sort();
    let mut months = 0;
    let (mut cur_start, mut cur_end) = spans[0];
    for &(s, e) in &spans[1..] {
        if s <= cur_end {
            cur_end = cur_end.max(e);
        } else {
            months += cur_end - cur_start;
            (cur_start, cur_end) = (s, e);
        }
    }
    months += cur_end - cur_start;
    Some(months / 12)
}

fn month_index(ym: &str) -> Option<i64> {
    let (y, m) = ym.split_once('-')?;
    let (y, m): (i64, i64) = (y.parse().ok()?, m.parse().ok()?);
    (1..=12).contains(&m).then_some(y * 12 + m - 1)
}
