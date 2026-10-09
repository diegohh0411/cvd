use super::*;

#[test]
fn normalize_strips_case_accents_punctuation_and_suffixes() {
    assert_eq!(normalize_name("Nubank S.A. de C.V."), "nubank");
    assert_eq!(normalize_name("  NUBANK  "), "nubank");
    assert_eq!(normalize_name("Société Générale"), "societe generale");
    assert_eq!(normalize_name("Google, LLC"), "google");
    assert_eq!(normalize_name("Acme Inc."), "acme");
    assert_eq!(normalize_name("Siemens GmbH"), "siemens");
    assert_eq!(normalize_name(""), "");
}

#[test]
fn domain_from_url_skips_job_boards() {
    assert_eq!(domain_from_url("https://www.stripe.com/jobs/123"), Some("stripe.com".into()));
    assert_eq!(domain_from_url("https://www.linkedin.com/jobs/view/1"), None);
    assert_eq!(domain_from_url("https://boards.greenhouse.io/acme/jobs/1"), None);
    assert_eq!(domain_from_url("not a url"), None);
    assert_eq!(domain_from_url(""), None);
}

mod policy_tests {
    use super::super::*;
    use crate::store::types::{Company, CompanyProfile, CompanyStatus};
    use chrono::{Duration, TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, 12, 0, 0).unwrap()
    }

    fn company(status: CompanyStatus, researched_days_ago: Option<i64>) -> Company {
        Company {
            id: 1,
            name: "Acme".into(),
            normalized_name: "acme".into(),
            domain: None,
            profile: CompanyProfile::default(),
            status,
            researched_at: researched_days_ago.map(|d| format_ts(now() - Duration::days(d))),
            model_id: String::new(),
            fail_count: 0,
            last_error: String::new(),
            retry_after: None,
            edited_at: None,
        }
    }

    #[test]
    fn fresh_within_ttl() {
        assert_eq!(need(&company(CompanyStatus::Ok, Some(29)), now()), Need::Fresh);
    }

    #[test]
    fn stale_after_30_days() {
        assert_eq!(need(&company(CompanyStatus::Ok, Some(31)), now()), Need::Research);
        assert_eq!(need(&company(CompanyStatus::Pending, None), now()), Need::Research);
    }

    #[test]
    fn edited_rows_still_expire() {
        let mut c = company(CompanyStatus::Ok, Some(31));
        c.edited_at = Some(format_ts(now()));
        assert_eq!(need(&c, now()), Need::Research);
    }

    #[test]
    fn failed_backs_off_then_retries() {
        let mut c = company(CompanyStatus::Failed, None);
        c.retry_after = Some(format_ts(now() + Duration::hours(1)));
        assert_eq!(need(&c, now()), Need::BackingOff);
        c.retry_after = Some(format_ts(now() - Duration::minutes(1)));
        assert_eq!(need(&c, now()), Need::Research);
    }

    #[test]
    fn stale_ok_profile_with_failed_refresh_backs_off() {
        // A refresh that failed keeps the old profile; it must not be retried on every match.
        let mut c = company(CompanyStatus::Failed, Some(40));
        c.retry_after = Some(format_ts(now() + Duration::hours(2)));
        assert_eq!(need(&c, now()), Need::BackingOff);
    }

    #[test]
    fn backoff_caps_at_7_days() {
        assert_eq!(backoff(1), std::time::Duration::from_secs(3600));
        assert_eq!(backoff(4), std::time::Duration::from_secs(8 * 3600));
        assert_eq!(backoff(20), std::time::Duration::from_secs(7 * 24 * 3600));
    }

    #[test]
    fn timestamps_roundtrip_store_format() {
        assert_eq!(format_ts(now()), "2026-10-08 12:00:00");
        assert_eq!(parse_ts("2026-10-08 12:00:00"), Some(now()));
        assert_eq!(parse_ts("garbage"), None);
    }
}

mod render_tests {
    use super::super::*;
    use crate::store::types::{CompanyProfile, Source};

    #[test]
    fn render_omits_empty_fields() {
        let p = CompanyProfile { industry: "Fintech".into(), ..Default::default() };
        assert_eq!(render("Acme", Some("acme.com"), &p), "Name: Acme (acme.com)\nIndustry: Fintech");
    }

    #[test]
    fn render_caps_length() {
        let long = "x".repeat(5000);
        let p = CompanyProfile {
            summary: long.clone(),
            industry: long.clone(),
            size: long.clone(),
            stage: long.clone(),
            headquarters: long.clone(),
            remote_policy: long.clone(),
            products: long.clone(),
            tech_stack: vec![long.clone(); 20],
            sources: vec![],
        };
        let out = render("Acme", None, &p);
        assert!(out.len() / 3 <= 400, "{} chars", out.len());
        assert!(out.starts_with("Name: Acme\n"));
    }

    #[test]
    fn render_never_includes_sources() {
        let p = CompanyProfile {
            industry: "Fintech".into(),
            sources: vec![Source { url: "https://secret.example".into(), title: "S".into() }],
            ..Default::default()
        };
        assert!(!render("Acme", None, &p).contains("secret.example"));
    }

    #[test]
    fn render_lists_tech_stack() {
        let p = CompanyProfile { tech_stack: vec!["Rust".into(), "Go".into()], ..Default::default() };
        assert_eq!(render("Acme", None, &p), "Name: Acme\nTech stack: Rust, Go");
    }
}
