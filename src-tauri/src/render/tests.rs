use super::*;
use crate::store::types::Profile;

fn profile() -> Profile {
    Profile {
        name: "Ada Lovelace".into(),
        headline: "Analyst".into(),
        email: "ada@example.com".into(),
        phone: "+1 555 0100".into(),
        location: "London".into(),
        links: vec!["github.com/ada".into()],
        summary: String::new(),
    }
}

fn doc(bullets: Vec<String>, style: u8) -> CvDoc {
    CvDoc {
        profile: profile(),
        sections: vec![CvSection {
            heading: SectionHeading::Experience,
            items: vec![CvItem {
                title: "Engineer".into(),
                org: "Analytical Engines Ltd".into(),
                location: "London".into(),
                dates: "Jan 2020 – Present".into(),
                bullets,
            }],
        }],
        skills: vec!["Rust".into(), "Typst".into()],
        style: StyleStep(style),
    }
}

fn n_bullets(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("Bullet number {i} describing a measurable outcome with enough words to wrap across two full lines of text on a US letter page, so that thirty of them overflow one page"))
        .collect()
}

fn text_of(pdf: &[u8]) -> String {
    pdf_extract::extract_text_from_mem(pdf).expect("extract text")
}

/// Extraction may break lines/spaces differently; compare ignoring whitespace.
fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn one_page_minimal() {
    let out = compile(&doc(vec!["Built a thing".into(), "Shipped it".into()], 0)).unwrap();
    assert_eq!(out.pages, 1);
    assert!(out.pdf.starts_with(b"%PDF"));
}

#[test]
fn ats_text_extractable() {
    let bullets = vec!["Reduced build time by half".to_string(), "Mentored four engineers".to_string()];
    let out = compile(&doc(bullets.clone(), 0)).unwrap();
    let text = squash(&text_of(&out.pdf));
    for b in &bullets {
        assert!(text.contains(&squash(b)), "missing bullet {b:?} in {text:?}");
    }
    assert!(text.contains("Experience"));
    assert!(text.contains("Skills"));
}

#[test]
fn special_chars_literal() {
    let tricky = "Cut costs 40% #1 *fast* $5k @acme <tag> _x_";
    let out = compile(&doc(vec![tricky.into()], 0)).unwrap();
    let text = squash(&text_of(&out.pdf));
    assert!(text.contains(&squash(tricky)), "got {text:?}");
}

#[test]
fn fill_reported() {
    let long = compile(&doc(n_bullets(30), 0)).unwrap();
    assert!(long.pages >= 2, "pages = {}", long.pages);
    let short = compile(&doc(n_bullets(2), 0)).unwrap();
    assert_eq!(short.pages, 1);
    assert!(short.fill > 0.0 && short.fill < 0.5, "fill = {}", short.fill);
}

#[test]
fn style_steps_shrink() {
    let loose = compile(&doc(n_bullets(30), 0)).unwrap();
    let tight = compile(&doc(n_bullets(30), 2)).unwrap();
    assert!(tight.pages <= loose.pages);
}
