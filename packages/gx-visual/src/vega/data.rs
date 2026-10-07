//! Rows, field lookup, aggregation and the ordered categories of a field.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::spec::{Aggregate, ChartSpec, FieldDef, FieldType, SortSpec};
use super::temporal;
use crate::text::{format_number, format_plain};

pub(crate) type Row = Map<String, Value>;

/// A field's value in `row`: the literal key first, then a dotted path into nested objects.
pub(crate) fn lookup<'a>(row: &'a Row, field: &str) -> Option<&'a Value> {
    if let Some(value) = row.get(field) {
        return Some(value);
    }
    if !field.contains('.') {
        return None;
    }
    let mut parts = field.split('.');
    let mut current = row.get(parts.next()?)?;
    for part in parts {
        current = match current {
            Value::Object(map) => map.get(part)?,
            Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

pub(crate) fn field_exists(rows: &[Row], field: &str) -> bool {
    rows.iter().any(|row| lookup(row, field).is_some())
}

/// Vega-Lite's type for a field with no `type`: numbers only → quantitative, else nominal.
pub(crate) fn infer_type(rows: &[Row], field: &str) -> FieldType {
    let mut any = false;
    for row in rows {
        match lookup(row, field) {
            Some(Value::Number(_)) => any = true,
            Some(Value::Null) | None => {}
            Some(_) => return FieldType::Nominal,
        }
    }
    if any {
        FieldType::Quantitative
    } else {
        FieldType::Nominal
    }
}

/// A number, or a string holding one.
pub(crate) fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|v| v.is_finite())
}

/// A value as a category label; `None` for null.
pub(crate) fn label(value: &Value, ty: FieldType) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) if ty == FieldType::Temporal => Some(temporal::label(s)),
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => n.as_f64().map(|v| {
            if ty == FieldType::Temporal {
                temporal::number_label(v)
            } else {
                format_plain(v)
            }
        }),
        Value::Bool(b) => Some(b.to_string()),
        other => Some(other.to_string()),
    }
}

/// A raw value written for a tooltip.
pub(crate) fn display(value: &Value, ty: FieldType) -> String {
    match value {
        Value::Number(n) if ty == FieldType::Quantitative => {
            n.as_f64().map(format_number).unwrap_or_default()
        }
        other => label(other, ty).unwrap_or_default(),
    }
}

pub(crate) fn row_number(row: &Row, def: &FieldDef) -> Option<f64> {
    def.field
        .as_deref()
        .and_then(|field| lookup(row, field))
        .and_then(number)
}

pub(crate) fn row_label(row: &Row, def: &FieldDef) -> Option<String> {
    def.field
        .as_deref()
        .and_then(|field| lookup(row, field))
        .and_then(|value| label(value, def.ty))
}

pub(crate) fn aggregate(op: Aggregate, values: &[f64]) -> Option<f64> {
    if op == Aggregate::Count {
        return Some(values.len() as f64);
    }
    if values.is_empty() {
        return None;
    }
    let sum: f64 = values.iter().sum();
    Some(match op {
        Aggregate::Sum => sum,
        Aggregate::Mean => sum / values.len() as f64,
        Aggregate::Count => values.len() as f64,
        Aggregate::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
        Aggregate::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        Aggregate::Median => {
            let mut sorted = values.to_vec();
            sorted.sort_by(f64::total_cmp);
            let mid = sorted.len() / 2;
            if sorted.len().is_multiple_of(2) {
                (sorted[mid - 1] + sorted[mid]) / 2.0
            } else {
                sorted[mid]
            }
        }
    })
}

/// What `def` measures over the rows `idx`: its aggregate, or `default` when it has none.
pub(crate) fn measure(
    rows: &[Row],
    idx: &[usize],
    def: &FieldDef,
    default: Aggregate,
) -> Option<f64> {
    let op = def.aggregate.unwrap_or(default);
    if op == Aggregate::Count {
        return Some(idx.len() as f64);
    }
    let values: Vec<f64> = idx
        .iter()
        .filter_map(|&i| rows.get(i))
        .filter_map(|row| row_number(row, def))
        .collect();
    aggregate(op, &values).filter(|v| v.is_finite())
}

