//! Typst renderer: Classic ATS template compiled in-process to PDF.

use std::sync::OnceLock;

use serde::Serialize;
use typst::foundations::{Dict, IntoValue};
use typst::layout::{Frame, FrameItem};
use typst_layout::PagedDocument;
use typst_as_lib::{TypstEngine, TypstTemplateMainFile};
use typst_pdf::PdfOptions;

use crate::store::types::Profile;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SectionHeading {
    Experience,
    Projects,
    Education,
    Skills,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StyleStep(pub u8);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CvItem {
    pub title: String,
    pub org: String,
    pub location: String,
    pub dates: String,
    pub bullets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CvSection {
    pub heading: SectionHeading,
    pub items: Vec<CvItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CvDoc {
    pub profile: Profile,
    pub sections: Vec<CvSection>,
    pub skills: Vec<String>,
    pub style: StyleStep,
}

#[derive(Debug, Clone)]
pub struct Compiled {
    pub pages: usize,
    pub fill: f32,
    pub pdf: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("typst compile failed: {0}")]
    Compile(String),
    #[error("pdf export failed: {0}")]
    Pdf(String),
}

const TEMPLATE: &str = include_str!("../../templates/classic-ats.typ");
static FONTS: [&[u8]; 3] = [
    include_bytes!("../../assets/fonts/SourceSans3-Regular.otf"),
    include_bytes!("../../assets/fonts/SourceSans3-Bold.otf"),
    include_bytes!("../../assets/fonts/SourceSans3-It.otf"),
];

/// Page margins in points (shared by the template and the fill metric).
const MARGIN_X: f64 = 40.0;
const MARGIN_Y: f64 = 36.0;

impl SectionHeading {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Experience => "Experience",
            Self::Projects => "Projects",
            Self::Education => "Education",
            Self::Skills => "Skills",
        }
    }
}

/// Typographic parameters for one tightening step, passed to the template.
#[derive(Serialize)]
struct StyleParams {
    font_size: f64,
    leading: f64,
    gap: f64,
    section_gap: f64,
    margin_x: f64,
    margin_y: f64,
}

impl StyleStep {
    fn params(self) -> StyleParams {
        let (font_size, leading, gap, section_gap) = match self.0 {
            0 => (10.5, 0.55, 5.0, 11.0),
            1 => (10.0, 0.55, 5.0, 11.0),
            _ => (10.0, 0.45, 3.0, 7.0),
        };
        StyleParams { font_size, leading, gap, section_gap, margin_x: MARGIN_X, margin_y: MARGIN_Y }
    }
}

#[derive(Serialize)]
struct TemplateSection<'a> {
    heading: &'static str,
    items: &'a [CvItem],
}

#[derive(Serialize)]
struct TemplateData<'a> {
    profile: &'a Profile,
    sections: Vec<TemplateSection<'a>>,
    skills: &'a [String],
    style: StyleParams,
}

fn engine() -> &'static TypstEngine<TypstTemplateMainFile> {
    static ENGINE: OnceLock<TypstEngine<TypstTemplateMainFile>> = OnceLock::new();
    ENGINE.get_or_init(|| TypstEngine::builder().main_file(TEMPLATE).fonts(FONTS).build())
}

pub fn compile(doc: &CvDoc) -> Result<Compiled, RenderError> {
    let data = TemplateData {
        profile: &doc.profile,
        sections: doc
            .sections
            .iter()
            .map(|s| TemplateSection { heading: s.heading.as_str(), items: &s.items })
            .collect(),
        skills: &doc.skills,
        style: doc.style.params(),
    };
    let json = serde_json::to_string(&data).map_err(|e| RenderError::Compile(e.to_string()))?;
    let mut inputs = Dict::new();
    inputs.insert("data".into(), json.into_value());

    let document: PagedDocument = engine()
        .compile_with_input(inputs)
        .output
        .map_err(|e| RenderError::Compile(format!("{e:?}")))?;
    let pdf = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| RenderError::Pdf(format!("{e:?}")))?;

    let last = document.pages().last().ok_or_else(|| RenderError::Compile("no pages".into()))?;
    let content_height = last.frame.height().to_pt() - 2.0 * MARGIN_Y;
    let used = max_y(&last.frame) - MARGIN_Y;
    let fill = (used / content_height).clamp(0.0, 1.0) as f32;

    Ok(Compiled { pages: document.pages().len(), fill, pdf })
}

/// Lowest y-extent (in pt, from the frame top) of any visible item.
fn max_y(frame: &Frame) -> f64 {
    frame
        .items()
        .map(|(pos, item)| {
            let y = pos.y.to_pt();
            match item {
                FrameItem::Group(g) => y + max_y(&g.frame),
                // Text position is the baseline; add the descent.
                FrameItem::Text(t) => y + t.size.to_pt() * 0.25,
                FrameItem::Shape(s, _) => y + s.bbox(true).max.y.to_pt(),
                FrameItem::Image(_, size, _) => y + size.y.to_pt(),
                _ => 0.0,
            }
        })
        .fold(0.0, f64::max)
}
