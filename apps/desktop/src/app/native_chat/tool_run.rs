//! The run of tool rows under a message: a per-tool glyph, the one-line
//! preview, the "+N previous tool calls" fold, failed results in the error
//! tone, and the subagent heading. Which rows a collapsed run keeps and what
//! the fold's labels say are decided in
//! `packages/gx-chat-core/src/transcript/tool_rows.rs`; this file only lays them out.

use super::disclosure_body::{DisclosureRail, disclosure_body};
use super::disclosure_motion::motion_clip_trailing;
use super::{
    appearance::ChatAppearance, fonts::CHAT_MONO, state::NativeChatView, transcript::text,
};
use crate::app::native_chat::cursor::ChatCursor as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled as _, StyledText, div,
    px,
};
use gpui_component::text::{TextView, TextViewStyle};
use serde_json::Value;

/// The spacing between the rows of a run, kept the same inside an expansion as
/// it is outside one so opening a group never re-flows the rows it reveals.
const ROW_GAP: f32 = 8.0;

/// The bundled icon for each glyph `sessionChatToolGlyph` classifies a tool into.
fn glyph_icon(glyph: &str) -> &'static str {
    match glyph {
        "edit" => "titlebar/pencil.svg",
        "file" => "titlebar/file-text.svg",
        "terminal" => "titlebar/terminal-2.svg",
        "web" => "titlebar/world.svg",
        _ => "titlebar/tool.svg",
    }
}

impl NativeChatView {
    pub(super) fn tool_rows(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let tools = message["tools"].as_array().cloned().unwrap_or_default();
        if tools.is_empty() {
            return Vec::new();
        }
        let id = text(message, "id");
        // The run already sits under a heading that collapses it (a reasoning row, an assistant
        // commentary heading), so it shows every row and adds no group of its own: React passed
        // `showAllRows` at exactly those call sites, and the core carries the rule in tool_rows.rs.
        let show_all = message["toolsShowAllRows"] == true;
        // An answered question renders as its own exchange card, and a message sent to another
        // agent as its own message card. Each stays a plain row only where its card already shows
        // somewhere else, which was React's `questionPairsAsRows` (now the core's
        // questions/hoisting.rs).
        let cards_as_rows = show_all || self.in_work_fold;
        let visible: Vec<usize> = (0..tools.len())
            .filter(|index| {
                cards_as_rows
                    || (tools[*index]["exchange"] != true && tools[*index]["sentMessage"] != true)
            })
            .collect();
        if visible.is_empty() {
            return Vec::new();
        }
        if p.simple && !show_all {
            let key = format!("tools:{id}");
            // The run disclosure opens on demand and is never opened by verbose mode, as React's was.
            let expanded = self.expanded.contains(&key);
            let motion = self.disclosure_frame(&key, expanded, cx);
            let mut rows = vec![self.disclosure(
                key.clone(),
                text(message, "simpleToolLabel"),
                expanded,
                None,
                p,
                cx,
            )];
            if expanded || motion.is_some() {
                let body = self.tool_row_list(&id, &tools, &visible, p, cx);
                let body = disclosure_body(
                    p,
                    DisclosureRail::Marker,
                    ROW_GAP,
                    key.clone(),
                    "Collapse tool calls",
                    body,
                    cx,
                );
                rows.push(self.disclosure_body_motion(&key, motion, ROW_GAP * p.scale, body));
            }
            return rows;
        }
        let fold = message["toolFold"].clone();
        let hidden = fold["hiddenCount"].as_u64().unwrap_or(0);
        if show_all || hidden == 0 {
            return self.tool_row_list(&id, &tools, &visible, p, cx);
        }
        let run_key = format!("tool-run:{id}");
        let run_expanded = self.expanded.contains(&run_key);
        let label = text(
            &fold,
            if run_expanded {
                "expandedLabel"
            } else {
                "collapsedLabel"
            },
        );
        // While the group opens or closes, the rows it hides ease in above the ones it always
        // shows, on the rail the open group has; the rail itself is the one part that appears whole.
        if let Some(frame) = self.disclosure_frame(&run_key, run_expanded, cx) {
            let (kept, hidden): (Vec<usize>, Vec<usize>) = visible
                .iter()
                .copied()
                .partition(|index| fold["visible"][*index] == true);
            let mut body: Vec<AnyElement> = Vec::new();
            let hidden_rows = self.tool_row_list(&id, &tools, &hidden, p, cx);
            if !hidden_rows.is_empty() {
                body.push(motion_clip_trailing(
                    self.disclosure_height(&run_key),
                    frame,
                    ROW_GAP * p.scale,
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .gap(px(ROW_GAP * p.scale))
                        .children(hidden_rows)
                        .into_any_element(),
                ));
            }
            body.extend(self.tool_row_list(&id, &tools, &kept, p, cx));
            body.push(self.tool_fold_toggle(run_key.clone(), label, run_expanded, p, cx));
            return vec![disclosure_body(
                p,
                DisclosureRail::Marker,
                ROW_GAP,
                run_key,
                "Show fewer tool calls",
                body,
                cx,
            )];
        }
        if run_expanded {
            let mut body = self.tool_row_list(&id, &tools, &visible, p, cx);
            body.push(self.tool_fold_toggle(run_key.clone(), label, true, p, cx));
            return vec![disclosure_body(
                p,
                DisclosureRail::Marker,
                ROW_GAP,
                run_key,
                "Show fewer tool calls",
                body,
                cx,
            )];
        }
        let kept: Vec<usize> = visible
            .into_iter()
            .filter(|index| fold["visible"][*index] == true)
            .collect();
        let mut rows = self.tool_row_list(&id, &tools, &kept, p, cx);
        rows.push(self.tool_fold_toggle(run_key, label, false, p, cx));
        rows
    }

