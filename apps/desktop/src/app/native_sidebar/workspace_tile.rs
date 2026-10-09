//! The workspace tile at the left end of the Spaces row: the window's workspace as its letter on
//! the workspace's color, with a small chevron badge; a click opens the workspace menu.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User: one tile to the left of the Spaces row (not a rail), then a hairline, then that
//! workspace's Spaces. The menu comes from gx-core (`sidebar_menu/workspace.rs`).

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px, rgb,
};
use gpui_component::tooltip::ManagedTooltipExt as _;

use super::appearance::SidebarAppearance;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;

/// The tile, the gap after it and its hairline: what the Space row gives up for it.
pub(super) const WORKSPACE_TILE_ROOM: f32 = 28.0 + 4.0 + 1.0 + 4.0;

impl GhostexGpuiApp {
    /// Whether the window draws the workspace tile: this computer's section is shown and its
    /// daemon has workspaces. With Spaces off the row is drawn for the tile alone only once there
    /// is more than one workspace, so an install that never made one looks as it did.
    pub(super) fn native_sidebar_shows_workspace_tile(
        &self,
        selected_machine_id: &str,
        spaces_enabled: bool,
    ) -> bool {
        selected_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID
            && self
                .gx_store_workspaces_state()
                .is_some_and(|state| spaces_enabled || state.workspaces.len() > 1)
    }

    pub(super) fn render_native_sidebar_workspace_tile(
        &self,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let tile = self.gx_store_workspace_tile()?;
        let scale = appearance.scale;
        let color = u32::from_str_radix(tile.color.trim_start_matches('#'), 16)
            .map(rgb)
            .map(gpui::Hsla::from)
            .unwrap_or(appearance.muted);
        let tooltip = tile.name.clone();
        let tooltip_span = super::tooltips::SidebarTooltipSpan::sidebar(self.sidebar_width, scale);
        Some(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(4.0 * scale))
                .child(
                    div()
                        .id("native-sidebar-workspace-tile")
                        .role(gpui::Role::Button)
                        .aria_label(format!("Workspace {}", tile.name))
                        .relative()
                        .size(px(28.0 * scale))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.0 * scale))
                        .bg(color)
                        .cursor_pointer()
                        .hover(|tile| tile.opacity(0.9))
                        .text_size(px(13.0 * scale))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(gpui::white())
                        .child(tile.letter.clone())
                        .child(
                            div()
                                .absolute()
                                .right(px(-3.0 * scale))
                                .bottom(px(-3.0 * scale))
                                .size(px(12.0 * scale))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(gpui::black().opacity(0.55))
                                .child(titlebar_svg_icon(
                                    "titlebar/chevron-down.svg",
                                    8.0 * scale,
                                    gpui::white(),
                                )),
                        )
                        .when(
                            self.native_sidebar.pointer_inside
                                && self.native_sidebar.menu.is_none()
                                && !cx.has_active_drag(),
                            |tile| {
                                tile.managed_discrete_tooltip_with_placement(
                                    tooltip_span.placement(),
                                    appearance.tooltip_delay,
                                    move |window, cx| {
                                        super::tooltips::sidebar_free_width_tooltip(
                                            tooltip.clone(),
                                            tooltip_span,
                                            scale,
                                            window,
                                            cx,
                                        )
                                    },
                                )
                            },
                        )
                        .on_click(cx.listener(move |app, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            let Some(menu) = app.gx_store_workspace_menu(cx) else {
                                return;
                            };
                            Self::show_native_sidebar_menu(
                                &menu,
                                event.position(),
                                scale,
                                window,
                                cx,
                            );
                        })),
                )
                .child(
                    div()
                        .w(px(1.0))
                        .h(px(16.0 * scale))
                        .bg(appearance.foreground.opacity(0.12)),
                )
                .into_any_element(),
        )
    }
}
