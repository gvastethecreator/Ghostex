use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, Hsla, Image, IntoElement, ParentElement, Pixels, Styled, Window, div,
    img, px,
};

use super::{appearance::SidebarAppearance, model::NativeSidebarSession};
use crate::app::{consts::*, helpers::*};

#[derive(Clone)]
pub(crate) enum RowDragIdentity {
    Project {
        image: Option<Arc<Image>>,
        show_icon: bool,
    },
    Collection {
        color: Hsla,
        background: Hsla,
    },
    Session {
        session: Arc<NativeSidebarSession>,
    },
}

#[derive(Clone)]
pub(crate) struct RowDragPreview {
    pub(crate) identity: RowDragIdentity,
    pub(crate) appearance: SidebarAppearance,
    pub(crate) width: Pixels,
    pub(crate) pointer_x: Pixels,
    /// Where in the row the pointer grabbed it.
    pub(crate) grab: gpui::Point<Pixels>,
}

/// How far below the pointer a dragged session hangs, and how far right of it it starts.
///
/// CDXC:Sidebar 2026-10-01 WHY:
/// GPUI paints the dragged row after everything else, and a drop line is never more than about half a row (plus the gap between projects) from the pointer, so a dragged row drawn under the pointer covered the line every time; that is how a drop into Pinned looked like it showed no line. The dragged session hangs just below the pointer instead, clear of the line next to the pointer. A session's line can also land farther away (Sessions and Parked keep their Last Activity order), so a dragged session also starts right of the pointer and the line's left end stays visible wherever it is drawn.
///
/// CDXC:Sidebar 2026-10-06 DECISION:
/// User: "when i drag a project header it's showing below my cursor which is wrong". A dragged project or collection row sits under the pointer where it was grabbed, which supersedes the rule above for those rows; its fill is see-through, so the line beside the pointer still shows.
const HANG_BELOW_POINTER: f32 = 26.0;
const SESSION_RIGHT_OF_POINTER: f32 = 14.0;

impl RowDragPreview {
    pub(crate) fn render(&self, title: &str, window: &Window) -> AnyElement {
        let appearance = &self.appearance;
        let scale = appearance.scale;
        let session_backing = titlebar_background().blend(gpui::rgb(0xffffff).opacity(0.06).into());
        let row = div()
            .flex()
            .items_center()
            .w(self.width)
            .min_w_0()
            .font_family(crate::ui_fonts::UI_FONT)
            .font_weight(FontWeight::LIGHT)
            .text_size(px(15.55 * scale))
            .text_color(appearance.foreground);
        let row = match &self.identity {
            // The header box's own insets, so the icon and name stay where they were grabbed.
            RowDragIdentity::Project { image, show_icon } => row
                .h(px(30.0 * scale))
                .pl(px((5.0
                    + super::project_header::PROJECT_HEADER_CHEVRON_GUTTER)
                    * scale))
                .pr(px(6.0 * scale))
                .gap(px(10.0 * scale))
                .rounded(px(5.0 * scale))
                .bg(appearance.hover)
                .when(*show_icon, |row| {
                    row.child(match image {
                        Some(image) => img(image.clone())
                            .size(px(16.0 * scale))
                            .flex_shrink_0()
                            .into_any_element(),
                        None => titlebar_svg_icon(
                            TITLEBAR_ICON_FOLDER_OPEN,
                            16.0 * scale,
                            appearance.muted,
                        )
                        .into_any_element(),
                    })
                }),
            RowDragIdentity::Collection { color, background } => row
                .h(px(30.0 * scale))
                .px(px(8.0 * scale))
                .gap(px(5.0 * scale))
                .border_l_2()
                .border_color(*color)
                .bg(*background),
            RowDragIdentity::Session { session } => row
                .h(px(34.0 * scale))
                .pl(px(5.0 * scale))
                .pr(px(6.0 * scale))
                .gap(px(6.0 * scale))
                .rounded(px(5.0 * scale))
                .bg(session_backing)
                .opacity(0.95)
                .when(session.is_focused, |row| {
                    row.bg(session_backing.blend(appearance.session_selected))
                })
                .when(session.is_visible && !session.is_focused, |row| {
                    row.bg(session_backing.blend(appearance.visible))
                })
                .when(session.is_visible || session.is_focused, |row| {
                    row.text_color(chrome_color(0xd8d8d8, 0x292929))
                })
                .child(
                    div()
                        .size(px(15.0 * scale))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(super::icons::session_drag_icon(session, appearance)),
                ),
        };
        // GPUI draws the preview with its origin at the pointer minus the grab offset, so a row with
        // no padding above it sits exactly where it was grabbed.
        let lock_x = !matches!(self.identity, RowDragIdentity::Session { .. });
        div()
            .relative()
            .when(!lock_x, |wrapper| {
                wrapper
                    .pt(self.grab.y + px(HANG_BELOW_POINTER * scale))
                    .pl(self.grab.x + px(SESSION_RIGHT_OF_POINTER * scale))
            })
            .when(lock_x, |wrapper| {
                wrapper.left(self.pointer_x - window.mouse_position().x)
            })
            .child(row.child(div().flex_1().min_w_0().truncate().child(title.to_owned())))
            .into_any_element()
    }
}
