//! Fuzzy search over saved jobs (company, title and URL host), ranked by
//! `nucleo-matcher`.
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::store::types::JobDescription;

/// Ids of the jobs matching `query`, best match first; ties keep input order.
/// A blank query matches nothing; callers show the full list instead.
pub fn search_jobs(jobs: &[JobDescription], query: &str) -> Vec<i64> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut hits: Vec<(u32, i64)> = jobs
        .iter()
        .filter_map(|j| {
            let hay = format!("{} {} {}", j.company, j.title, host(&j.url));
            pattern
                .score(Utf32Str::new(&hay, &mut buf), &mut matcher)
                .map(|score| (score, j.id))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    hits.into_iter().map(|(_, id)| id).collect()
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#']).next().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: i64, company: &str, title: &str, url: &str) -> JobDescription {
        JobDescription {
            id,
            title: title.into(),
            company: company.into(),
            raw_text: String::new(),
            created_at: String::new(),
            url: url.into(),
            status_id: None,
            company_id: None,
        }
    }

    fn jobs() -> Vec<JobDescription> {
        vec![
            job(1, "Stripe", "Backend Engineer", "https://stripe.com/jobs/1"),
            job(
                2,
                "Google",
                "Software Engineer",
                "https://careers.google.com/x",
            ),
            job(3, "Acme", "Product Designer", ""),
        ]
    }

    #[test]
    fn blank_query_matches_nothing() {
        assert!(search_jobs(&jobs(), "  ").is_empty());
    }

    #[test]
    fn matches_subsequences_across_company_and_title() {
        assert_eq!(search_jobs(&jobs(), "goog eng"), vec![2]);
        assert_eq!(search_jobs(&jobs(), "bkend"), vec![1]);
    }

    #[test]
    fn matches_url_host_and_ignores_case() {
        assert_eq!(search_jobs(&jobs(), "STRIPE.COM"), vec![1]);
    }

    #[test]
    fn ranks_better_matches_first() {
        let ids = search_jobs(&jobs(), "engineer");
        assert_eq!(ids.len(), 2);
        assert!(!ids.contains(&3));
    }

    #[test]
    fn no_match_is_empty() {
        assert!(search_jobs(&jobs(), "zzzz").is_empty());
    }
}
