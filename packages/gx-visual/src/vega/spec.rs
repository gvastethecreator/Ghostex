//! Reading a Vega-Lite spec into the strict subset Ghostex draws. Anything outside the subset is
//! an error that names it; a handful of presentation-only keys are ignored.

use serde_json::{Map, Value};

use super::data::{self, Row};
use crate::spec::title_text;
use crate::theme::{parse_css_color, Color};

/// Data rows a chart may hold.
pub(crate) const MAX_ROWS: usize = 5000;

/// Top-level Vega-Lite keys outside the subset.
const UNSUPPORTED_KEYS: &[&str] = &[
    "layer",
    "facet",
    "repeat",
    "concat",
    "hconcat",
    "vconcat",
    "transform",
    "params",
    "selection",
    "projection",
    "spec",
];

/// Encoding channels that change nothing in a static native drawing.
const IGNORED_CHANNELS: &[&str] = &[
    "size",
    "opacity",
    "fillOpacity",
    "strokeOpacity",
    "strokeWidth",
    "strokeDash",
    "shape",
    "order",
    "detail",
    "key",
    "href",
    "description",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkKind {
    Bar,
    Line,
    Area,
    Point,
    Arc,
}

#[derive(Clone, Debug)]
pub(crate) struct Mark {
    pub kind: MarkKind,
    pub point: bool,
    pub inner_radius: f32,
    pub color: Option<Color>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldType {
    Quantitative,
    Nominal,
    Ordinal,
    Temporal,
}

impl FieldType {
    pub(crate) fn is_category(self) -> bool {
        self != FieldType::Quantitative
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Aggregate {
    Sum,
    Mean,
    Count,
    Min,
    Max,
    Median,
}

#[derive(Clone, Debug)]
pub(crate) enum SortSpec {
    Ascending,
    Descending,
    /// `sort: null`: the order rows first mention each value.
    Data,
    ByChannel {
        channel: String,
        descending: bool,
    },
    ByField {
        field: Option<String>,
        op: Aggregate,
        descending: bool,
    },
    Explicit(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StackMode {
    Default,
    Off,
    Zero,
    Normalize,
}

#[derive(Clone, Debug)]
pub(crate) struct FieldDef {
    pub channel: &'static str,
    pub field: Option<String>,
    pub ty: FieldType,
    pub aggregate: Option<Aggregate>,
    /// `Some(None)` is an explicit `"title": null`.
    pub title: Option<Option<String>>,
    pub sort: SortSpec,
    pub axis_hidden: bool,
    pub axis_title: Option<Option<String>>,
    pub domain: Option<(f64, f64)>,
    pub zero: Option<bool>,
    pub stack: StackMode,
}

impl FieldDef {
    /// The name tooltips and legends use.
    pub(crate) fn label(&self) -> String {
        if let Some(Some(title)) = &self.title {
            return title.clone();
        }
        match (&self.field, self.aggregate) {
            (None, _) | (Some(_), Some(Aggregate::Count)) => "Count".to_string(),
            (Some(field), _) => field.clone(),
        }
    }

    /// The axis title, or `None` when the axis is hidden or its title is switched off.
    pub(crate) fn axis_title(&self) -> Option<String> {
        if self.axis_hidden {
            return None;
        }
        if let Some(title) = &self.axis_title {
            return title.clone();
        }
        match &self.title {
            Some(title) => title.clone(),
            None => Some(self.label()),
        }
    }

    pub(crate) fn same_as(&self, other: &FieldDef) -> bool {
        self.field == other.field && self.aggregate == other.aggregate
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ChartSpec {
    pub mark: Mark,
    pub height: Option<f32>,
    pub title: Option<String>,
    pub rows: Vec<Row>,
    pub x: Option<FieldDef>,
    pub y: Option<FieldDef>,
    pub theta: Option<FieldDef>,
    pub x_offset: Option<FieldDef>,
    pub color: Option<FieldDef>,
    pub color_value: Option<Color>,
    pub tooltip: Vec<FieldDef>,
}

impl ChartSpec {
    pub(crate) fn channel(&self, name: &str) -> Option<&FieldDef> {
        match name {
            "x" => self.x.as_ref(),
            "y" => self.y.as_ref(),
            "theta" => self.theta.as_ref(),
            "color" => self.color.as_ref(),
            "xOffset" => self.x_offset.as_ref(),
            _ => None,
        }
    }

    /// The color field, when color encodes a field.
    pub(crate) fn color_field(&self) -> Option<&FieldDef> {
        self.color.as_ref().filter(|def| def.field.is_some())
    }

    /// The chart's plot height ("height", clamped), before a legend is added on top.
    pub(crate) fn plot_height(&self) -> f32 {
        let default = if self.mark.kind == MarkKind::Arc {
            220.0
        } else {
            240.0
        };
        self.height
            .filter(|h| h.is_finite())
            .map(|h| h.clamp(100.0, 600.0))
            .unwrap_or(default)
    }
}

/// The error for a Vega-Lite key outside the subset, when `obj` has one.
pub(crate) fn unsupported_feature(obj: &Map<String, Value>) -> Option<String> {
    UNSUPPORTED_KEYS
        .iter()
        .find(|key| obj.contains_key(**key))
        .map(|key| format!("Unsupported Vega-Lite feature: \"{key}\"."))
}

pub(crate) fn parse_chart(obj: &Map<String, Value>) -> Result<ChartSpec, String> {
    if let Some(error) = unsupported_feature(obj) {
        return Err(error);
    }
    let mark = parse_mark(obj.get("mark"))?;
    let rows = parse_data(obj.get("data"))?;
    let encoding = match obj.get("encoding") {
        Some(Value::Object(encoding)) => encoding,
        Some(_) => return Err("\"encoding\" must be an object.".to_string()),
        None => {
            return Err(
                "This chart has no \"encoding\"; map fields to x and y (theta for a pie)."
                    .to_string(),
            )
        }
    };
    let mut spec = ChartSpec {
        mark,
        height: obj.get("height").and_then(Value::as_f64).map(|h| h as f32),
        title: title_text(obj.get("title")),
        rows,
        x: None,
        y: None,
        theta: None,
        x_offset: None,
        color: None,
        color_value: None,
        tooltip: Vec::new(),
    };
    for (channel, def) in encoding {
        match channel.as_str() {
            "x" => spec.x = parse_def("x", def, &spec.rows)?,
            "y" => spec.y = parse_def("y", def, &spec.rows)?,
            "theta" => spec.theta = parse_def("theta", def, &spec.rows)?,
            "xOffset" => {
                spec.x_offset = parse_def("xOffset", def, &spec.rows)?;
                if spec.x_offset.as_ref().is_some_and(|d| !d.ty.is_category()) {
                    return Err("encoding.xOffset must be a nominal or ordinal field.".to_string());
                }
            }
            "color" => {
                let constant = def
                    .as_object()
                    .filter(|d| !d.contains_key("field") && !d.contains_key("aggregate"))
                    .and_then(|d| d.get("value"));
                match constant {
                    Some(Value::String(s)) => {
                        spec.color_value = Some(parse_css_color(s).ok_or_else(|| {
                            format!("Unknown color \"{s}\" in encoding.color.value.")
                        })?);
                    }
                    Some(_) => {}
                    None => spec.color = parse_def("color", def, &spec.rows)?,
                }
            }
            "tooltip" => spec.tooltip = parse_tooltip(def, &spec.rows)?,
            other if IGNORED_CHANNELS.contains(&other) => {}
            other => return Err(format!("Unsupported encoding channel \"{other}\".")),
        }
    }
    Ok(spec)
}

fn parse_mark(value: Option<&Value>) -> Result<Mark, String> {
    let (kind, props) = match value {
        Some(Value::String(kind)) => (kind.as_str(), None),
        Some(Value::Object(props)) => match props.get("type").and_then(Value::as_str) {
            Some(kind) => (kind, Some(props)),
            None => return Err("\"mark\" needs a \"type\", like \"bar\" or \"line\".".to_string()),
        },
        None => return Err("This chart has no \"mark\".".to_string()),
        Some(_) => {
            return Err(
                "\"mark\" must be a string like \"bar\" or an object with \"type\".".to_string(),
            )
        }
    };
    let kind = match kind {
        "bar" => MarkKind::Bar,
        "line" => MarkKind::Line,
        "area" => MarkKind::Area,
        "point" | "circle" | "square" => MarkKind::Point,
        "arc" => MarkKind::Arc,
        other => return Err(format!("Unsupported mark: \"{other}\".")),
    };
    let mut mark = Mark {
        kind,
        point: false,
        inner_radius: 0.0,
        color: None,
    };
    if let Some(props) = props {
        mark.point = match props.get("point") {
            Some(Value::Bool(b)) => *b,
            Some(Value::Object(_)) => true,
            _ => false,
        };
        mark.inner_radius = props
            .get("innerRadius")
            .and_then(Value::as_f64)
            .filter(|r| r.is_finite() && *r > 0.0)
            .map(|r| r as f32)
            .unwrap_or(0.0);
        if let Some(Value::String(color)) = props.get("color") {
            mark.color = Some(
                parse_css_color(color)
                    .ok_or_else(|| format!("Unknown color \"{color}\" in mark.color."))?,
            );
        }
    }
    Ok(mark)
}

fn parse_data(value: Option<&Value>) -> Result<Vec<Row>, String> {
    let Some(value) = value else {
        return Err("This chart has no data rows; put them in data.values.".to_string());
    };
    let Value::Object(data) = value else {
        return Err("\"data\" must be an object with \"values\".".to_string());
    };
    for key in ["url", "name", "sequence"] {
        if data.contains_key(key) {
            return Err(format!(
                "data.{key} isn't supported; put the rows in data.values."
            ));
        }
    }
    let values = match data.get("values") {
        Some(Value::Array(values)) => values,
        Some(_) => return Err("data.values must be an array of row objects.".to_string()),
        None => return Err("This chart has no data rows; put them in data.values.".to_string()),
    };
    if values.is_empty() {
        return Err("This chart has no data rows.".to_string());
    }
    if values.len() > MAX_ROWS {
        return Err(format!(
            "This chart has {} data rows; the limit is {MAX_ROWS}. Aggregate the rows first.",
            values.len()
        ));
    }
    Ok(values
        .iter()
        .map(|value| match value {
            Value::Object(row) => row.clone(),
            other => {
                let mut row = Map::new();
                row.insert("data".to_string(), other.clone());
                row
            }
        })
        .collect())
}

fn parse_tooltip(value: &Value, rows: &[Row]) -> Result<Vec<FieldDef>, String> {
    match value {
        Value::Null | Value::Bool(_) => Ok(Vec::new()),
        Value::Array(defs) => {
            let mut out = Vec::new();
            for def in defs {
                if let Some(def) = parse_def("tooltip", def, rows)? {
                    out.push(def);
                }
            }
            Ok(out)
        }
        other => Ok(parse_def("tooltip", other, rows)?.into_iter().collect()),
    }
}

fn parse_aggregate(name: &str) -> Option<Aggregate> {
    Some(match name {
        "sum" => Aggregate::Sum,
        "mean" | "average" => Aggregate::Mean,
        "count" => Aggregate::Count,
        "min" => Aggregate::Min,
        "max" => Aggregate::Max,
        "median" => Aggregate::Median,
        _ => return None,
    })
}

fn parse_def(
    channel: &'static str,
    value: &Value,
    rows: &[Row],
) -> Result<Option<FieldDef>, String> {
    let Value::Object(def) = value else {
        return Err(format!(
            "encoding.{channel} must be an object like {{\"field\": …, \"type\": …}}."
        ));
    };
    if let Some(bin) = def.get("bin") {
        if !matches!(bin, Value::Bool(false) | Value::Null) {
            return Err("bin isn't supported; aggregate the rows first.".to_string());
        }
    }
    let aggregate = match def.get("aggregate") {
        None | Some(Value::Null) => None,
        Some(Value::String(name)) => Some(parse_aggregate(name).ok_or_else(|| {
            format!("Unsupported aggregate \"{name}\" in encoding.{channel}; use sum, mean, count, min, max or median.")
        })?),
        Some(_) => {
            return Err(format!(
                "Unsupported aggregate in encoding.{channel}; use sum, mean, count, min, max or median."
            ))
        }
    };
    let field = match def.get("field") {
        None | Some(Value::Null) => None,
        Some(Value::String(field)) => Some(field.clone()),
        Some(_) => return Err(format!("encoding.{channel}.field must be a field name.")),
    };
    if field.is_none() && aggregate != Some(Aggregate::Count) {
        if def.contains_key("value") || def.contains_key("datum") || def.contains_key("condition") {
            return Ok(None);
        }
        return Err(format!("encoding.{channel} needs a \"field\"."));
    }
    if let Some(field) = &field {
        if !data::field_exists(rows, field) {
            return Err(format!("Unknown field \"{field}\" in encoding.{channel}."));
        }
    }
    let declared =
        match def.get("type").and_then(Value::as_str) {
            Some(name) => Some(parse_type(name).ok_or_else(|| {
                format!("Unsupported field type \"{name}\" in encoding.{channel}.")
            })?),
            None => None,
        };
    let ty = match (aggregate, declared, &field) {
        (Some(_), _, _) => FieldType::Quantitative,
        (None, Some(ty), _) => ty,
        (None, None, Some(field)) => data::infer_type(rows, field),
        (None, None, None) => FieldType::Quantitative,
    };
    let title = match def.get("title") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(other) => Some(title_text(Some(other))),
    };
    let (axis_hidden, axis_title) = match def.get("axis") {
        Some(Value::Null) | Some(Value::Bool(false)) => (true, None),
        Some(Value::Object(axis)) => (
            false,
            match axis.get("title") {
                None => None,
                Some(Value::Null) => Some(None),
                Some(other) => Some(title_text(Some(other))),
            },
        ),
        _ => (false, None),
    };
    let mut sort = parse_sort(channel, def.get("sort"))?;
    let mut domain = None;
    let mut zero = None;
    if let Some(Value::Object(scale)) = def.get("scale") {
        zero = scale.get("zero").and_then(Value::as_bool);
        if let Some(Value::Array(values)) = scale.get("domain") {
            let numbers: Vec<f64> = values.iter().filter_map(Value::as_f64).collect();
            if numbers.len() == 2 && values.len() == 2 && numbers[0] != numbers[1] {
                domain = Some((numbers[0].min(numbers[1]), numbers[0].max(numbers[1])));
            } else if ty.is_category() && matches!(sort, SortSpec::Ascending) {
                sort =
                    SortSpec::Explicit(values.iter().filter_map(|v| data::label(v, ty)).collect());
            }
        }
    }
    let stack = match def.get("stack") {
        None => StackMode::Default,
        Some(Value::Null) | Some(Value::Bool(false)) => StackMode::Off,
        Some(Value::String(mode)) if mode == "normalize" => StackMode::Normalize,
        Some(_) => StackMode::Zero,
    };
    Ok(Some(FieldDef {
        channel,
        field,
        ty,
        aggregate,
        title,
        sort,
        axis_hidden,
        axis_title,
        domain,
        zero,
        stack,
    }))
}

fn parse_type(name: &str) -> Option<FieldType> {
    Some(match name {
        "quantitative" | "Q" => FieldType::Quantitative,
        "nominal" | "N" => FieldType::Nominal,
        "ordinal" | "O" => FieldType::Ordinal,
        "temporal" | "T" => FieldType::Temporal,
        _ => return None,
    })
}

fn parse_sort(channel: &str, value: Option<&Value>) -> Result<SortSpec, String> {
    let descending_of = |order: Option<&Value>| order.and_then(Value::as_str) == Some("descending");
    Ok(match value {
        None => SortSpec::Ascending,
        Some(Value::Null) => SortSpec::Data,
        Some(Value::String(order)) => match order.as_str() {
            "ascending" => SortSpec::Ascending,
            "descending" => SortSpec::Descending,
            other => {
                let (name, descending) = match other.strip_prefix('-') {
                    Some(name) => (name, true),
                    None => (other, false),
                };
                if !matches!(name, "x" | "y" | "color" | "theta" | "xOffset") {
                    return Err(format!(
                        "Unsupported sort \"{other}\" in encoding.{channel}."
                    ));
                }
                SortSpec::ByChannel {
                    channel: name.to_string(),
                    descending,
                }
            }
        },
        Some(Value::Array(values)) => SortSpec::Explicit(
            values
                .iter()
                .filter_map(|v| data::label(v, FieldType::Nominal))
                .collect(),
        ),
        Some(Value::Object(sort)) => {
            let descending = descending_of(sort.get("order"));
            if let Some(name) = sort.get("encoding").and_then(Value::as_str) {
                SortSpec::ByChannel {
                    channel: name.to_string(),
                    descending,
                }
            } else if sort.contains_key("field") || sort.contains_key("op") {
                let op = match sort.get("op").and_then(Value::as_str) {
                    Some(op) => parse_aggregate(op).ok_or_else(|| {
                        format!("Unsupported sort op \"{op}\" in encoding.{channel}.")
                    })?,
                    None => Aggregate::Sum,
                };
                SortSpec::ByField {
                    field: sort
                        .get("field")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    op,
                    descending,
                }
            } else if descending {
                SortSpec::Descending
            } else {
                SortSpec::Ascending
            }
        }
        Some(_) => SortSpec::Ascending,
    })
}
