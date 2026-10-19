//! Greedy page-budget fit (spec §5): base, then entry candidates, then single
//! bullets, compiling each addition and tightening the style before dropping.

use serde::{Deserialize, Serialize};

use super::{Compiled, CvDoc, CvItem, CvSection, RenderError, SectionHeading, StyleStep};
use crate::matcher::Ranked;
use crate::store::types::{Entry, EntryKind, Profile};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedEntry {
    pub entry_id: i64,
    /// In stored position order.
    pub bullet_ids: Vec<i64>,
    pub pinned: bool,
}

/// Entries in render order (section, then date or explicit order).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    pub entries: Vec<SelectedEntry>,
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Entry(i64),
    Bullet(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Override {
    Include(Target),
    Exclude(Target),
    Order(Vec<i64>),
    /// Bullet ids in the order they should render within their entry; bullets
    /// not listed follow in score order.
    BulletOrder(Vec<i64>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitResult {
    pub selection: Selection,
    pub style: StyleStep,
    pub compiled: Compiled,
    pub overflow: bool,
}

const MAX_STEP: u8 = 2;
/// Below this last-page fill, below-floor entries are added as filler.
const MIN_FILL: f32 = 0.75;
const SKILL_MIN_PROB: f64 = 0.5;

/// Selects the entries and bullets that fit in `budget` pages. See spec §5.
pub fn fit(
    ranked: &Ranked,
    profile: &Profile,
    budget: u8,
    floor: f64,
    overrides: &[Override],
) -> Result<FitResult, RenderError> {
    fit_with(
        ranked,
        profile,
        budget,
        floor,
        overrides,
        &mut super::compile,
    )
    .map(|r| r.0)
}

/// Builds the document for a saved `selection` (render order kept; entries
/// missing from `entries` are skipped). Used to export exactly what was chosen.
pub fn selection_doc(
    entries: &[Entry],
    profile: &Profile,
    selection: &Selection,
    style: StyleStep,
) -> CvDoc {
    let mut sections: Vec<CvSection> = Vec::new();
    for heading in [
        SectionHeading::Experience,
        SectionHeading::Projects,
        SectionHeading::Education,
    ] {
        let items: Vec<CvItem> = selection
            .entries
            .iter()
            .filter_map(|s| entries.iter().find(|e| e.id == s.entry_id).map(|e| (s, e)))
            .filter(|(_, e)| section_of(e.kind) == heading)
            .map(|(s, e)| CvItem {
                title: e.title.clone(),
                org: e.org.clone(),
                location: e.location.clone(),
                dates: dates(e),
                bullets: s
                    .bullet_ids
                    .iter()
                    .filter_map(|id| e.bullets.iter().find(|b| b.id == *id))
                    .map(|b| b.text.clone())
                    .collect(),
            })
            .collect();
        if !items.is_empty() {
            sections.push(CvSection { heading, items });
        }
    }
    CvDoc {
        profile: profile.clone(),
        sections,
        skills: selection.skills.clone(),
        style,
    }
}

/// One entry in the working selection (`idx` into `Ranked.entries`).
#[derive(Clone)]
struct Pick {
    idx: usize,
    bullets: Vec<i64>,
    pinned: bool,
}

type CompileFn<'a> = dyn FnMut(&CvDoc) -> Result<Compiled, RenderError> + 'a;

struct Fitter<'a, 'c> {
    ranked: &'a Ranked,
    profile: &'a Profile,
    budget: usize,
    skills: Vec<String>,
    order: Vec<i64>,
    bullet_order: Vec<i64>,
    compile: &'c mut CompileFn<'c>,
    compiles: usize,
    style: StyleStep,
}

/// `fit` with an injectable compiler; also returns the number of compiles.
pub(crate) fn fit_with<'c>(
    ranked: &Ranked,
    profile: &Profile,
    budget: u8,
    floor: f64,
    overrides: &[Override],
    compile: &'c mut CompileFn<'c>,
) -> Result<(FitResult, usize), RenderError> {
    let excluded = |t: Target| overrides.contains(&Override::Exclude(t));
    let included = |t: Target| overrides.contains(&Override::Include(t));
    let order = overrides
        .iter()
        .rev()
        .find_map(|o| match o {
            Override::Order(ids) => Some(ids.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let bullet_order = overrides
        .iter()
        .rev()
        .find_map(|o| match o {
            Override::BulletOrder(ids) => Some(ids.clone()),
            _ => None,
        })
        .unwrap_or_default();

    // Non-excluded bullets per entry, in score order; empty = entry is out.
    let eligible: Vec<Vec<(i64, f64)>> = ranked
        .entries
        .iter()
        .map(|re| {
            if excluded(Target::Entry(re.entry.id)) {
                return vec![];
            }
            re.bullets
                .iter()
                .filter(|(b, _)| !excluded(Target::Bullet(b.id)))
                .map(|(b, s)| (b.id, *s))
                .collect()
        })
        .collect();

    // Pinned picks from Include overrides.
    let mut picks: Vec<Pick> = Vec::new();
    for (idx, re) in ranked.entries.iter().enumerate() {
        let bullets = &eligible[idx];
        if bullets.is_empty() {
            continue;
        }
        let mut chosen: Vec<i64> = bullets
            .iter()
            .map(|b| b.0)
            .filter(|&b| included(Target::Bullet(b)))
            .collect();
        if chosen.is_empty() && included(Target::Entry(re.entry.id)) {
            chosen.push(bullets[0].0);
        }
        if !chosen.is_empty() {
            picks.push(Pick {
                idx,
                bullets: chosen,
                pinned: true,
            });
        }
    }
    // Base: the top-scoring eligible entry with its top bullet.
    let top = (0..ranked.entries.len())
        .find(|&i| !eligible[i].is_empty() && ranked.entries[i].score >= floor);
    if let Some(idx) = top {
        let first = eligible[idx][0].0;
        match picks.iter_mut().find(|p| p.idx == idx) {
            Some(p) if !p.bullets.contains(&first) => p.bullets.push(first),
            Some(_) => {}
            None => picks.push(Pick {
                idx,
                bullets: vec![first],
                pinned: false,
            }),
        }
    }

    let skills = ranked
        .skills
        .iter()
        .filter(|(_, p)| *p >= SKILL_MIN_PROB)
        .map(|(s, _)| s.name.clone())
        .collect();
    let mut f = Fitter {
        ranked,
        profile,
        budget: budget.max(1) as usize,
        skills,
        order,
        bullet_order,
        compile,
        compiles: 0,
        style: StyleStep(0),
    };

    let mut compiled = match f.try_fit(&picks)? {
        Ok(c) => c,
        Err(c) => {
            // Even the base overflows at the tightest step.
            f.style = StyleStep(MAX_STEP);
            let result = FitResult {
                selection: f.selection(&picks),
                style: f.style,
                compiled: c,
                overflow: true,
            };
            return Ok((result, f.compiles));
        }
    };

    // Entry candidates by score: top 2 bullets, falling back to the top 1.
    for (idx, bullets) in eligible.iter().enumerate() {
        if bullets.is_empty()
            || ranked.entries[idx].score < floor
            || picks.iter().any(|p| p.idx == idx)
        {
            continue;
        }
        let top: Vec<i64> = bullets.iter().take(2).map(|b| b.0).collect();
        for n in (1..=top.len()).rev() {
            let mut next = picks.clone();
            next.push(Pick {
                idx,
                bullets: top[..n].to_vec(),
                pinned: false,
            });
            if let Ok(c) = f.try_fit(&next)? {
                picks = next;
                compiled = c;
                break;
            }
        }
    }

    // Single-bullet candidates for selected entries, by score.
    let mut singles: Vec<(f64, usize, i64)> = picks
        .iter()
        .flat_map(|p| {
            eligible[p.idx]
                .iter()
                .filter(|(b, s)| *s >= floor && !p.bullets.contains(b))
                .map(move |(b, s)| (*s, p.idx, *b))
        })
        .collect();
    singles.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, idx, bullet) in singles {
        let mut next = picks.clone();
        if let Some(p) = next.iter_mut().find(|p| p.idx == idx) {
            p.bullets.push(bullet);
        }
        if let Ok(c) = f.try_fit(&next)? {
            picks = next;
            compiled = c;
        }
    }

    // Filler: if the last page is under MIN_FILL, add below-floor entries
    // (volunteering first, then by score) at the current style, never adding a
    // page. Each tries its top 2 bullets, then its top 1.
    if compiled.fill < MIN_FILL {
        let pages = compiled.pages;
        let mut fillers: Vec<usize> = (0..eligible.len())
            .filter(|&i| !eligible[i].is_empty() && !picks.iter().any(|p| p.idx == i))
            .collect();
        // `ranked.entries` is already score-desc, so a stable sort keeps that order.
        fillers.sort_by_key(|&i| ranked.entries[i].entry.kind != EntryKind::Volunteer);
        for idx in fillers {
            if compiled.fill >= MIN_FILL {
                break;
            }
            let top: Vec<i64> = eligible[idx].iter().take(2).map(|b| b.0).collect();
            for n in (1..=top.len()).rev() {
                let mut next = picks.clone();
                next.push(Pick {
                    idx,
                    bullets: top[..n].to_vec(),
                    pinned: false,
                });
                let c = f.compile_at(&next)?;
                if c.pages <= pages {
                    picks = next;
                    compiled = c;
                    break;
                }
            }
        }
    }

    let result = FitResult {
        selection: f.selection(&picks),
        style: f.style,
        compiled,
        overflow: false,
    };
    Ok((result, f.compiles))
}

impl Fitter<'_, '_> {
    /// Compiles `picks` at the current step, then tighter ones. On fit the
    /// document moves to that step and `Ok(compiled)` is returned; otherwise
    /// `Err(last compile)`.
    fn try_fit(&mut self, picks: &[Pick]) -> Result<Result<Compiled, Compiled>, RenderError> {
        let mut last = None;
        for step in self.style.0..=MAX_STEP {
            let doc = self.doc(picks, StyleStep(step));
            let c = (self.compile)(&doc)?;
            self.compiles += 1;
            if c.pages <= self.budget {
                self.style = StyleStep(step);
                return Ok(Ok(c));
            }
            last = Some(c);
        }
        Ok(Err(last.expect("at least one step is tried")))
    }

    /// Compiles `picks` at the current step without tightening.
    fn compile_at(&mut self, picks: &[Pick]) -> Result<Compiled, RenderError> {
        let doc = self.doc(picks, self.style);
        self.compiles += 1;
        (self.compile)(&doc)
    }

    fn entry(&self, p: &Pick) -> &Entry {
        &self.ranked.entries[p.idx].entry
    }

    /// Picks grouped by section; entries in explicit, then library (`sort_hint`),
    /// then date-desc order; bullets in explicit, then score-desc (then stored position) order.
    fn sorted(&self, picks: &[Pick]) -> Vec<(SectionHeading, Vec<Pick>)> {
        let pos = |e: &Entry| {
            self.order
                .iter()
                .position(|&id| id == e.id)
                .unwrap_or(usize::MAX)
        };
        let mut out = Vec::new();
        for heading in [
            SectionHeading::Experience,
            SectionHeading::Projects,
            SectionHeading::Education,
        ] {
            let mut items: Vec<Pick> = picks
                .iter()
                .filter(|p| section_of(self.entry(p).kind) == heading)
                .cloned()
                .collect();
            if items.is_empty() {
                continue;
            }
            items.sort_by(|a, b| {
                let (ea, eb) = (self.entry(a), self.entry(b));
                // A zero `sort_hint` means "no library order": those follow the hinted ones.
                let hint = |e: &Entry| (e.sort_hint <= 0, e.sort_hint);
                pos(ea)
                    .cmp(&pos(eb))
                    .then_with(|| hint(ea).cmp(&hint(eb)))
                    .then_with(|| eb.start_date.cmp(&ea.start_date))
            });
            for p in &mut items {
                let entry = &self.ranked.entries[p.idx].entry;
                let bpos = |id: i64| {
                    self.bullet_order
                        .iter()
                        .position(|&b| b == id)
                        .unwrap_or(usize::MAX)
                };
                let ranked_bullets = &self.ranked.entries[p.idx].bullets;
                let key = |id: i64| {
                    let score = ranked_bullets
                        .iter()
                        .find(|(b, _)| b.id == id)
                        .map_or(0.0, |b| b.1);
                    let position = entry
                        .bullets
                        .iter()
                        .find(|b| b.id == id)
                        .map(|b| b.position);
                    (bpos(id), std::cmp::Reverse(ordered_float(score)), position)
                };
                p.bullets.sort_by_key(|id| key(*id));
            }
            out.push((heading, items));
        }
        out
    }

    fn doc(&self, picks: &[Pick], style: StyleStep) -> CvDoc {
        let sections = self
            .sorted(picks)
            .into_iter()
            .map(|(heading, items)| CvSection {
                heading,
                items: items
                    .iter()
                    .map(|p| {
                        let e = self.entry(p);
                        CvItem {
                            title: e.title.clone(),
                            org: e.org.clone(),
                            location: e.location.clone(),
                            dates: dates(e),
                            bullets: p
                                .bullets
                                .iter()
                                .filter_map(|id| e.bullets.iter().find(|b| b.id == *id))
                                .map(|b| b.text.clone())
                                .collect(),
                        }
                    })
                    .collect(),
            })
            .collect();
        CvDoc {
            profile: self.profile.clone(),
            sections,
            skills: self.skills.clone(),
            style,
        }
    }

    fn selection(&self, picks: &[Pick]) -> Selection {
        let entries = self
            .sorted(picks)
            .into_iter()
            .flat_map(|(_, items)| items)
            .map(|p| SelectedEntry {
                entry_id: self.entry(&p).id,
                bullet_ids: p.bullets,
                pinned: p.pinned,
            })
            .collect();
        Selection {
            entries,
            skills: self.skills.clone(),
        }
    }
}

/// Total-order key for a score (scores are finite, non-NaN).
fn ordered_float(x: f64) -> i64 {
    (x * 1e6).round() as i64
}

fn section_of(kind: EntryKind) -> SectionHeading {
    match kind {
        EntryKind::Experience | EntryKind::Volunteer => SectionHeading::Experience,
        EntryKind::Project => SectionHeading::Projects,
        EntryKind::Education => SectionHeading::Education,
    }
}

fn dates(e: &Entry) -> String {
    let end = e.end_date.as_deref().map_or("Present".into(), month_year);
    format!("{} – {}", month_year(&e.start_date), end)
}

/// `2026-08` (or `2026-08-15`) → `Aug 2026`; anything else passes through.
pub(crate) fn month_year(d: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = d.splitn(3, '-');
    match (
        parts.next(),
        parts.next().and_then(|m| m.parse::<usize>().ok()),
    ) {
        (Some(y), Some(m @ 1..=12)) if y.len() == 4 => format!("{} {y}", MONTHS[m - 1]),
        _ => d.to_string(),
    }
}
