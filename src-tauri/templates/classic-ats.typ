// Classic ATS: single column, plain headings, list bullets, no tables/grids.
// All data arrives as one JSON string in `sys.inputs.data`. Every value is
// inserted as a string (never evaluated as markup), so it renders literally.
#let d = json(bytes(sys.inputs.data))
#let st = d.style
#let p = d.profile

#set document(title: p.name)
#set page(paper: "us-letter", margin: (x: st.margin_x * 1pt, y: st.margin_y * 1pt))
#set text(font: "Source Sans 3", size: st.font_size * 1pt, lang: "en")
#set par(leading: st.leading * 1em, spacing: st.gap * 1pt)
#set list(marker: [•], indent: 0.4em, spacing: st.leading * 1em)
#show heading: it => block(above: st.section_gap * 1pt, below: st.gap * 1pt, {
  set text(size: st.font_size * 1.15pt, weight: "bold")
  it.body
})

#align(center, {
  text(size: st.font_size * 1.8pt, weight: "bold", p.name)
  if p.headline != "" { linebreak(); p.headline }
  let contact = (p.email, p.phone, p.location).filter(s => s != "") + p.links
  if contact.len() > 0 { linebreak(); contact.join(" | ") }
})
#if p.summary != "" { par(p.summary) }

#for section in d.sections {
  heading(level: 1, section.heading)
  for item in section.items {
    block(above: st.gap * 1.4pt, below: st.gap * 1pt, {
      text(weight: "bold", item.title)
      if item.dates != "" { h(1fr); item.dates }
      let sub = (item.org, item.location).filter(s => s != "")
      if sub.len() > 0 { linebreak(); text(style: "italic", sub.join(", ")) }
      if item.bullets.len() > 0 { list(..item.bullets) }
    })
  }
}

#if d.skills.len() > 0 {
  heading(level: 1, "Skills")
  par(d.skills.join(", "))
}
