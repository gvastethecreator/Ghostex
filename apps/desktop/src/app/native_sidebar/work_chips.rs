//! A work-mode session card's second line: the PR, the Linear or GitHub issue and the Linear
//! project the session is linked to, each a small chip the user can click.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/work.rs (what the chips read),
//! server/src/work_mode/ (where the links and their states come from),
//! apps/desktop/src/app/gx_store/work_mode.rs (`openWorkLink`, what a click does).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use serde_json::json;

use super::appearance::SidebarAppearance;
use super::model::NativeSidebarSession;
use super::tooltips::SidebarTooltipSpan;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

const OPEN_GREEN: u32 = 0x3fb950;
const DRAFT_GREY: u32 = 0x8b949e;
const MERGED_PURPLE: u32 = 0xa371f7;
const CLOSED_RED: u32 = 0xf85149;
const PENDING_AMBER: u32 = 0xd29922;
const STARTED_YELLOW: u32 = 0xf2c94c;
const REVIEW_GREEN: u32 = 0x4cb782;
const DONE_INDIGO: u32 = 0x5e6ad2;
const LINEAR_INDIGO: u32 = 0x7c84f0;

/// The height of the chip line under line 1; `WORK_SESSION_HEIGHT` leaves 2px under it.
const CHIP_LINE_HEIGHT: f32 = 16.0;

