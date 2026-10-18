use serde::{Deserialize, Serialize};

/// Settings row holding [`ApplyDefaults`] as JSON.
pub const APPLY_DEFAULTS_KEY: &str = "apply.defaults";

/// Answers that are not in the CV but come up on most application forms.
/// Every field is optional: empty or `None` means "leave it for the user".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplyDefaults {
    /// Overrides for the split of `Profile.name`.
    pub first_name: String,
    pub last_name: String,
    pub city: String,
    pub region: String,
    pub country: String,
    pub authorized_to_work: Option<bool>,
    pub needs_sponsorship: Option<bool>,
    pub willing_to_relocate: Option<bool>,
    pub notice_period: String,
    pub earliest_start: String,
    pub salary_expectation: String,
}

impl ApplyDefaults {
    /// A missing or unreadable row reads as empty defaults.
    pub fn from_setting(raw: Option<&str>) -> Self {
        raw.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()
    }

    pub fn trimmed(self) -> Self {
        let t = |s: String| s.trim().to_string();
        Self {
            first_name: t(self.first_name),
            last_name: t(self.last_name),
            city: t(self.city),
            region: t(self.region),
            country: t(self.country),
            notice_period: t(self.notice_period),
            earliest_start: t(self.earliest_start),
            salary_expectation: t(self.salary_expectation),
            ..self
        }
    }
}