    /// A `!` command the user ran (`shellCommand`, gx-chat-core's `fold_shell_commands`): its one
    /// tool row, opening onto the Command and Result blocks, in the user's bubble. The command
    /// shows in simple mode too, since it is what the user typed.
    pub(super) fn shell_command_card(
        &mut self,
        message: &Value,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let id = text(message, "id");
        let tools = message["tools"].as_array().cloned().unwrap_or_default();
        let mut appearance = p.clone();
        appearance.simple = false;
        // While the command runs, its output streams in from the terminal and the card stays open on it.
        let live = message["shellCommandLive"] == true;
        let rows: Vec<AnyElement> = tools
            .iter()
            .enumerate()
            .map(|(index, tool)| self.tool_row(&id, index, tool, live, &appearance, cx))
            .collect();
        div()
            .flex()
            .flex_col()
            .items_end()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(ROW_GAP * s))
                    .max_w(gpui::relative(0.8))
                    .min_w_0()
                    .rounded(px(16.0 * s))
                    .px(px(12.0 * s))
                    .py(px(8.0 * s))
                    .bg(p.input)
                    .children(rows),
            )
            .into_any_element()
    }

    /// The tool's name and its mono preview as ONE line of text, two font runs.
    ///
    /// CDXC:SessionChat 2026-10-07 WHY: drawn as two text elements side by side on a centred row, the JetBrains Mono preview sat a pixel below the prose name ("Shell echo …"), at every row position and whatever the font metrics said, because each element places and snaps its own baseline. Runs of one line share one baseline, so the name and the command line up on every platform; the en space stands in for the old 6px gap.
    fn name_and_preview(
        name: String,
        preview: String,
        heading: gpui::Hsla,
        p: &ChatAppearance,
    ) -> StyledText {
        let gap = "\u{2002}";
        let run = |len: usize, family: String, color: gpui::Hsla| gpui::TextRun {
            len,
            font: gpui::font(family),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = vec![
            run(name.len() + gap.len(), p.font.clone(), heading),
            run(preview.len(), CHAT_MONO.to_string(), p.muted),
        ];
        StyledText::new(format!("{name}{gap}{preview}")).with_runs(runs)
    }

    fn tool_row_list(
        &mut self,
        id: &str,
        tools: &[Value],
        indices: &[usize],
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        indices
            .iter()
            .map(|index| self.tool_row(id, *index, &tools[*index], false, p, cx))
            .collect()
    }

    fn tool_fold_toggle(
        &self,
        key: String,
        label: String,
        expanded: bool,
        p: &ChatAppearance,
        cx: &Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        div()
            .id(key.clone())
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .gap(px(6.0 * s))
            .rounded(px(4.0 * s))
            .text_color(p.muted)
            .chat_cursor_pointer()
            .hover(|style| style.bg(p.border.opacity(0.4)))
            .child(
                div()
                    .w(px(16.0 * s))
                    .ml(px(2.0 * s))
                    .h(px(22.75 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        gpui::svg()
                            .path(if expanded {
                                "titlebar/chevron-down.svg"
                            } else {
                                "titlebar/chevron-right.svg"
                            })
                            .size(px(14.0 * s))
                            .text_color(p.muted),
                    ),
            )
            .child(label)
            .on_click(cx.listener(move |view, _, _, cx| view.toggle_disclosure(&key, cx)))
            .into_any_element()
    }

    fn tool_row(
        &mut self,
        message_id: &str,
        index: usize,
        tool: &Value,
        always_open: bool,
        p: &ChatAppearance,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = p.scale;
        let key = format!("tool:{message_id}:{index}");
        let expanded = always_open || self.expanded.contains(&key);
        let has_detail = tool["hasDetail"] == true;
        let motion = self.disclosure_frame(&key, expanded && has_detail, cx);
        let failed = tool["failed"] == true;
        let name = text(tool, "name");
        let preview = text(tool, "preview");
        let subagent = text(&tool["subagent"], "name");
        let heading = if failed { p.error() } else { p.primary };
        let toggle_key = key.clone();
        let a11y_label = match (subagent.is_empty(), preview.is_empty()) {
            (true, true) => format!("Tool {name}"),
            (true, false) => format!("Tool {name}: {preview}"),
            (false, _) => format!("Tool {name} ({subagent}): {preview}"),
        };
        let mut trigger = div()
            .id(key.clone())
            .role(gpui::Role::Button)
            .aria_label(a11y_label)
            .aria_expanded(expanded)
            .when(failed, |this| this.aria_description("failed"))
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(6.0 * s))
            .rounded(px(4.0 * s))
            .when(has_detail, |this| {
                this.chat_cursor_pointer()
                    .hover(|style| style.bg(p.border.opacity(0.4)))
                    .on_click(
                        cx.listener(move |view, _, _, cx| view.toggle_disclosure(&toggle_key, cx)),
                    )
            })
            .child(
                div()
                    .w(px(16.0 * s))
                    .ml(px(2.0 * s))
                    .h(px(22.75 * s))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .child(
                        gpui::svg()
                            .path(glyph_icon(tool["glyph"].as_str().unwrap_or("tool")))
                            .size(px(14.0 * s))
                            .text_color(p.muted),
                    ),
            );
        // The preview is the tool's argument; a subagent row shows the agent instead, and simple mode shows neither.
        // It takes only the width it needs and shrinks with an ellipsis when it cannot have that,
        // so the chevron follows the text instead of being pushed to the pane's right edge
        // (React's `.ghostex-chat-work-preview`, which never grows either).
        trigger = if !preview.is_empty() && subagent.is_empty() && !p.simple {
            trigger.child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(Self::name_and_preview(name, preview, heading, p)),
            )
        } else {
            trigger.child(div().flex_shrink_0().text_color(heading).child(name))
        };
        if has_detail {
            trigger = trigger.child(
                gpui::svg()
                    .path(if expanded {
                        "titlebar/chevron-down.svg"
                    } else {
                        "titlebar/chevron-right.svg"
                    })
                    .size(px(12.0 * s))
                    .flex_shrink_0()
                    .text_color(p.muted),
            );
        }
        let mut row = div().flex().flex_col().min_w_0().w_full().gap(px(4.0 * s));
        row = row.child(if subagent.is_empty() {
            trigger.into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .min_w_0()
                .gap(px(8.0 * s))
                .child(trigger)
                // The chip opens that child's transcript, like React's `SessionChatSubagentLink`;
                // a selector pointing back at this conversation stays plain text.
                .child(match Self::subagent_open_command(&tool["subagent"]) {
                    Some(command) => self.subagent_link(
                        format!("subagent:{key}"),
                        subagent.clone(),
                        Some(
                            tool["subagent"]["agentType"]
                                .as_str()
                                .unwrap_or(&subagent)
                                .to_string(),
                        ),
                        command,
                        p,
                        cx,
                    ),
                    None => div()
                        .min_w_0()
                        .truncate()
                        .text_color(p.control_primary)
                        .child(subagent)
                        .into_any_element(),
                })
                .into_any_element()
        });
        if (expanded || motion.is_some()) && has_detail {
            let detail = self.row_detail(&key, "tool", message_id, index as u64);
            let input = text(&detail, "input");
            let output = text(&detail, "output");
            let has_call = tool["hasCall"] == true;
            let command = tool["glyph"] == "terminal";
            // The first block's label row carries the copy button, which copies the whole call.
            let mut copy = Some(text(&detail, "copyText")).filter(|copy| !copy.is_empty());
            let mut detail: Vec<AnyElement> = Vec::new();
            if !input.is_empty() {
                let label = if command {
                    Some("Command")
                } else if !output.is_empty() {
                    Some("Input")
                } else {
                    None
                };
                detail.push(self.tool_body(
                    format!("input:{key}"),
                    label,
                    input,
                    false,
                    copy.take(),
                    p,
                ));
            }
            if !output.is_empty() {
                let label = has_call.then_some("Result");
                detail.push(self.tool_body(
                    format!("output:{key}"),
                    label,
                    output,
                    failed,
                    copy.take(),
                    p,
                ));
            }
            if !detail.is_empty() {
                let body = disclosure_body(
                    p,
                    DisclosureRail::ToolDetail,
                    8.0,
                    key.clone(),
                    format!("Collapse {}", tool["name"].as_str().unwrap_or_default()),
                    detail,
                    cx,
                );
                row = row.child(self.disclosure_body_motion(&key, motion, 4.0 * s, body));
            }
        }
        row.into_any_element()
    }

    /// One labelled block of a tool's detail: its arguments, the command it ran,
    /// or what it reported back. React painted it as a plain monospaced `<pre>`
    /// (`.ghostex-chat-tool-body`), so this is verbatim text in a scroll-capped
    /// box, never a Markdown code block card: that would put a language header on
    /// output the agent never wrote as code. `copy` is the whole call's text for
    /// the copy button on the first block's label row.
    ///
    /// CDXC:SessionChat 2026-10-05 DECISION: "please let me select text here and copy". The text is a selectable `TextView` (one headerless fenced block, so it stays verbatim), the selection mechanism the transcript's prose already uses, so a drag selects it and Cmd/Ctrl+C or the transcript menu copies it.
    fn tool_body(
        &self,
        key: String,
        label: Option<&str>,
        content: String,
        failed: bool,
        copy: Option<String>,
        p: &ChatAppearance,
    ) -> AnyElement {
        let s = p.scale;
        let color = if failed { p.error() } else { p.muted };
        let header = (label.is_some() || copy.is_some()).then(|| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0 * s))
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.25 * s))
                        .text_color(p.muted)
                        .child(label.unwrap_or_default().to_string()),
                )
                .when_some(copy, |this, copy| {
                    this.child(super::code_block::action(
                        SharedString::from(format!("copy:{key}")),
                        "titlebar/copy.svg",
                        p,
                        move |cx| {
                            crate::app::helpers::gpui_copy_to_clipboard(
                                ClipboardItem::new_string(copy.clone()),
                                cx,
                            )
                        },
                    ))
                })
        });
        let mut style = TextViewStyle::default().default_cursor(true);
        // The box below is the frame; the block inside it adds no card of its own.
        style.code_block = StyleRefinement::default()
            .p(px(0.0))
            .bg(gpui::transparent_black())
            .font_family(CHAT_MONO)
            .text_size(px(12.6 * s))
            .line_height(px(20.5 * s))
            .text_color(color);
        let text_view = TextView::markdown(
            SharedString::from(format!("{key}:text")),
            verbatim_fence(&content),
        )
        .selectable(true)
        .style(style)
        .min_w_0()
        .text_color(color);
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(4.0 * s))
            .children(header)
            .child(
                self.nested_scroll(
                    key,
                    div()
                        .min_w_0()
                        // React capped the block at 16.25 of its own lines plus its padding.
                        .max_h(px(220.75 * s))
                        .px(px(10.0 * s))
                        .py(px(8.0 * s))
                        .bg(p.input)
                        .rounded(px(6.0 * s))
                        .font_family(CHAT_MONO)
                        .text_size(px(12.6 * s))
                        .line_height(px(20.5 * s))
                        .text_color(color)
                        .child(text_view),
                ),
            )
            .into_any_element()
    }
}

/// `text` as one fenced Markdown block, so it renders and copies verbatim: the fence is longer
/// than any backtick run inside it, which no line of the text can then close.
fn verbatim_fence(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    let newline = if text.ends_with('\n') { "" } else { "\n" };
    format!("{fence}\n{text}{newline}{fence}\n")
}
