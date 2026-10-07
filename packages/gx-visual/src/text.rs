//! Text measuring without font access, truncation, wrapping and number formatting.
//!
//! Every client lays the scene out with these estimates, so the GPUI painter, the phone's SVG and
//! the CLI agree on where a label ends even though each draws with its own font.

/// Estimated width of `text` in logical pixels at `size` and `weight`.
pub fn text_width(text: &str, size: f32, weight: u16) -> f32 {
    let em: f32 = text.chars().map(advance).sum();
    let width = em * size;
    if weight >= 600 {
        width * 1.05
    } else {
        width
    }
}

fn advance(c: char) -> f32 {
    match c {
        '.' | ',' | ':' | ';' | '\'' | '|' | '!' | 'i' | 'l' | '1' => 0.28,
        '-' | '_' => 0.36,
        'm' | 'w' | 'M' | 'W' => 0.82,
        ' ' | '\t' => 0.28,
        '0'..='9' => 0.57,
        'A'..='Z' => 0.64,
        'a'..='z' => 0.53,
        c if c.is_ascii_punctuation() => 0.4,
        c if c.is_ascii() => 0.0,
        c if is_wide(c) => 1.0,
        _ => 0.6,
    }
}

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD)
}

/// Line height for text of `size`.
pub(crate) fn line_height(size: f32) -> f32 {
    size * 1.3
}

const ELLIPSIS: &str = "…";

/// `text` cut with "…" so it fits `max_width`; empty when not even the ellipsis fits.
pub(crate) fn truncate(text: &str, max_width: f32, size: f32, weight: u16) -> String {
    // The tolerance absorbs float rounding when `max_width` was derived from this text's own width.
    let max_width = max_width + 0.01;
    if text_width(text, size, weight) <= max_width {
        return text.to_string();
    }
    let ellipsis = text_width(ELLIPSIS, size, weight);
    if ellipsis > max_width {
        return String::new();
    }
    let scale = if weight >= 600 { size * 1.05 } else { size };
    let mut used = ellipsis;
    let mut end = 0;
    for (i, c) in text.char_indices() {
        let w = advance(c) * scale;
        if used + w > max_width {
            break;
        }
        used += w;
        end = i + c.len_utf8();
    }
    let mut out = text[..end].trim_end().to_string();
    out.push_str(ELLIPSIS);
    out
}

/// Greedy word wrap to `max_width`; `\n` forces a break and a word wider than a line is split.
pub(crate) fn wrap(text: &str, max_width: f32, size: f32, weight: u16) -> Vec<String> {
    let max_width = max_width.max(size);
    let space = text_width(" ", size, weight);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let paragraph = paragraph.trim_end_matches('\r');
        let mut line = String::new();
        let mut line_width = 0.0;
        for word in paragraph.split_whitespace() {
            let word_width = text_width(word, size, weight);
            let gap = if line.is_empty() { 0.0 } else { space };
            if line_width + gap + word_width <= max_width {
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
                line_width += gap + word_width;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                line_width = 0.0;
            }
            if word_width <= max_width {
                line.push_str(word);
                line_width = word_width;
                continue;
            }
            for c in word.chars() {
                let w = text_width(c.encode_utf8(&mut [0; 4]), size, weight);
                if line_width + w > max_width && !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    line_width = 0.0;
                }
                line.push(c);
                line_width += w;
            }
        }
        lines.push(line);
    }
    lines
}

/// Trims trailing zeros (and a trailing point) off a fixed-point number.
fn trim_fraction(text: String) -> String {
    let trimmed = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    };
    if trimmed == "-0" {
        "0".to_string()
    } else {
        trimmed
    }
}

fn group_thousands(fixed: &str) -> String {
    let (sign, rest) = match fixed.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", fixed),
    };
    let (int, frac) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };
    let digits = int.as_bytes();
    let mut out = String::with_capacity(fixed.len() + digits.len() / 3 + 1);
    out.push_str(sign);
    for (i, d) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*d as char);
    }
    if let Some(frac) = frac {
        out.push('.');
        out.push_str(frac);
    }
    out
}

/// A value for tooltips, tiles and tables: thousands separators and up to 4 decimals.
pub(crate) fn format_number(v: f64) -> String {
    if !v.is_finite() {
        return "–".to_string();
    }
    if v.abs() >= 1e15 {
        return format!("{v:.3e}");
    }
    group_thousands(&trim_fraction(format!("{v:.4}")))
}

/// A number written as a category label: no separators, up to 4 decimals.
pub(crate) fn format_plain(v: f64) -> String {
    if !v.is_finite() {
        return "–".to_string();
    }
    if v.abs() >= 1e15 {
        return format!("{v}");
    }
    trim_fraction(format!("{v:.4}"))
}

/// Decimals needed to write multiples of `step` exactly (at most 6).
fn decimals_for(step: f64) -> usize {
    let step = step.abs();
    if !step.is_finite() || step == 0.0 {
        return 0;
    }
    for d in 0..=6 {
        let scaled = step * 10f64.powi(d as i32);
        if (scaled - scaled.round()).abs() < 1e-6 * scaled.max(1.0) {
            return d;
        }
    }
    6
}

/// A compact axis tick: 1.2k, 3.4M, 1.2B, decimals from the tick step.
pub(crate) fn format_tick(v: f64, step: f64) -> String {
    if !v.is_finite() {
        return String::new();
    }
    let abs = v.abs();
    let (div, unit) = if abs >= 1e12 {
        (1e12, "T")
    } else if abs >= 1e9 {
        (1e9, "B")
    } else if abs >= 1e6 {
        (1e6, "M")
    } else if abs >= 1e3 {
        (1e3, "k")
    } else {
        (1.0, "")
    };
    let decimals = if unit.is_empty() {
        decimals_for(step)
    } else {
        decimals_for(step / div).min(2)
    };
    let mut text = trim_fraction(format!("{:.*}", decimals, v / div));
    text.push_str(unit);
    text
}

/// A fraction (0..1) as a percentage with up to one decimal.
pub(crate) fn format_percent(fraction: f64) -> String {
    if !fraction.is_finite() {
        return "–".to_string();
    }
    let mut text = trim_fraction(format!("{:.1}", fraction * 100.0));
    text.push('%');
    text
}

/// An axis tick on a 0..1 percentage axis.
pub(crate) fn format_percent_tick(fraction: f64, step: f64) -> String {
    let decimals = decimals_for(step * 100.0).min(2);
    let mut text = trim_fraction(format!("{:.*}", decimals, fraction * 100.0));
    text.push('%');
    text
}