/// The ordered categories of a field.
pub(crate) struct Domain {
    pub labels: Vec<String>,
    index: BTreeMap<String, usize>,
}

impl Domain {
    pub(crate) fn index_of(&self, label: &str) -> Option<usize> {
        self.index.get(label).copied()
    }

    pub(crate) fn len(&self) -> usize {
        self.labels.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    /// The category index of `row` under `def`.
    pub(crate) fn of_row(&self, row: &Row, def: &FieldDef) -> Option<usize> {
        row_label(row, def).and_then(|label| self.index_of(&label))
    }
}

enum Key {
    Num(f64),
    Text(String),
}

fn sort_key(value: &Value, ty: FieldType) -> Key {
    match value {
        Value::Number(n) => Key::Num(n.as_f64().unwrap_or(0.0)),
        Value::String(s) if ty == FieldType::Temporal => {
            Key::Text(temporal::sort_key(s).unwrap_or_else(|| s.clone()))
        }
        Value::String(s) => Key::Text(s.clone()),
        other => Key::Text(other.to_string()),
    }
}

fn compare(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Num(a), Key::Num(b)) => a.total_cmp(b),
        (Key::Num(_), Key::Text(_)) => Ordering::Less,
        (Key::Text(_), Key::Num(_)) => Ordering::Greater,
        (Key::Text(a), Key::Text(b)) => a.cmp(b),
    }
}

struct Entry {
    label: String,
    key: Key,
    rows: Vec<usize>,
}

/// The categories of `def` in the order its `sort` asks for (Vega-Lite's default: ascending).
pub(crate) fn domain(spec: &ChartSpec, def: &FieldDef) -> Domain {
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    if let Some(field) = def.field.as_deref() {
        for (i, row) in spec.rows.iter().enumerate() {
            let Some(value) = lookup(row, field) else {
                continue;
            };
            let Some(label) = label(value, def.ty) else {
                continue;
            };
            match seen.get(&label) {
                Some(&at) => entries[at].rows.push(i),
                None => {
                    seen.insert(label.clone(), entries.len());
                    entries.push(Entry {
                        label,
                        key: sort_key(value, def.ty),
                        rows: vec![i],
                    });
                }
            }
        }
    }
    let by_value = |entries: &mut Vec<Entry>, value: &dyn Fn(&Entry) -> f64, descending: bool| {
        let mut keyed: Vec<(f64, Entry)> = entries.drain(..).map(|e| (value(&e), e)).collect();
        keyed.sort_by(|a, b| {
            let order = a.0.total_cmp(&b.0);
            if descending {
                order.reverse()
            } else {
                order
            }
        });
        entries.extend(keyed.into_iter().map(|(_, e)| e));
    };
    match &def.sort {
        SortSpec::Ascending => entries.sort_by(|a, b| compare(&a.key, &b.key)),
        SortSpec::Descending => entries.sort_by(|a, b| compare(&b.key, &a.key)),
        SortSpec::Data => {}
        SortSpec::Explicit(order) => entries.sort_by_key(|e| {
            order
                .iter()
                .position(|o| *o == e.label)
                .unwrap_or(usize::MAX)
        }),
        SortSpec::ByChannel {
            channel,
            descending,
        } => match spec
            .channel(channel)
            .filter(|other| other.channel != def.channel)
        {
            Some(other) => by_value(
                &mut entries,
                &|e: &Entry| measure(&spec.rows, &e.rows, other, Aggregate::Sum).unwrap_or(0.0),
                *descending,
            ),
            None if *descending => entries.sort_by(|a, b| compare(&b.key, &a.key)),
            None => entries.sort_by(|a, b| compare(&a.key, &b.key)),
        },
        SortSpec::ByField {
            field,
            op,
            descending,
        } => {
            let probe = FieldDef {
                field: field.clone(),
                aggregate: Some(*op),
                ..def.clone()
            };
            by_value(
                &mut entries,
                &|e: &Entry| measure(&spec.rows, &e.rows, &probe, *op).unwrap_or(0.0),
                *descending,
            );
        }
    }
    let labels: Vec<String> = entries.into_iter().map(|e| e.label).collect();
    let index = labels
        .iter()
        .enumerate()
        .map(|(i, label)| (label.clone(), i))
        .collect();
    Domain { labels, index }
}
