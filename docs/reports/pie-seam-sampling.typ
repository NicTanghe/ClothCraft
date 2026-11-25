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

The compiler maps authored contour vertices, trim endpoints, chain joints
and notch correspondences into a shared seam coordinate. Between these
anchors it subdivides once, using the longer mapped rest-arclength interval
and the finest requested spacing. The final pass replaces optional first-pass
samples in those spans and protects the resulting constraints.

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

= Spacing controls

Click any mesh setting value to open its number editor. *Sewn edge / base
spacing* ranges from 0.1 to 1.0 and defaults to 1.0. It sets the shared station
density and the local sizing band. *Boundary / base spacing* applies to free
spans, including the unsewn parts of partially sewn edges. Nearby features
can still refine the area. Select *Prepare physics mesh and reset* to remesh.

A seam can share samples when all its segments belong to polygons and at
least one panel uses matching. An ordinary polygon can receive samples from
its prepared mate. The mesher still checks area and vertex limits. Protected
constraints may locally limit triangle angles. Older plans retain their
topology and drape until preparation is reapplied.

= Verification

The CPU regressions verify actual paired vertices on curved, trimmed,
reversed and eased seams, including mates with different preparation
settings. On the straight-side fixture, changing the finer panel from 0.25
to 1.0 more than halves the seam samples on both sides and lowers nearby
vertex density. The saved USDA rebuilds the same mesh. Wrinkle quality and
solver speedup remain unmeasured.
