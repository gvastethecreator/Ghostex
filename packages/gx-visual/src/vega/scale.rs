//! Linear scales with "nice" ticks, and band scales for categories.

use super::spec::FieldDef;
use crate::text::{format_percent_tick, format_plain, format_tick};

/// How a linear axis writes its ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TickFormat {
    /// 1.2k, 3.4M.
    Compact,
    /// 25%, for normalized stacks.
    Percent,
    /// Whole numbers written as is (years).
    Plain,
}

pub(crate) struct Linear {
    pub d0: f64,
    pub d1: f64,
    pub r0: f32,
    pub r1: f32,
    pub ticks: Vec<f64>,
    pub step: f64,
    pub format: TickFormat,
}

impl Linear {
    /// A scale over `domain`, widened to round ticks when `nice`, aiming at `target` intervals.
    pub(crate) fn new(domain: (f64, f64), nice: bool, target: usize, format: TickFormat) -> Linear {
        let (mut lo, mut hi) = domain;
        if !(hi - lo).is_finite() {
            lo = 0.0;
            hi = 1.0;
        }
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        if hi - lo <= f64::EPSILON * lo.abs().max(hi.abs()).max(1.0) {
            if lo == 0.0 {
                hi = 1.0;
            } else {
                let pad = (lo.abs() * 0.1).max(f64::EPSILON);
                lo -= pad;
                hi += pad;
            }
        }
        let mut step = nice_step(hi - lo, target);
        if format == TickFormat::Plain {
            step = step.max(1.0).round();
        }
        if nice {
            lo = (lo / step).floor() * step;
            hi = (hi / step).ceil() * step;
        }
        let ticks = ticks_within(lo, hi, step);
        Linear {
            d0: lo,
            d1: hi,
            r0: 0.0,
            r1: 1.0,
            ticks,
            step,
            format,
        }
    }

    pub(crate) fn pos(&self, v: f64) -> f32 {
        let span = self.d1 - self.d0;
        if span.is_nan() || span <= 0.0 {
            return (self.r0 + self.r1) / 2.0;
        }
        let t = ((v - self.d0) / span) as f32;
        self.r0 + t * (self.r1 - self.r0)
    }

    pub(crate) fn pos_clamped(&self, v: f64) -> f32 {
        self.pos(v.clamp(self.d0, self.d1))
    }

    pub(crate) fn contains(&self, v: f64) -> bool {
        v >= self.d0 && v <= self.d1
    }

    pub(crate) fn label(&self, v: f64) -> String {
        match self.format {
            TickFormat::Compact => format_tick(v, self.step),
            TickFormat::Percent => format_percent_tick(v, self.step),
            TickFormat::Plain => format_plain(v.round()),
        }
    }
}

/// A round step (1, 2, 2.5 or 5 × 10ⁿ) that splits `range` into about `target` intervals.
fn nice_step(range: f64, target: usize) -> f64 {
    let raw = range / target.max(1) as f64;
    if !raw.is_finite() || raw <= 0.0 {
        return 1.0;
    }
    let magnitude = 10f64.powf(raw.log10().floor());
    let norm = raw / magnitude;
    let nice = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

fn ticks_within(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    if !step.is_finite() || step <= 0.0 {
        return vec![lo, hi];
    }
    let first = (lo / step - 1e-9).ceil();
    let last = (hi / step + 1e-9).floor();
    if !(first.is_finite() && last.is_finite()) || last - first > 60.0 {
        return vec![lo, hi];
    }
    let mut ticks = Vec::new();
    let mut k = first;
    while k <= last {
        let v = k * step;
        ticks.push(if v == 0.0 { 0.0 } else { v });
        k += 1.0;
    }
    ticks
}

/// The domain of a quantitative axis: the field's `scale.domain` when given, else the data's
/// extent with zero included for bars and areas (`force_zero`), or for lines and points when the
/// data sits near zero or `scale.zero` asks for it.
pub(crate) fn quant_domain(values: &[f64], def: &FieldDef, force_zero: bool) -> (f64, f64) {
    if let Some(domain) = def.domain {
        return domain;
    }
    let finite = values.iter().copied().filter(|v| v.is_finite());
    let (mut lo, mut hi) = finite.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
        (lo.min(v), hi.max(v))
    });
    if lo > hi {
        return (0.0, 1.0);
    }
    let zero = force_zero
        || match def.zero {
            Some(zero) => zero,
            None => (lo >= 0.0 && lo <= 0.5 * hi) || (hi <= 0.0 && hi >= 0.5 * lo),
        };
    if zero {
        lo = lo.min(0.0);
        hi = hi.max(0.0);
    }
    (lo, hi)
}

/// Whether a quantitative axis holds years, written without separators or compacting.
pub(crate) fn is_year_axis(def: &FieldDef, values: &[f64]) -> bool {
    let named = def
        .field
        .as_deref()
        .map(|f| f.to_ascii_lowercase())
        .is_some_and(|f| f == "year" || f == "yr" || f.ends_with("_year") || f.ends_with(" year"));
    let whole_years = !values.is_empty()
        && values
            .iter()
            .all(|v| v.fract() == 0.0 && (1800.0..=2200.0).contains(v));
    whole_years && (named || values.iter().all(|v| (1900.0..=2100.0).contains(v)))
}

/// Categories spread evenly over a range; `padding` is the share of each step left empty.
#[derive(Clone, Copy)]
pub(crate) struct Band {
    pub n: usize,
    pub r0: f32,
    pub r1: f32,
    pub padding: f32,
}

impl Band {
    pub(crate) fn step(&self) -> f32 {
        (self.r1 - self.r0) / self.n.max(1) as f32
    }

    pub(crate) fn center(&self, i: usize) -> f32 {
        self.r0 + self.step() * (i as f32 + 0.5)
    }

    pub(crate) fn width(&self) -> f32 {
        (self.step() * (1.0 - self.padding)).max(1.0)
    }

    pub(crate) fn start(&self, i: usize) -> f32 {
        self.center(i) - self.width() / 2.0
    }
}
