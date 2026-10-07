use std::{cell::Cell, rc::Rc};

use gpui::{AppContext, Bounds, Context, ParentElement, StatefulInteractiveElement, Styled};

use super::drag::{SidebarDrag, SidebarDragPreview};
use crate::GhostexGpuiApp;

pub(super) trait SidebarDragSource:
    ParentElement + StatefulInteractiveElement + Styled + Sized
{
    fn sidebar_drag_source(self, drag: SidebarDrag, cx: &mut Context<GhostexGpuiApp>) -> Self {
        let bounds = Rc::new(Cell::new(Bounds::default()));
        let painted_bounds = bounds.clone();
        let view = cx.entity().downgrade();
        self.relative()
            .child(
                gpui::canvas(
                    move |bounds, _, _| painted_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .on_drag(drag, move |drag, _, window, cx| {
                let _ = view.update(cx, |app, cx| {
                    app.native_sidebar.dragging = Some((drag.kind, drag.id.clone()));
                    cx.notify();
                });
                let mut preview = drag.clone();
                match &mut preview.preview {
                    SidebarDragPreview::Space(space) => space.pointer_y = window.mouse_position().y,
                    SidebarDragPreview::Row(row) => {
                        row.width = bounds.get().size.width;
                        row.pointer_x = window.mouse_position().x;
                    }
                }
                cx.new(|_| preview)
            })
    }
}
impl<T: ParentElement + StatefulInteractiveElement + Styled> SidebarDragSource for T {}
