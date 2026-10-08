//! The Vega-Lite subset: parsing a spec and drawing it per mark.

mod arc;
mod axis;
mod bar;
mod data;
mod legend;
mod line;
mod point;
mod scale;
mod spec;
mod temporal;
mod tooltip;

pub(crate) use spec::{parse_chart, unsupported_feature, ChartSpec};

use crate::scene::Block;
use crate::theme::{Color, Theme};
use data::Domain;
use spec::MarkKind;

/// Color series a chart may draw.
const MAX_SERIES: usize = 24;

pub(crate) fn render_chart(spec: &ChartSpec, width: f32, theme: &Theme) -> Result<Block, String> {
    match spec.mark.kind {
        MarkKind::Bar => bar::render(spec, width, theme),
        MarkKind::Line | MarkKind::Area => line::render(spec, width, theme),
        MarkKind::Point => point::render(spec, width, theme),
        MarkKind::Arc => arc::render(spec, width, theme),
    }
}

/// The chart's color series: the color field's categories, or one series in the constant color.
pub(crate) struct Series {
    pub domain: Option<Domain>,
    pub colors: Vec<Color>,
}

impl Series {
    pub(crate) fn of(spec: &ChartSpec, theme: &Theme) -> Result<Series, String> {
        let Some(def) = spec.color_field() else {
            let color = spec
                .color_value
                .or(spec.mark.color)
                .unwrap_or_else(|| theme.series_color(0));
            return Ok(Series {
                domain: None,
                colors: vec![color],
            });
        };
        let domain = data::domain(spec, def);
        if domain.len() > MAX_SERIES {
            return Err(format!(
                "This chart has {} color series; the limit is {MAX_SERIES}. Group the smaller ones first.",
                domain.len()
            ));
        }
        let colors = (0..domain.len().max(1))
            .map(|i| theme.series_color(i))
            .collect();
        Ok(Series {
            domain: Some(domain),
            colors,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.domain.as_ref().map(Domain::len).unwrap_or(1).max(1)
    }

    pub(crate) fn color(&self, i: usize) -> Color {
        self.colors
            .get(i)
            .or_else(|| self.colors.first())
            .copied()
            .unwrap_or(Color::rgb(128, 128, 128))
    }

    pub(crate) fn label(&self, i: usize) -> Option<&str> {
        self.domain
            .as_ref()
            .and_then(|d| d.labels.get(i))
            .map(String::as_str)
    }

    /// The series of `row`: its color category, or 0 with no color field. `None` drops the row.
    pub(crate) fn of_row(&self, spec: &ChartSpec, row: &data::Row) -> Option<usize> {
        match (&self.domain, spec.color_field()) {
            (Some(domain), Some(def)) => domain.of_row(row, def),
            _ => Some(0),
        }
    }

    /// Legend entries when more than one series is drawn.
    pub(crate) fn legend(&self) -> Vec<legend::Entry> {
        match &self.domain {
            Some(domain) if domain.len() > 1 => domain
                .labels
                .iter()
                .enumerate()
                .map(|(i, label)| legend::Entry {
                    label: label.clone(),
                    color: self.color(i),
                })
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Draws the legend row at the top of a chart; returns where the plot starts.
pub(crate) fn legend_on_top(block: &mut Block, series: &Series, width: f32, theme: &Theme) -> f32 {
    let entries = series.legend();
    if entries.is_empty() {
        return 0.0;
    }
    legend::row(block, &entries, 0.0, 0.0, width, theme) + 8.0
}

/// "No rows have …" for a channel whose field holds nothing usable.
pub(crate) fn no_values(def: &spec::FieldDef) -> String {
    match &def.field {
        Some(field) => format!(
            "No rows have a usable value for \"{field}\" in encoding.{}.",
            def.channel
        ),
        None => "This chart has no data rows.".to_string(),
    }
}