/// One chip's parts, before it is drawn.
struct Chip {
    key: &'static str,
    icon: &'static str,
    icon_color: Hsla,
    label: String,
    /// The PR's checks mark, drawn after the number.
    trailing: Option<(&'static str, Hsla)>,
    url: Option<String>,
    tooltip: String,
}

impl GhostexGpuiApp {
    /// CDXC:WorkMode 2026-10-09 DECISION:
    /// User: in work mode a session linked to a PR, issue or Linear project gets a second line of borderless chips (PR with state and checks, issue with status); hovering one underlines it and clicking it opens it; sessions with no links stay one line.
    pub(super) fn render_native_session_work_chips(
        &self,
        session: &NativeSidebarSession,
        indent: f32,
        tooltip_span: SidebarTooltipSpan,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let work = session.work.as_ref()?;
        let scale = appearance.scale;
        let mut chips: Vec<Chip> = Vec::new();
        if let Some(pr) = &work.pull_request {
            let (icon, color, state_label) = match pr.state.as_str() {
                "draft" => ("titlebar/git-pull-request.svg", DRAFT_GREY, "Draft"),
                "merged" => ("titlebar/git-merge.svg", MERGED_PURPLE, "Merged"),
                "closed" => ("titlebar/git-pull-request.svg", CLOSED_RED, "Closed"),
                _ => ("titlebar/git-pull-request.svg", OPEN_GREEN, "Open"),
            };
            let checks = match pr.checks.as_deref() {
                Some("passing") => {
                    Some(("titlebar/circle-check.svg", OPEN_GREEN, "checks passing"))
                }
                Some("failing") => Some(("titlebar/circle-x.svg", CLOSED_RED, "checks failing")),
                Some("pending") => Some(("titlebar/loader2.svg", PENDING_AMBER, "checks running")),
                _ => None,
            };
            chips.push(Chip {
                key: "pr",
                icon,
                icon_color: rgb(color).into(),
                label: format!("#{}", pr.number),
                trailing: checks.map(|(icon, color, _)| (icon, rgb(color).into())),
                url: pr.url.clone(),
                tooltip: match checks {
                    Some((_, _, checks)) => format!("PR #{} · {state_label} · {checks}", pr.number),
                    None => format!("PR #{} · {state_label}", pr.number),
                },
            });
        }
        if let Some(first) = work.linear_issues.first() {
            let (icon, color) =
                linear_glyph(first.state_type.as_deref(), first.state_name.as_deref());
            let extra = work.linear_issues.len() - 1;
            chips.push(Chip {
                key: "linear",
                icon,
                icon_color: rgb(color).into(),
                label: if extra > 0 {
                    format!("{} +{extra}", first.identifier)
                } else {
                    first.identifier.clone()
                },
                trailing: None,
                url: first.url.clone(),
                tooltip: work
                    .linear_issues
                    .iter()
                    .map(|issue| {
                        [
                            Some(issue.identifier.as_str()),
                            issue.title.as_deref(),
                            issue.state_name.as_deref(),
                        ]
                        .into_iter()
                        .flatten()
                        .filter(|part| !part.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ")
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            });
        }
        if let Some(first) = work.github_issues.first() {
            let open = first.state.as_deref() != Some("closed");
            let extra = work.github_issues.len() - 1;
            chips.push(Chip {
                key: "issue",
                icon: if open {
                    "titlebar/circle-dot.svg"
                } else {
                    "titlebar/circle-check.svg"
                },
                icon_color: rgb(if open { OPEN_GREEN } else { MERGED_PURPLE }).into(),
                label: if extra > 0 {
                    format!("#{} +{extra}", first.number)
                } else {
                    format!("#{}", first.number)
                },
                trailing: None,
                url: first.url.clone(),
                tooltip: work
                    .github_issues
                    .iter()
                    .map(|issue| {
                        let state = if issue.state.as_deref() == Some("closed") {
                            "Closed"
                        } else {
                            "Open"
                        };
                        match issue
                            .title
                            .as_deref()
                            .filter(|title| !title.trim().is_empty())
                        {
                            Some(title) => format!("Issue #{} · {title} · {state}", issue.number),
                            None => format!("Issue #{} · {state}", issue.number),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            });
        }
        if let Some(project) = &work.linear_project {
            chips.push(Chip {
                key: "linear-project",
                icon: "titlebar/box.svg",
                icon_color: rgb(LINEAR_INDIGO).into(),
                label: project.name.clone(),
                trailing: None,
                url: project.url.clone(),
                tooltip: format!("Linear project: {}", project.name),
            });
        }
        if chips.is_empty() {
            return None;
        }
        let show_tooltips = self.native_sidebar.pointer_inside
            && self.native_sidebar.menu.is_none()
            && !cx.has_active_drag();
        let session_id = session.session_id.clone();
        Some(
            h_flex()
                .h(px(CHIP_LINE_HEIGHT * scale))
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                // Under the title: the row's indent, the 15px icon and the 6px gap after it.
                .pl(px((indent + 15.0 + 6.0) * scale))
                .pr(px(6.0 * scale))
                .gap(px(10.0 * scale))
                .children(chips.into_iter().map(|chip| {
                    render_chip(
                        &session_id,
                        chip,
                        tooltip_span,
                        show_tooltips,
                        appearance,
                        cx,
                    )
                }))
                .into_any_element(),
        )
    }
}

fn render_chip(
    session_id: &str,
    chip: Chip,
    tooltip_span: SidebarTooltipSpan,
    show_tooltip: bool,
    appearance: &SidebarAppearance,
    cx: &mut gpui::Context<GhostexGpuiApp>,
) -> AnyElement {
    let scale = appearance.scale;
    let muted = appearance.muted;
    let foreground = appearance.foreground;
    let tooltip = chip.tooltip.clone();
    let url = chip.url.clone();
    div()
        .id(format!("native-session-work-{}-{session_id}", chip.key))
        .role(gpui::Role::Link)
        .aria_label(chip.tooltip.clone())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(3.0 * scale))
        .text_size(px(11.5 * scale))
        .text_color(muted)
        .child(titlebar_svg_icon(chip.icon, 12.0 * scale, chip.icon_color))
        .child(div().whitespace_nowrap().child(chip.label))
        .when_some(chip.trailing, |chip, (icon, color)| {
            chip.child(titlebar_svg_icon(icon, 12.0 * scale, color))
        })
        .when_some(url, |chip, url| {
            chip.cursor_pointer()
                .hover(move |style| {
                    style
                        .text_color(foreground)
                        .text_decoration_1()
                        .text_decoration_color(foreground)
                })
                // Its own click: the row's selection and drag never start from a chip.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    app.dispatch_native_sidebar_ui(json!({"type": "openWorkLink", "url": url}), cx);
                }))
        })
        .when(show_tooltip, |chip| {
            chip.managed_discrete_tooltip_with_placement(
                tooltip_span.placement(),
                appearance.tooltip_delay,
                move |window, cx| {
                    super::tooltips::sidebar_tooltip(
                        tooltip.clone(),
                        tooltip_span,
                        scale,
                        window,
                        cx,
                    )
                },
            )
        })
        .into_any_element()
}

/// Linear's status ring for a workflow state: empty or dashed before work starts, half while it
/// runs, three-quarters in review, a filled check when done, a cross when cancelled.
fn linear_glyph(state_type: Option<&str>, state_name: Option<&str>) -> (&'static str, u32) {
    let in_review = state_name.is_some_and(|name| name.to_ascii_lowercase().contains("review"));
    match state_type {
        Some("started") if in_review => ("titlebar/work-issue-review.svg", REVIEW_GREEN),
        Some("started") => ("titlebar/work-issue-started.svg", STARTED_YELLOW),
        Some("completed") => ("titlebar/circle-check-filled.svg", DONE_INDIGO),
        Some("canceled") => ("titlebar/circle-x.svg", DRAFT_GREY),
        Some("backlog") | Some("triage") => ("titlebar/circle-dashed.svg", DRAFT_GREY),
        Some(_) => ("titlebar/circle.svg", DRAFT_GREY),
        // Linear has not answered yet (no API key, or not fetched): the plain ring, in Linear's
        // colour so the chip still reads as a Linear issue.
        None => ("titlebar/circle.svg", LINEAR_INDIGO),
    }
}
