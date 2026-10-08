//! How a chart's data draws itself in the first time it is shown: bars grow from their base,
//! lines and areas are revealed from the left, pie slices sweep round from twelve o'clock, and
//! points scale up. Hosts animate a scene by painting [`Scene::at`] for a growing `progress`.
//!
//! CDXC:SessionChat 2026-10-08 DECISION: User asked for charts to draw in when they first appear (the idea of GPUI Kit's chart appear motion, #3296), on the desktop and the phone. One ease-out over `chart-motion.json`'s duration, once per chart per run of the app, and never with reduced motion on.
//! CDXC:SessionChat 2026-10-08 SEE-ALSO: `apps/desktop/src/app/native_chat/visual.rs` paints the frames; the phone asks for them through the core's `renderVisual` query (`apps/mobile/app/src/chat/native/transcript/VisualBlock.tsx`).

use serde::Serialize;

use crate::scene::{Item, Scene};

/// How one data mark moves while its chart draws in.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Motion {
    /// A bar grows from `base`, its zero line or stack base, along x when `horizontal`, else y.
    Grow { base: f32, horizontal: bool },
    /// A line or an area is revealed from `from` to `to`, its leftmost and rightmost x.
    Reveal { from: f32, to: f32 },
    /// A pie or donut slice from `start` to `end` (radians clockwise from twelve o'clock), shown
    /// as far as the sweep has come round.
    Sweep {
        cx: f32,
        cy: f32,
        outer: f32,
        inner: f32,
        start: f32,
        end: f32,
        full: bool,
    },
    /// A point scales up from its centre.
    Pop,
}

impl Motion {
    pub(crate) fn translate(self, dx: f32, dy: f32) -> Motion {
        match self {
            Motion::Grow { base, horizontal } => Motion::Grow {
                base: base + if horizontal { dx } else { dy },
                horizontal,
            },
            Motion::Reveal { from, to } => Motion::Reveal {
                from: from + dx,
                to: to + dx,
            },
            Motion::Sweep {
                cx,
                cy,
                outer,
                inner,
                start,
                end,
                full,
            } => Motion::Sweep {
                cx: cx + dx,
                cy: cy + dy,
                outer,
                inner,
                start,
                end,
                full,
            },
            Motion::Pop => Motion::Pop,
        }
    }
}

/// Chart.js's default appear curve (`easeOutQuart`): fast at first, settling gently.
fn ease(progress: f32) -> f32 {
    1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(4)
}

impl Scene {
    /// Whether any item in the scene draws in, so a host knows there is something to animate.
    pub fn has_motion(&self) -> bool {
        !self.motions.is_empty()
    }

    /// The scene as it looks `progress` (0 to 1, linear in time) of the way through drawing in.
    /// Axes, labels, legends and tooltips' regions stay as they are; only data marks move.
    pub fn at(&self, progress: f32) -> Scene {
        let mut scene = self.clone();
        let t = ease(progress);
        if t >= 1.0 {
            return scene;
        }
        for &(index, motion) in &self.motions {
            if let Some(item) = scene.items.get_mut(index) {
                apply(item, motion, t);
            }
        }
        scene
    }
}

fn apply(item: &mut Item, motion: Motion, t: f32) {
    match (motion, item) {
        (
            Motion::Grow { base, horizontal },
            Item::Rect {
                x,
                y,
                width,
                height,
                radius,
                ..
            },
        ) => {
            let grow = |lo: f32, len: f32| {
                let near = base + (lo - base) * t;
                let far = base + (lo + len - base) * t;
                (near.min(far), (far - near).abs())
            };
            if horizontal {
                (*x, *width) = grow(*x, *width);
            } else {
                (*y, *height) = grow(*y, *height);
            }
            *radius = radius.min(*width / 2.0).min(*height / 2.0);
        }
        (
            Motion::Pop,
            Item::Rect {
                x,
                y,
                width,
                height,
                radius,
                ..
            },
        ) => {
            let (cx, cy) = (*x + *width / 2.0, *y + *height / 2.0);
            *width *= t;
            *height *= t;
            *radius *= t;
            *x = cx - *width / 2.0;
            *y = cy - *height / 2.0;
        }
        (Motion::Reveal { from, to }, Item::Path { points, closed, .. }) => {
            let cut = from + (to - from) * t;
            *points = if *closed {
                clip_polygon(points, cut)
            } else {
                clip_polyline(points, cut)
            };
        }
        (
            Motion::Sweep {
                cx,
                cy,
                outer,
                inner,
                start,
                end,
                full,
            },
            Item::Path { points, .. },
        ) => {
            let reached = std::f32::consts::TAU * t;
            *points = if reached <= start {
                Vec::new()
            } else {
                let shown = reached.min(end);
                crate::vega::wedge(cx, cy, outer, inner, start, shown, full && shown >= end)
            };
        }
        _ => {}
    }
}

/// Where the segment from `a` to `b` crosses the vertical line `x = cut`.
fn crossing(a: [f32; 2], b: [f32; 2], cut: f32) -> [f32; 2] {
    let span = b[0] - a[0];
    if span.abs() < f32::EPSILON {
        return [cut, a[1]];
    }
    let f = (cut - a[0]) / span;
    [cut, a[1] + (b[1] - a[1]) * f]
}

/// The part of an open line left of `cut`, ending where it crosses `cut`.
fn clip_polyline(points: &[[f32; 2]], cut: f32) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(points.len());
    for (i, &point) in points.iter().enumerate() {
        if point[0] <= cut {
            out.push(point);
        } else {
            if let Some(&previous) = i.checked_sub(1).and_then(|p| points.get(p)) {
                if previous[0] <= cut {
                    out.push(crossing(previous, point, cut));
                }
            }
            break;
        }
    }
    out
}

/// The part of a closed polygon left of `cut` (one Sutherland–Hodgman pass).
fn clip_polygon(points: &[[f32; 2]], cut: f32) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(points.len() + 2);
    for (i, &current) in points.iter().enumerate() {
        let previous = points[(i + points.len() - 1) % points.len()];
        let (inside_now, inside_before) = (current[0] <= cut, previous[0] <= cut);
        if inside_now {
            if !inside_before {
                out.push(crossing(previous, current, cut));
            }
            out.push(current);
        } else if inside_before {
            out.push(crossing(previous, current, cut));
        }
    }
    out
}
