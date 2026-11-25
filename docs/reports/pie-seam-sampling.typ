#let teal = rgb("#15635B")
#let ink = rgb("#20313A")
#let muted = rgb("#52646B")
#let pale = rgb("#EAF4F1")
#let rule = rgb("#C8D9D5")

#set document(
  title: "Matched seam sampling in ClothCraft",
  description: "Addendum to the physics-inspired mesh resolution report",
)
#set page(
  paper: "a4",
  margin: (x: 20mm, top: 24mm, bottom: 21mm),
  header: [
    #set text(size: 8pt, fill: muted)
    #text(fill: teal, weight: "bold")[CLOTHCRAFT]
    #h(1fr)
    PHYSICS-INSPIRED MESH RESOLUTION
    #v(2mm)
    #line(length: 100%, stroke: 0.55pt + rule)
  ],
  footer: [
    #line(length: 100%, stroke: 0.55pt + rule)
    #v(1.5mm)
    #text(size: 8pt, fill: muted)[Meshing report addendum · 25 November 2026]
  ],
)
#set text(font: ("Noto Sans", "DejaVu Sans"), size: 10.2pt, fill: ink, lang: "en")
#set par(leading: 0.65em, spacing: 0.9em)
#show heading: set text(fill: teal, size: 12pt)
#show heading: set block(above: 1.2em, below: 0.6em)
#show link: set text(fill: teal)

#text(font: "Libertinus Serif", size: 33pt, weight: "bold")[Matched seam sampling]
#v(1mm)
#text(size: 11pt, fill: muted)[An extension to ClothCraft’s static physics-inspired remesher]
#v(3mm)

In physics-inspired preparation, an eligible sewn boundary now gets a vertex
at every station on its mating boundary.
The panels keep separate vertices and share a coordinate only along the
seam correspondence. A gathered edge can therefore use the same stations as a
shorter waistband while retaining its longer rest length in material space.

This extends the static sizing and constrained triangulation introduced in
upstream PR 230 and exposed through ClothCraft’s mesh preparation controls
in PR 8. #link("https://github.com/erythrofr/ClothCraft/pull/230")[Upstream PR 230]
· #link("https://github.com/NicTanghe/ClothCraft/pull/8")[ClothCraft PR 8]
· #link("https://doi.org/10.1145/3721238.3730619")[Zhang et al., SIGGRAPH 2025]

= How the samples match

The first pass measures the vertices selected by each panel’s sizing field.
The compiler maps both sets into one seam coordinate using normalized rest
arclength and the stitch integrator’s reversal and notch rules. It adds the
trim endpoints, chain joints and notch stations. The final pass inserts
the missing samples into the authored contour segments and refines the
panels separately.

#table(
  columns: (2fr, 1fr, 1fr, 1fr, 1fr, 1fr),
  align: (left, center, center, center, center, center),
  inset: (x: 6pt, y: 5pt),
  stroke: 0.4pt + rule,
  fill: (_, row) => if row == 0 { pale },
  table.header([Example eased seam], [Start], [Sample], [Notch], [Sample], [End]),
  [Side A], [0%], [25%], [50%], [75%], [100%],
  [Side B], [0%], [37.5%], [75%], [87.5%], [100%],
)

Here the notch pairs 50% of side A with 75% of side B. The stations to either
side follow the piecewise linear mapping between notch and endpoint.

= Scope and quality checks

A seam participates when all its segments are on polygon panels and at least
one panel has been prepared with seam matching. An ordinary polygon can receive samples from a
prepared mate. Other seams continue to use interpolated coupling. The
final triangulation preserves boundary samples and checks its area and vertex
budgets. Protected constraints can prevent the angle target from being
reached locally, especially when stations are close. Matching can add
vertices and rebuild work. Older plans retain their mesh topology and saved
drape. Reapplying physics-inspired preparation enables the new sampling.

= Verification

45 focused CPU tests passed. Regression tests check actual paired vertices for a trimmed, reversed and
eased seam between prepared and ordinary polygons. They also check a
chain joint, independent panel indices, reproducible rebuilds and sample
positions on unsewn boundaries. Existing area, mass, seam measure and
CPU gathered-sewing checks cover the resulting meshes. These checks
establish correspondence and mesh validity. Wrinkle quality and solver
speedup are open measurements.
