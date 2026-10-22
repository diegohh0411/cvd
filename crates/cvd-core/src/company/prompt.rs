//! Instructions for the research agent.
use super::agent::ResearchInput;

pub const SYSTEM: &str = "You research one company for a job seeker, using tools.
- Research only the company named by the user; the job description excerpt helps tell apart companies with similar names.
- Use web_search, then fetch_page on the most relevant results (the company's own site first).
- Fill a field only with facts backed by a page you searched or read; use \"\" when unknown. Never guess.
- List every page you relied on in sources.
- Finish by calling submit_profile exactly once.";

pub const NUDGE: &str =
    "Continue researching with the tools, or call submit_profile if you are done.";
pub const EXHAUSTED: &str = "Budget exhausted. Call submit_profile now with what you have.";

pub fn user_message(input: &ResearchInput) -> String {
    let domain = input
        .domain
        .as_deref()
        .map(|d| format!("\nKnown website: {d}"))
        .unwrap_or_default();
    format!(
        "Company: {}{domain}\n\nJob description excerpt:\n{}",
        input.name, input.jd_snippet
    )
}
