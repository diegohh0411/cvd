use super::fit::*;
use super::*;
use crate::matcher::{Ranked, RankedEntry};
use crate::store::types::{Bullet, Entry, EntryKind, Skill};

fn profile() -> Profile {
    Profile {
        name: "Ada Lovelace".into(),
        headline: "Analyst".into(),
        email: "ada@example.com".into(),
        phone: "+1 555 0100".into(),
        location: "London".into(),
        links: vec![],
        summary: String::new(),
    }
}

const LINE: &str = "Delivered a measurable outcome for the team with enough words to wrap across two full lines of text on a US letter page in the template";

/// Entry `id` with bullets `id*100 + i`; bullet scores descend from `score`.
fn entry(id: i64, score: f64, start: &str, n_bullets: usize) -> RankedEntry {
    let bullets: Vec<(Bullet, f64)> = (0..n_bullets)
        .map(|i| {
            (
                Bullet {
                    id: id * 100 + i as i64,
                    entry_id: id,
                    text: format!("{LINE} ({id}.{i})"),
                    position: i as i64,
                },
                score - 0.01 * i as f64,
            )
        })
        .collect();
    RankedEntry {
        entry: Entry {
            id,
            kind: EntryKind::Experience,
            title: format!("Engineer {id}"),
            org: "Org".into(),
            location: "London".into(),
            start_date: start.into(),
            end_date: None,
            description: String::new(),
            sort_hint: 0,
            archived: false,
            bullets: bullets.iter().map(|b| b.0.clone()).collect(),
            skills: vec![],
        },
        score,
        bullets,
    }
}

fn ranked(mut entries: Vec<RankedEntry>) -> Ranked {
    entries.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ranked {
        entries,
        skills: vec![(Skill { id: 1, name: "Rust".into(), category: String::new() }, 0.9)],
        seniority: String::new(),
    }
}

fn all_bullets(sel: &Selection) -> Vec<i64> {
    sel.entries.iter().flat_map(|e| e.bullet_ids.clone()).collect()
}

#[test]
fn never_exceeds_budget() {
    let r = ranked((0..25).map(|i| entry(i + 1, 4.0 - i as f64 * 0.1, &format!("20{:02}-01", i), 5)).collect());
    let out = fit(&r, &profile(), 1, 0.0, &[]).unwrap();
    assert_eq!(out.compiled.pages, 1);
    assert!(!out.overflow);
}

#[test]
fn drops_bullets_before_entries() {
    // 3 entries x 15 bullets: all entries fit, not all bullets do.
    let r = ranked(vec![
        entry(1, 3.9, "2022-01", 15),
        entry(2, 3.5, "2021-01", 15),
        entry(3, 3.0, "2020-01", 15),
    ]);
    let out = fit(&r, &profile(), 1, 0.0, &[]).unwrap();
    assert_eq!(out.selection.entries.len(), 3);
    let kept = all_bullets(&out.selection);
    assert!(kept.len() < 45, "case must overflow");
    let score = |id: i64| {
        r.entries.iter().flat_map(|e| e.bullets.iter()).find(|b| b.0.id == id).unwrap().1
    };
    let dropped: Vec<i64> = r
        .entries
        .iter()
        .flat_map(|e| e.bullets.iter().map(|b| b.0.id))
        .filter(|b| !kept.contains(b))
        .collect();
    // Only extra bullets (beyond each entry's top two) are dropped, and they
    // are the lowest-scoring extras.
    assert!(dropped.iter().all(|d| d % 100 >= 2), "dropped {dropped:?}");
    let max_dropped = dropped.iter().map(|&d| score(d)).fold(f64::MIN, f64::max);
    let min_kept_extra =
        kept.iter().filter(|&&b| b % 100 >= 2).map(|&b| score(b)).fold(f64::MAX, f64::min);
    assert!(max_dropped <= min_kept_extra, "{max_dropped} > {min_kept_extra}");
    assert!(out.compiled.pages == 1);
}

#[test]
fn min_one_bullet() {
    let r = ranked((0..12).map(|i| entry(i + 1, 4.0 - i as f64 * 0.2, "2020-01", 3)).collect());
    let out = fit(&r, &profile(), 1, 0.0, &[]).unwrap();
    assert!(out.selection.entries.iter().all(|e| !e.bullet_ids.is_empty()));
}

#[test]
fn floor_respected() {
    let r = ranked(vec![entry(1, 3.5, "2022-01", 1), entry(2, 1.0, "2021-01", 1)]);
    let out = fit(&r, &profile(), 1, 2.0, &[]).unwrap();
    let ids: Vec<i64> = out.selection.entries.iter().map(|e| e.entry_id).collect();
    assert_eq!(ids, vec![1]);
}

#[test]
fn date_order_within_section() {
    let r = ranked(vec![
        entry(1, 2.0, "2023-01", 1),
        entry(2, 3.9, "2018-01", 1),
        entry(3, 3.0, "2021-06", 1),
    ]);
    let out = fit(&r, &profile(), 1, 0.0, &[]).unwrap();
    let ids: Vec<i64> = out.selection.entries.iter().map(|e| e.entry_id).collect();
    assert_eq!(ids, vec![1, 3, 2]);
}

#[test]
fn tighten_before_drop() {
    // Fake compile: one page at step 0 holds 3 bullets, step 1 holds 4.
    let r = ranked(vec![
        entry(1, 3.9, "2022-01", 1),
        entry(2, 3.5, "2021-01", 1),
        entry(3, 3.0, "2020-01", 1),
        entry(4, 2.5, "2019-01", 1),
    ]);
    let mut compile_fn = |doc: &CvDoc| {
        let n: usize = doc.sections.iter().flat_map(|s| &s.items).map(|i| i.bullets.len()).sum();
        let cap = 3 + doc.style.0 as usize;
        Ok(Compiled { pages: if n <= cap { 1 } else { 2 }, fill: 0.5, pdf: vec![] })
    };
    let (out, _) = fit_with(&r, &profile(), 1, 0.0, &[], &mut compile_fn).unwrap();
    assert_eq!(out.selection.entries.len(), 4);
    assert_eq!(out.style, StyleStep(1));
}

#[test]
fn oversized_terminates() {
    let mut e = entry(1, 3.9, "2022-01", 1);
    e.bullets[0].0.text = "word ".repeat(8000);
    e.entry.bullets[0].text = e.bullets[0].0.text.clone();
    let r = ranked(vec![e]);
    let mut real = |doc: &CvDoc| compile(doc);
    let (out, compiles) = fit_with(&r, &profile(), 1, 0.0, &[], &mut real).unwrap();
    assert!(out.overflow);
    // No candidates beyond the base: at most one compile per style step.
    assert!(compiles <= 3, "compiles = {compiles}");
}

#[test]
fn override_exclude_wins() {
    let r = ranked(vec![entry(1, 3.9, "2022-01", 1), entry(2, 3.5, "2021-01", 1)]);
    let mut compile_fn = |doc: &CvDoc| {
        let n: usize = doc.sections.iter().map(|s| s.items.len()).sum();
        Ok(Compiled { pages: if n <= 1 { 1 } else { 2 }, fill: 0.5, pdf: vec![] })
    };
    let ov = [Override::Include(Target::Entry(1)), Override::Exclude(Target::Entry(1))];
    let (out, _) = fit_with(&r, &profile(), 1, 0.0, &ov, &mut compile_fn).unwrap();
    let ids: Vec<i64> = out.selection.entries.iter().map(|e| e.entry_id).collect();
    assert_eq!(ids, vec![2]);
}
