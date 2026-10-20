use serde::{Deserialize, Serialize};

/// Most fields accepted from one page message; the rest are dropped.
pub const MAX_FIELDS: usize = 300;
const MAX_TEXT: usize = 500;
const MAX_OPTIONS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Email,
    Tel,
    Url,
    Number,
    Date,
    Textarea,
    Select,
    Radio,
    Checkbox,
    Combobox,
    File,
    #[serde(other)]
    Other,
}

/// One form control as the injected script describes it. Comes from a remote
/// page, so every string is capped by [`FieldSpec::sanitized`] before use.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct FieldSpec {
    /// `data-cvd-key` the script stamped on the element (or radio group).
    pub key: String,
    pub kind: Option<FieldKind>,
    pub label: String,
    pub placeholder: String,
    pub autocomplete: String,
    pub name_attr: String,
    pub help: String,
    pub section: String,
    pub required: bool,
    pub options: Vec<String>,
}

impl FieldSpec {
    pub fn kind(&self) -> FieldKind {
        self.kind.unwrap_or(FieldKind::Other)
    }

    pub fn sanitized(mut self) -> Self {
        let cap = |s: &mut String| {
            if s.len() > MAX_TEXT {
                let mut end = MAX_TEXT;
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s.truncate(end);
            }
        };
        for s in [
            &mut self.key,
            &mut self.label,
            &mut self.placeholder,
            &mut self.autocomplete,
            &mut self.name_attr,
            &mut self.help,
            &mut self.section,
        ] {
            cap(s);
        }
        self.options.truncate(MAX_OPTIONS);
        self.options.iter_mut().for_each(cap);
        self
    }
}

/// What the page should do with one field. Sent back to the injected script.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FillAction {
    /// Type `value` into a text-like field, or pick the option with this exact text.
    Set {
        key: String,
        value: String,
        fact: &'static str,
    },
    /// Attach the job's CV PDF.
    File {
        key: String,
        name: String,
        b64: String,
    },
    NeedsUser {
        key: String,
        reason: String,
    },
}

impl FillAction {
    pub fn key(&self) -> &str {
        match self {
            Self::Set { key, .. } | Self::File { key, .. } | Self::NeedsUser { key, .. } => key,
        }
    }
}
