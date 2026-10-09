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
