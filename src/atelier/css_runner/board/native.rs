//! Typed display paths for the metric board. No SVG serialization or path parsing.
use cssimpler::core::{
    Color, LengthPercentageCalc, Node, PathCommand, PathCommand::*, Style, VectorPaint, VectorPath,
    VectorPoint, VectorScene, VectorViewBox,
};
use glam::DVec2;

/// A display path with a semantic role used by board inspection and regression tests.
/// The document remains the source of editable identities and f64 geometry.
#[derive(Clone, Debug)]
pub struct BoardPath {
    pub path: VectorPath,
    pub class: &'static str,
    pub background: bool,
}

impl BoardPath {
    pub fn new(
        commands: Vec<PathCommand>,
        fill: Option<Color>,
        stroke: Option<Color>,
        stroke_width: f64,
        class: &'static str,
    ) -> Self {
        Self {
            path: VectorPath::new(
                commands,
                VectorPaint {
                    fill,
                    stroke,
                    stroke_width: stroke_width as f32,
                },
            )
            .expect("finite board display path"),
            class,
            background: false,
        }
    }
}

pub fn point((x, y): (f64, f64)) -> VectorPoint {
    VectorPoint::new(x as f32, y as f32)
}

pub fn polyline(points: &[(f64, f64)], closed: bool) -> Vec<PathCommand> {
    let mut commands = Vec::with_capacity(points.len() + usize::from(closed));
    if let Some((&first, rest)) = points.split_first() {
        commands.push(MoveTo(point(first)));
        commands.extend(rest.iter().map(|&p| LineTo(point(p))));
        if closed {
            commands.push(Close);
        }
    }
    commands
}

pub fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Vec<PathCommand> {
    polyline(
        &[
            (x, y),
            (x + width, y),
            (x + width, y + height),
            (x, y + height),
        ],
        true,
    )
}

pub fn circle(x: f64, y: f64, radius: f64) -> Vec<PathCommand> {
    // Four cubic quarter arcs; keep curves native until tiny-skia rasterization.
    let k = radius * 0.552_284_749_830_793_6;
    vec![
        MoveTo(point((x + radius, y))),
        CubicTo {
            control1: point((x + radius, y + k)),
            control2: point((x + k, y + radius)),
            end: point((x, y + radius)),
        },
        CubicTo {
            control1: point((x - k, y + radius)),
            control2: point((x - radius, y + k)),
            end: point((x - radius, y)),
        },
        CubicTo {
            control1: point((x - radius, y - k)),
            control2: point((x - k, y - radius)),
            end: point((x, y - radius)),
        },
        CubicTo {
            control1: point((x + k, y - radius)),
            control2: point((x + radius, y - k)),
            end: point((x + radius, y)),
        },
        Close,
    ]
}

/// Preserve dash phase across polyline segments without creating path strings.
pub fn dashed_commands(points: &[(f64, f64)], dash: f64, gap: f64) -> Vec<PathCommand> {
    if points.len() < 2 || dash <= 1e-6 {
        return Vec::new();
    }
    let period = dash + gap.max(0.0);
    let mut commands = Vec::new();
    let mut draw = true;
    let mut remaining = dash;
    for pair in points.windows(2) {
        let mut p0 = DVec2::new(pair[0].0, pair[0].1);
        let p1 = DVec2::new(pair[1].0, pair[1].1);
        let mut left = p0.distance(p1);
        if left <= 1e-9 {
            continue;
        }
        let direction = (p1 - p0) / left;
        while left > 1e-9 {
            let step = remaining.min(left);
            let next = p0 + direction * step;
            if draw {
                commands.push(MoveTo(point((p0.x, p0.y))));
                commands.push(LineTo(point((next.x, next.y))));
            }
            p0 = next;
            left -= step;
            remaining -= step;
            if remaining <= 1e-9 {
                draw = !draw;
                remaining = if draw { dash } else { gap.max(period * 0.1) };
            }
        }
    }
    commands
}

pub fn board_node(paths: Vec<BoardPath>, view_box: VectorViewBox, width: f32, height: f32) -> Node {
    let (background, drawing): (Vec<_>, Vec<_>) = paths.into_iter().partition(|p| p.background);
    let scene = |paths: Vec<BoardPath>| {
        VectorScene::new(view_box, paths.into_iter().map(|p| p.path).collect())
            .expect("valid metric board view box")
    };
    let background: Node = Node::vector(scene(background))
        .with_id("board-2d-background")
        .with_class("board-2d-layer")
        .with_cached_layer(true)
        .into();
    let drawing: Node = Node::vector(scene(drawing))
        .with_id("board-2d-drawing")
        .with_class("board-2d-layer")
        .with_cached_layer(false)
        .into();
    let mut style = Style::default();
    style.layout.calc.width = Some(LengthPercentageCalc::new(0.0, width));
    style.layout.calc.height = Some(LengthPercentageCalc::new(0.0, height));
    Node::element("div")
        .with_id("board-2d")
        .with_style(style)
        .with_children([background, drawing])
        .into()
}
