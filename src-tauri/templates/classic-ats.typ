// Classic ATS: single column, plain headings, list bullets, no tables/grids.
// All data arrives as one JSON string in `sys.inputs.data`. Every value is
// inserted as a string (never evaluated as markup), so it renders literally.
#let d = json(bytes(sys.inputs.data))
#let st = d.style
#let p = d.profile
#let ink = rgb("#1a1a1a")
#let muted = rgb("#4a4a4a")

#set document(title: p.name, author: p.name)
#set page(paper: "us-letter", margin: (x: st.margin_x * 1pt, y: st.margin_y * 1pt))
#set text(font: "Source Sans 3", size: st.font_size * 1pt, lang: "en", fill: ink)
#set par(leading: st.leading * 1em, spacing: st.gap * 1pt, justify: false)
#set list(marker: [•], indent: 0.3em, body-indent: 0.5em, spacing: st.leading * 1em)
#show link: set text(fill: rgb("#1f4e9c"))

// Section heading: bold, slightly larger, with a full-width rule underneath.
#show heading: it => block(above: st.section_gap * 1pt + 0.6em, below: st.gap * 1.2pt, sticky: true, {
  stack(spacing: 3pt,
    text(size: st.font_size * 1.2pt, weight: "bold", tracking: 0.04em, it.body),
    line(length: 100%, stroke: 0.6pt + ink))
})

// Contact entries become clickable when they look like an email or a URL.
#let contact-entry(s) = {
  if s.contains("@") and not s.contains("/") { link("mailto:" + s, s) }
  else if s.starts-with("http") { link(s, s.replace(regex("^https?://(www\.)?"), "")) }
  else if s.contains(".") and not s.contains(" ") { link("https://" + s, s) }
  else { s }
}

// Header
#align(center, {
  text(size: st.font_size * 2.1pt, weight: "bold", tracking: 0.02em, p.name)
  if p.headline != "" {
    linebreak()
    text(size: st.font_size * 1.1pt, fill: muted, p.headline)
  }
  let contact = (p.email, p.phone, p.location).filter(s => s != "") + p.links
  if contact.len() > 0 {
    linebreak()
    contact.map(contact-entry).join(text(fill: muted, "  |  "))
  }
})

#if p.summary != "" {
  heading(level: 1, "Profile")
  par(p.summary)
}

// Entry: line 1 = org (bold) + location; line 2 = role (italic) + dates.
// Falls back to a single line when there is no org.
#for section in d.sections {
  heading(level: 1, section.heading)
  for (i, item) in section.items.enumerate() {
    // Breathing gap between entries (not between heading and first entry).
    if i > 0 { v(0.6em) }
    block(above: st.gap * 1.8pt, below: st.gap * 1pt, breakable: true, {
      if item.org != "" {
        text(weight: "bold", item.org)
        if item.location != "" { h(1fr); text(weight: "bold", item.location) }
        linebreak()
        text(style: "italic", item.title)
        if item.dates != "" { h(1fr); text(style: "italic", item.dates) }
      } else {
        text(weight: "bold", item.title)
        if item.dates != "" { h(1fr); text(style: "italic", item.dates) }
        if item.location != "" { linebreak(); text(style: "italic", item.location) }
      }
      if item.bullets.len() > 0 {
        v(st.gap * 0.3pt)
        list(..item.bullets)
      }
    })
  }
}

#if d.skills.len() > 0 {
  heading(level: 1, "Skills")
  par(d.skills.join(", "))
}
