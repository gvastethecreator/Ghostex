//! Point charts: scatter plots (both axes quantitative) and dot plots (one or both categorical).

use std::collections::BTreeMap;

use super::axis::{self, AxisKind, AxisSpec, Scale};
use super::data::{self, Domain};
use super::scale::{is_year_axis, quant_domain, TickFormat};
use super::spec::{Aggregate, ChartSpec, FieldDef};
use super::tooltip::{self, Shown};
use super::{legend_on_top, no_values, Series};
use crate::scene::Block;
use crate::text::{format_number, format_plain};
use crate::theme::Theme;

const RADIUS: f32 = 4.0;

/// A position on one axis: a category index or a number.
#[derive(Clone, Copy)]
enum At {
    Cat(usize),
    Num(f64),
}

struct Dot {
    x: At,
    y: At,
    series: usize,
    rows: Vec<usize>,
}

/// How one axis reads a row: through its categories, or as a number.
fn position(row: &data::Row, def: &FieldDef, domain: Option<&Domain>) -> Option<At> {
    match domain {
        Some(domain) => domain.of_row(row, def).map(At::Cat),
        None => data::row_number(row, def).map(At::Num),
    }
}

pub(crate) fn render(spec: &ChartSpec, width: f32, theme: &Theme) -> Result<Block, String> {
    let (Some(x), Some(y)) = (spec.x.as_ref(), spec.y.as_ref()) else {
        return Err("A point chart needs both x and y in encoding.".to_string());
    };
    let series = Series::of(spec, theme)?;
    let x_domain = x.ty.is_category().then(|| data::domain(spec, x));
    let y_domain = y.ty.is_category().then(|| data::domain(spec, y));

    let mut dots: Vec<Dot> = Vec::new();
    if x.aggregate.is_none() && y.aggregate.is_none() {
        for (i, row) in spec.rows.iter().enumerate() {
            let (Some(px), Some(py), Some(s)) = (
                position(row, x, x_domain.as_ref()),
                position(row, y, y_domain.as_ref()),
                series.of_row(spec, row),
            ) else {
                continue;
            };
            dots.push(Dot {
                x: px,
                y: py,
                series: s,
                rows: vec![i],
            });
        }
    } else {
        // Group by the channels that are not aggregated, then aggregate the others.
        let key_of = |row: &data::Row, def: &FieldDef| -> Option<String> {
            if def.aggregate.is_some() {
                return Some(String::new());
            }
            if def.ty.is_category() {
                data::row_label(row, def)
            } else {
                data::row_number(row, def).map(format_plain)
            }
        };
        let mut groups: BTreeMap<(usize, String, String), Vec<usize>> = BTreeMap::new();
        for (i, row) in spec.rows.iter().enumerate() {
            let (Some(kx), Some(ky), Some(s)) =
                (key_of(row, x), key_of(row, y), series.of_row(spec, row))
            else {
                continue;
            };
            groups.entry((s, kx, ky)).or_default().push(i);
        }
        for ((s, _, _), rows) in groups {
            let first = rows.first().and_then(|&i| spec.rows.get(i));
            let axis_at = |def: &FieldDef, domain: Option<&Domain>| -> Option<At> {
                if def.aggregate.is_some() {
                    data::measure(&spec.rows, &rows, def, Aggregate::Sum).map(At::Num)
                } else {
                    position(first?, def, domain)
                }
            };
            if let (Some(px), Some(py)) =
                (axis_at(x, x_domain.as_ref()), axis_at(y, y_domain.as_ref()))
            {
                dots.push(Dot {
                    x: px,
                    y: py,
                    series: s,
                    rows,
                });
            }
        }
    }
    if dots.is_empty() {
        return Err(no_values(if x_domain.is_none() { x } else { y }));
    }

    let axis_for =
        |def: &FieldDef, domain: &Option<Domain>, pick: fn(&Dot) -> At| -> (AxisSpec, bool) {
            let mut years = false;
            let kind = match domain {
                Some(domain) => AxisKind::Band {
                    labels: domain.labels.clone(),
                    padding: 0.0,
                },
                None => {
                    let values: Vec<f64> = dots
                        .iter()
                        .filter_map(|d| match pick(d) {
                            At::Num(v) => Some(v),
                            At::Cat(_) => None,
                        })
                        .collect();
                    years = is_year_axis(def, &values);
                    AxisKind::Linear {
                        domain: quant_domain(&values, def, false),
                        nice: def.domain.is_none(),
                        format: if years {
                            TickFormat::Plain
                        } else {
                            TickFormat::Compact
                        },
                    }
                }
            };
            let spec = AxisSpec {
                kind,
                title: def.axis_title(),
                hidden: def.axis_hidden,
            };
            (spec, years)
        };
    let (x_axis, x_years) = axis_for(x, &x_domain, |d| d.x);
    let (y_axis, y_years) = axis_for(y, &y_domain, |d| d.y);

    let mut block = Block::new();
    let top = legend_on_top(&mut block, &series, width, theme);
    let height = spec.plot_height();
    let frame = axis::build(&mut block, theme, width, top, height, x_axis, y_axis);
    let place = |scale: &Scale, at: At| -> f32 {
        match (scale, at) {
            (Scale::Band(band), At::Cat(i)) => band.center(i),
            (Scale::Linear(linear), At::Num(v)) => linear.pos_clamped(v),
            (Scale::Band(band), At::Num(_)) => band.r0,
            (Scale::Linear(linear), At::Cat(_)) => linear.r0,
        }
    };
    let describe = |def: &FieldDef, at: At, domain: &Option<Domain>, years: bool| -> String {
        match (at, domain) {
            (At::Cat(i), Some(domain)) => domain.labels.get(i).cloned().unwrap_or_default(),
            (At::Num(v), _) if years && def.aggregate.is_none() => format_plain(v),
            (At::Num(v), _) => format_number(v),
            (At::Cat(_), None) => String::new(),
        }
    };
    for dot in &dots {
        let cx = place(&frame.x, dot.x);
        let cy = place(&frame.y, dot.y);
        let color = series.color(dot.series);
        block.circle(cx, cy, RADIUS, color.with_alpha(color.a * 0.85));
        let shape = block.last_item();
        let mut shown = vec![
            Shown::new(x, describe(x, dot.x, &x_domain, x_years)),
            Shown::new(y, describe(y, dot.y, &y_domain, y_years)),
        ];
        if let (Some(def), Some(label)) = (spec.color_field(), series.label(dot.series)) {
            shown.push(Shown::colored(def, label.to_string(), color));
        }
        let lines = tooltip::lines(spec, &dot.rows, &shown);
        let hit = RADIUS + 3.0;
        block.region(shape, cx - hit, cy - hit, hit * 2.0, hit * 2.0, lines);
    }
    frame.baseline(&mut block, theme);
    block.height = top + height;
    Ok(block)
}
