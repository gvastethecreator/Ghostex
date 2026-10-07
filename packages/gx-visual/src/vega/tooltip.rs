//! Tooltip lines for one mark: the spec's `encoding.tooltip` when given, else what the mark shows.

use super::data::{self, lookup};
use super::spec::{Aggregate, ChartSpec, FieldDef};
use crate::scene::TooltipLine;
use crate::text::format_number;
use crate::theme::Color;

/// One encoded value the mark shows: its field def, the text written for it, and a series color.
pub(crate) struct Shown<'a> {
    pub def: &'a FieldDef,
    pub value: String,
    pub color: Option<Color>,
}

impl<'a> Shown<'a> {
    pub(crate) fn new(def: &'a FieldDef, value: String) -> Shown<'a> {
        Shown {
            def,
            value,
            color: None,
        }
    }

    pub(crate) fn colored(def: &'a FieldDef, value: String, color: Color) -> Shown<'a> {
        Shown {
            def,
            value,
            color: Some(color),
        }
    }
}

/// The tooltip of a mark drawn from the rows `idx`.
pub(crate) fn lines(spec: &ChartSpec, idx: &[usize], shown: &[Shown]) -> Vec<TooltipLine> {
    if spec.tooltip.is_empty() {
        return shown
            .iter()
            .map(|s| TooltipLine {
                label: s.def.label(),
                value: s.value.clone(),
                color: s.color,
            })
            .collect();
    }
    spec.tooltip
        .iter()
        .map(|def| {
            let matching = shown.iter().find(|s| s.def.same_as(def));
            let value = match (matching, def.aggregate) {
                (Some(s), _) => s.value.clone(),
                (None, Some(op)) => data::measure(&spec.rows, idx, def, op)
                    .map(format_number)
                    .unwrap_or_default(),
                (None, None) => first_value(spec, idx, def),
            };
            TooltipLine {
                label: def.label(),
                value,
                color: matching.and_then(|s| s.color),
            }
        })
        .collect()
}

/// The field's value when every row of the mark agrees on it, else its sum or a "…" marker.
fn first_value(spec: &ChartSpec, idx: &[usize], def: &FieldDef) -> String {
    let Some(field) = def.field.as_deref() else {
        return format_number(idx.len() as f64);
    };
    let mut values = idx
        .iter()
        .filter_map(|&i| spec.rows.get(i))
        .filter_map(|row| lookup(row, field));
    let Some(first) = values.next() else {
        return String::new();
    };
    if values.all(|v| v == first) {
        return data::display(first, def.ty);
    }
    if !def.ty.is_category() {
        if let Some(sum) = data::measure(&spec.rows, idx, def, Aggregate::Sum) {
            return format_number(sum);
        }
    }
    format!("{} …", data::display(first, def.ty))
}
