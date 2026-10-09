//! Cache keys for companies: a normalized name and, when known, a domain.

/// Legal-form suffixes, as normalized words. Longest first so multi-word
/// forms win over their tails.
const SUFFIXES: &[&str] = &[
    "s de rl de cv",
    "s a de c v",
    "sa de cv",
    "corporation",
    "s a",
    "gmbh",
    "corp",
    "inc",
    "llc",
    "ltd",
    "plc",
    "sas",
    "srl",
    "spa",
    "pty",
    "ag",
    "bv",
    "co",
    "sa",
];

/// Hosts that serve postings for many companies, so they never identify one.
const JOB_BOARDS: &[&str] = &[
    "linkedin.com",
    "indeed.com",
    "greenhouse.io",
    "lever.co",
    "workable.com",
    "ashbyhq.com",
    "myworkdayjobs.com",
    "glassdoor.com",
    "occ.com.mx",
    "computrabajo.com",
];

fn fold_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        'ý' | 'ÿ' => 'y',
        c => c,
    }
}

/// Lowercase, accents folded, punctuation dropped, legal suffixes stripped.
pub fn normalize_name(name: &str) -> String {
    let cleaned: String = name
        .to_lowercase()
        .chars()
        .map(fold_accent)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    // "S.A." becomes "s a"; a lone suffix (e.g. a company named "Co") stays.
    loop {
        let joined = words.join(" ");
        let hit = SUFFIXES.iter().find(|s| {
            let n = s.split(' ').count();
            words.len() > n && joined.ends_with(&format!(" {s}"))
        });
        match hit {
            Some(s) => words.truncate(words.len() - s.split(' ').count()),
            None => return joined,
        }
    }
}

/// The posting's host as a company domain, unless it is a job board.
pub fn domain_from_url(url: &str) -> Option<String> {
    let host = reqwest::Url::parse(url.trim()).ok()?.host_str()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    (!is_job_board(&host) && host.contains('.')).then_some(host)
}

/// Hosts that list many companies' postings, so they never identify one.
pub fn is_job_board(host: &str) -> bool {
    let host = host.trim().trim_start_matches("www.").to_lowercase();
    JOB_BOARDS.iter().any(|b| host == *b || host.ends_with(&format!(".{b}")))
}
