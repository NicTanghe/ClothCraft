#let ink = rgb("#203039")
#let muted = rgb("#50636A")
#let teal = rgb("#14645E")
#let tint = rgb("#EAF4F1")
#let rule = rgb("#CADBD7")
#let base = "25c4cd70f5f6ceee40b5998320a457827e9054c8"
#let repo = "https://github.com/NicTanghe/ClothCraft"
#let source(path, location, label) = link(repo + "/blob/" + base + "/" + path + "#L" + str(location), label)

#set document(
  title: "ClothCraft USD transition audit",
  description: "Persistence, editor authority, viewport stages and solver data.",
)
#set page(
  paper: "a4",
  margin: (x: 20mm, top: 25mm, bottom: 22mm),
  header: [
    #set text(size: 8pt, fill: muted)
    #text(fill: teal, weight: "bold")[CLOTHCRAFT]
    #h(1fr)
    ARCHITECTURE AUDIT
    #v(2mm)
    #line(length: 100%, stroke: 0.55pt + rule)
  ],
  footer: context [
    #line(length: 100%, stroke: 0.55pt + rule)
    #v(1.5mm)
    #set text(size: 8pt, fill: muted)
    USD transition
    #h(1fr)
    #counter(page).display("1")
  ],
)
#set text(font: ("Noto Sans", "DejaVu Sans"), size: 10.1pt, fill: ink, lang: "en")
#set par(leading: 0.65em, spacing: 0.85em)
#set heading(numbering: none)
#show heading.where(level: 1): set text(font: "Libertinus Serif", size: 26pt)
#show heading.where(level: 2): set text(size: 12.5pt, fill: teal)
#show heading.where(level: 2): set block(above: 1.05em, below: 0.6em)
#show link: set text(fill: teal)
#show raw: set text(font: "DejaVu Sans Mono", size: 0.9em)

#let stage(title, description) = block(
  width: 100%,
  height: 23mm,
  fill: tint,
  inset: 7pt,
  radius: 3pt,
  [
    #text(size: 9pt, fill: teal, weight: "bold", title)
    #v(0.6mm)
    #text(size: 8.6pt, description)
  ],
)

#text(font: "Libertinus Serif", size: 34pt, weight: "bold")[USD in ClothCraft]
#v(1mm)
#text(size: 11pt, fill: muted)[Document storage, live viewport and internal edit authority]
#v(3mm)
#text(size: 8.7pt, fill: muted)[24 November 2026 · Review base] #link(repo + "/commit/" + base)[#raw("25c4cd7")]
#v(3mm)

ClothCraft uses USD for editable template files, optional fitting-project
files and the live 3D viewport. During editing, the authoritative garment is a
Rust #raw("GarmentTemplate") inside #raw("FittingProject"). That is the main
gap if USD is meant to own the internal authored scene. #source("src/fitting.rs", 20, [Fitting project])
· #source("src/atelier/css_runner/editing.rs", 107, [Edit and undo path])

== How the data moves

#grid(
  columns: (1fr, 16pt, 1fr, 16pt, 1fr),
  align: horizon,
  stage([Files], [USDA templates and fitting projects. Legacy JSON paths still decode.]),
  align(center, text(fill: teal)[⇄]),
  stage([Editor and solver], [Rust template and history compile into mesh, collider and solver state.]),
  align(center, text(fill: teal)[→]),
  stage([Live USD stage], [The viewport reads published geometry and camera data through cssimpler.]),
)

== Where USD is already in use

- *Documents.* Bundled templates are USDA. The template and fitting codecs map
  serde values to typed USD attributes and prims, including double-precision
  authored values. The reader uses those fields. The exported panel
  #raw("Fill") and #raw("Boundary") views are derived for interchange.
  #source("src/usd_document.rs", 73, [Document codec])
- *Avatar import.* The bounded USD importer opens a stage and extracts static
  mesh geometry with transforms, units and up-axis conversion. The editor
  retains the extracted surface and collider data for display and collisions.
  #source("src/usd_avatar.rs", 75, [USD avatar importer])
  · #source("src/avatar.rs", 155, [Mesh collider])
- *Viewport.* Both desktop paths share an event-thread USD stage with
  cssimpler. Accepted positions and normals are published into stage prims.
  The application still computes camera state and authors its pose to USD.
  #source("src/atelier/css_runner/state.rs", 384, [Fitting viewport sync])
  · #source("src/editor/css_editor.rs", 457, [Project v1 viewport sync])

== Current Rust authority

Commands, undo snapshots, material and seam references, compiled topology,
and accepted simulation state use Rust types. The worker receives a
#raw("FittingScene") and returns a #raw("State") tagged by generation.
Rest-space positions retain double precision. Runtime buffers and
acceleration structures can remain derived even if USD takes ownership of
authoring. #source("src/authoring.rs", 1541, [Command application])
· #source("src/atelier/worker.rs", 63, [Worker contract])
· #source("src/model.rs", 108, [Mesh and state types])

#pagebreak()

= Changes and remaining work

== Changes in this pull request

Both desktop viewports seed an anonymous OpenUSD layer directly from their
generated scene data. The live stage loads without writing and reopening
#raw("runner_preview.usda") or #raw("editor_preview.usda"). The published
geometry and the authored camera use the existing viewport paths.

A fitting session with no opened project saves to
#raw("<output>/fitting-project.usda"). Save continues to use the opened
path for existing documents. The #raw("cloth-fit") CLI continues to emit its
JSON checkpoint for benchmark and packaging scripts. Project v1 still
reads and writes its own JSON format.
#source("src/bin/cloth-fit.rs", 397, [CLI checkpoint])
· #source("src/project.rs", 100, [Project v1 storage])

== What still blocks stage-owned editing

*Define a lossless authored layer for the garment.* Choose explicit USD
properties for named boundaries, curves, holes, sewing, pins, grain, fabric and
placement. Keep semantic IDs stable across remeshing. Make the f64 rest data in
material metres the authoritative geometry and generate the mesh from it.
Issue 51 specifies a USD file and keeps undo on the in-memory template. A
stage-owned editor needs an additional transaction contract.
#link(repo + "/issues/51")[USD save-format issue]

*Put edits and undo behind the same transaction.* Prototype a vertical slice
with one curved panel and a named seam. Apply a control-point edit through
the authored layer, regenerate the mesh, and use the existing validation
before publishing. Exercise undo and redo, rejected edits, and save/reopen.
A failed edit should preserve the prior authored data, selection and accepted
worker generation.

*Separate authored data from session data.* Keep transient drape samples and
camera opinions in an explicitly scoped layer or cache. Define which accepted
state is saved for restart. Preserve portable assets and existing JSON imports
while the fitting script outputs migrate. The solver should continue to consume
compiled snapshots across the worker boundary.

== Verification

46 CPU regression checks passed for USDA round trips, snapshot publication,
legacy document I/O and avatar imports. 50 desktop-feature checks passed,
including headless viewport tests for both editors and reopening the default
USDA save with matching positions, velocities and time. One opt-in GPU test
was skipped.
