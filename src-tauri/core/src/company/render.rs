//! The company block System One sees as part of its `state`.
use crate::store::types::CompanyProfile;

const FIELD_CAP: usize = 300;
/// ~400 tokens at the matcher's 3-chars-per-token estimate.
const RENDER_CAP: usize = 1200;

fn cap(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match s.char_indices().nth(FIELD_CAP) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s,
    }
}

/// One line per non-empty field; sources are never included. Lines are
/// dropped from the bottom until the block fits the cap.
pub fn render(name: &str, domain: Option<&str>, p: &CompanyProfile) -> String {
    let mut lines = vec![match domain {
        Some(d) => format!("Name: {} ({d})", cap(name)),
        None => format!("Name: {}", cap(name)),
    }];
    let fields = [
        ("Summary", p.summary.clone()),
        ("Industry", p.industry.clone()),
        ("Size", p.size.clone()),
        ("Stage", p.stage.clone()),
        ("Headquarters", p.headquarters.clone()),
        ("Remote policy", p.remote_policy.clone()),
        ("Products", p.products.clone()),
        ("Tech stack", p.tech_stack.join(", ")),
    ];
    for (label, value) in fields {
        if !value.trim().is_empty() {
            lines.push(format!("{label}: {}", cap(&value)));
        }
    }
    while lines.len() > 1 && lines.join("\n").len() > RENDER_CAP {
        lines.pop();
    }
    lines.join("\n")
}
