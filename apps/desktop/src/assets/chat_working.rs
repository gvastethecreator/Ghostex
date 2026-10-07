use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkingStripVisual {
    pub spark_path: String,
    pub min_height: f32,
    pub padding_x: f32,
    pub gap: f32,
    pub spark_box: f32,
    pub spark_size: f32,
    pub font_size: f32,
    pub pulse_ms: f32,
    pub spin_ms: f32,
    pub armed_icon_gap: f32,
    pub armed_column_gap: f32,
    pub armed_row_gap: f32,
    pub delayed_send_color: String,
    pub close_after_done_color: String,
}

impl WorkingStripVisual {
    /// The clock tint for an armed action id (`delayedSend` / `closeAfterDone`).
    pub(crate) fn armed_color(&self, id: &str) -> Option<gpui::Rgba> {
        let hex = match id {
            "delayedSend" => &self.delayed_send_color,
            "closeAfterDone" => &self.close_after_done_color,
            _ => return None,
        };
        u32::from_str_radix(hex.trim_start_matches('#'), 16)
            .ok()
            .map(gpui::rgb)
    }
}

pub(crate) static VISUAL: LazyLock<WorkingStripVisual> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../packages/gx-chat-core/visual/working-strip.json"
    ))
    .expect("shared working strip appearance")
});

/// CDXC:SessionChat 2026-10-05 DECISION: User: "the rotating working icon still shows the lines just as before … make sure to fix it" (supersedes the 2026-10-04 padding-only fix). The flickering square was each rotated sprite's quad edge (the 96 px glow quads most visibly) sampling the neighbouring atlas tiles, which padding inside the asset cannot stop; the GPUI monochrome sprite shaders now clamp samples inside the tile (`.dependencies/zed` gpui_windows `shaders.hlsl`, gpui_wgpu `shaders.wgsl`, gpui_apple `shaders.metal`). `SPARK_PAD` still keeps the glyph clear of its own tile edge, and `spark_svg_size` renders the padded svg larger by the same ratio so the glyph keeps its size.
pub(crate) const SPARK_PAD: f32 = 4.0;

/// The on-screen size of the padded spark svg whose glyph measures `glyph_px`.
pub(crate) fn spark_svg_size(glyph_px: f32) -> f32 {
    glyph_px * (24.0 + 2.0 * SPARK_PAD) / 24.0
}

/// Cache a bounded set of SVG blur masks; GPUI tints their alpha masks like CSS drop-shadow.
pub(crate) fn asset(key: &str) -> Option<String> {
    let path = &VISUAL.spark_path;
    if key == "spark" {
        let box_units = 24.0 + 2.0 * SPARK_PAD;
        return Some(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-{SPARK_PAD} -{SPARK_PAD} {box_units} {box_units}"><path d="{path}"/></svg>"#
        ));
    }
    let (color, step) = key.split_once('-')?;
    let step = step.parse::<u8>().ok().filter(|step| *step <= 32)?;
    let progress = f32::from(step) / 32.0;
    let blur = match color {
        "white" => 1.0 + 4.0 * progress,
        "blue" => 12.0 * progress,
        _ => return None,
    };
    let inset = (96.0 - VISUAL.spark_size) / 2.0;
    let scale = VISUAL.spark_size / 24.0;
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><filter id="blur" x="0" y="0" width="96" height="96" filterUnits="userSpaceOnUse"><feGaussianBlur stdDeviation="{blur}"/></filter></defs><g filter="url(#blur)"><path transform="translate({inset} {inset}) scale({scale})" d="{path}"/></g></svg>"#
    ))
}
