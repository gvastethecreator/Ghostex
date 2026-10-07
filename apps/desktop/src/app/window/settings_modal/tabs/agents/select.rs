//! The two dropdowns of the Agents page the field library does not draw:
//!
//! - the searchable select the shadcn `Select` becomes at 8+ items (CDXC:DesignSystem 2026-09-08
//!   DECISION in packages/components/ui/select.tsx (deleted 2026-10-01)): a chevron trigger and a popup under it with a
//!   search field on top (packages/components/ui/searchable-dropdown.css), used by Default Prompt
//!   Agent and the editor's Agent type (whose rows carry the agent logos,
//!   `AgentTypeSelectOption`);
//! - a plain select with a placeholder (the agent CLI's Installation method, which reads
//!   "Choose how this CLI was installed" before a method is known), laid over its trigger like
//!   Base UI's select and the field library's `settings_select`.
//!
//! Their widget state lives on the page (`AgentsTab::dropdowns`).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{CONTROL_HEIGHT, settings_icon, tooltip_text};
use super::super::super::palette::SettingsPalette;
use super::AgentsTab;
use super::icons;
use super::logos::agent_icon;
use gpui::Focusable as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, AppContext as _, Bounds, ClickEvent, Context, Entity,
    FocusHandle, InteractiveElement as _, IntoElement, KeyDownEvent, MouseDownEvent,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, anchored, deferred, div, point, px,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable as _, Size as ComponentSize, h_flex, v_flex};
use std::cell::Cell;
use std::rc::Rc;

/// One option; `icon` is an agent icon id (`custom` draws the code-dots glyph).
#[derive(Clone, Debug)]
pub(super) struct DropdownOption {
    pub(super) value: String,
    pub(super) label: String,
    pub(super) icon: Option<String>,
}

impl DropdownOption {
    pub(super) fn plain(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            icon: None,
        }
    }
}

pub(super) struct DropdownState {
    pub(super) open: bool,
    /// The highlighted row among the visible (filtered) rows.
    highlight: Option<usize>,
    trigger: Rc<Cell<Option<Bounds<Pixels>>>>,
    focus: FocusHandle,
    search: Option<Entity<InputState>>,
    _subscription: Option<Subscription>,
}

type ChangeHandler = Rc<dyn Fn(&mut AgentsTab, String, &mut Window, &mut Context<AgentsTab>)>;

impl AgentsTab {
    fn dropdown_state(
        &mut self,
        id: &SharedString,
        searchable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> &mut DropdownState {
        if !self.dropdowns.contains_key(id) {
            let (search, subscription) = if searchable {
                let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
                let key = id.clone();
                let subscription = cx.subscribe_in(
                    &input,
                    window,
                    move |page: &mut Self, _, event: &InputEvent, _window, cx| {
                        if matches!(event, InputEvent::Change)
                            && let Some(state) = page.dropdowns.get_mut(&key)
                        {
                            // `autoHighlight`: the first match is highlighted as the query changes.
                            state.highlight = Some(0);
                            cx.notify();
                        }
                    },
                );
                (Some(input), Some(subscription))
            } else {
                (None, None)
            };
            self.dropdowns.insert(
                id.clone(),
                DropdownState {
                    open: false,
                    highlight: None,
                    trigger: Rc::new(Cell::new(None)),
                    focus: cx.focus_handle().tab_stop(true),
                    search,
                    _subscription: subscription,
                },
            );
        }
        self.dropdowns.get_mut(id).expect("dropdown state")
    }

    /// Closes every open dropdown of the page (one is open at a time).
    pub(super) fn close_dropdowns(&mut self) {
        for state in self.dropdowns.values_mut() {
            state.open = false;
        }
    }

    fn toggle_dropdown(
        &mut self,
        id: &SharedString,
        selected: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_open = self.dropdowns.get(id).is_some_and(|state| state.open);
        self.close_dropdowns();
        self.fields.close_select();
        let state = self.dropdowns.get_mut(id).expect("dropdown state");
        state.open = !was_open;
        state.highlight = selected.or(Some(0));
        if state.open {
            if let Some(search) = state.search.clone() {
                search.update(cx, |search, cx| {
                    search.set_value("", window, cx);
                    search.focus(window, cx);
                });
            } else {
                state.focus.focus(window, cx);
            }
        } else {
            state.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn close_dropdown(&mut self, id: &SharedString, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.dropdowns.get_mut(id) {
            state.open = false;
            state.focus.focus(window, cx);
        }
        cx.notify();
    }

    /// A select: `searchable` draws the search popup, otherwise the plain one laid over the
    /// trigger. `width` is the trigger's (None fills the row); `placeholder` shows while no option
    /// matches `value`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn dropdown(
        &mut self,
        p: &SettingsPalette,
        id: impl Into<SharedString>,
        options: &[DropdownOption],
        value: Option<&str>,
        placeholder: &str,
        searchable: bool,
        width: Option<f32>,
        disabled: bool,
        disabled_reason: Option<SharedString>,
        on_change: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id: SharedString = id.into();
        let on_change: ChangeHandler = Rc::new(on_change);
        let pending_open = self.pending_open_dropdown.as_deref() == Some(id.as_ref());
        let selected =
            value.and_then(|value| options.iter().position(|option| option.value == value));
        let (open, focus, trigger_bounds, highlight, search) = {
            let state = self.dropdown_state(&id, searchable, window, cx);
            (
                state.open,
                state.focus.clone(),
                state.trigger.clone(),
                state.highlight,
                state.search.clone(),
            )
        };
        if pending_open {
            self.pending_open_dropdown = None;
            self.toggle_dropdown(&id, selected, window, cx);
        }
        let open = (open || pending_open) && !disabled;
        let focused = focus.is_focused(window);
        let p = *p;
        let query = search
            .as_ref()
            .map(|search| search.read(cx).value().to_string())
            .unwrap_or_default();
        // The rows the popup shows: every option, or the ones matching every query term.
        let visible: Vec<usize> = if searchable && !query.trim().is_empty() {
            let terms: Vec<String> = query
                .trim()
                .to_lowercase()
                .split_whitespace()
                .map(str::to_string)
                .collect();
            options
                .iter()
                .enumerate()
                .filter(|(_, option)| {
                    let text = format!("{} {}", option.value, option.label).to_lowercase();
                    terms.iter().all(|term| text.contains(term))
                })
                .map(|(index, _)| index)
                .collect()
        } else {
            (0..options.len()).collect()
        };
        let label_content: AnyElement = match selected {
            Some(index) => {
                let option = &options[index];
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(8.0))
                    .items_center()
                    .children(option.icon.as_deref().map(|icon| {
                        agent_icon(if icon == "custom" { None } else { Some(icon) }, &p)
                    }))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(option.label.clone()),
                    )
                    .into_any_element()
            }
            None => div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(hsla(p.muted))
                .child(placeholder.to_string())
                .into_any_element(),
        };
        let key_id = id.clone();
        let key_change = on_change.clone();
        let key_options: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let trigger_label: SharedString = selected
            .map(|index| options[index].label.clone())
            .unwrap_or_else(|| placeholder.to_string())
            .into();
        let trigger = div()
            .id(SharedString::from(format!("{id}-trigger")))
            .role(gpui::Role::ComboBox)
            .aria_label(trigger_label)
            .aria_expanded(open)
            .track_focus(&focus)
            .flex_shrink_0()
            .when_some(width, |this, width| this.w(px(width)))
            .when(width.is_none(), |this| this.w_full())
            .max_w_full()
            .min_w_0()
            .h(px(CONTROL_HEIGHT))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(6.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(if focused || open {
                p.focus_border
            } else {
                p.hairline
            }))
            .bg(hsla(p.raised))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .when(disabled, |this| this.opacity(0.5))
            .when_some(disabled_reason.filter(|_| disabled), |this, reason| {
                this.tooltip(tooltip_text(reason))
            })
            .when(!disabled, |this| {
                let click_id = id.clone();
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(p.raised_hover)))
                    .on_press(cx, move |page, window, cx| {
                        page.toggle_dropdown(&click_id, selected, window, cx);
                    })
                    .on_key_down(cx.listener(move |page, event: &KeyDownEvent, window, cx| {
                        let key = event.keystroke.key.as_str();
                        let is_open = page.dropdowns.get(&key_id).is_some_and(|state| state.open);
                        if !is_open {
                            if matches!(key, "enter" | "space" | "down" | "up") {
                                cx.stop_propagation();
                                page.toggle_dropdown(&key_id, selected, window, cx);
                            }
                            return;
                        }
                        // The plain popup keeps focus on its trigger.
                        let count = key_options.len();
                        let Some(state) = page.dropdowns.get_mut(&key_id) else {
                            return;
                        };
                        match key {
                            "down" if count > 0 => {
                                state.highlight = Some(
                                    state
                                        .highlight
                                        .map_or(0, |index| (index + 1).min(count - 1)),
                                );
                            }
                            "up" if count > 0 => {
                                state.highlight = Some(
                                    state.highlight.map_or(0, |index| index.saturating_sub(1)),
                                );
                            }
                            "enter" | "space" => {
                                let chosen = state
                                    .highlight
                                    .and_then(|index| key_options.get(index))
                                    .cloned();
                                cx.stop_propagation();
                                page.close_dropdown(&key_id, window, cx);
                                if let Some(value) = chosen {
                                    key_change(page, value, window, cx);
                                }
                                return;
                            }
                            "escape" => {
                                cx.stop_propagation();
                                page.close_dropdown(&key_id, window, cx);
                                return;
                            }
                            _ => return,
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }))
            })
            .child(label_content)
            .child(
                settings_icon(
                    if searchable {
                        icons::CHEVRON_DOWN
                    } else {
                        icons::SELECTOR
                    },
                    16.0,
                    p.muted,
                )
                .flex_shrink_0(),
            );
        let wrapper = div()
            .flex_shrink_0()
            .when_some(width, |this, width| this.w(px(width)))
            .when(width.is_none(), |this| this.flex_1().w_full())
            .max_w_full()
            .min_w_0()
            .on_children_prepainted(capture_child_bounds(trigger_bounds.clone(), 0))
            .child(trigger);
        if !open {
            return wrapper.into_any_element();
        }
        let Some(bounds) = trigger_bounds.get() else {
            // The popup is placed from the trigger's bounds, known after this frame's layout.
            window.request_animation_frame();
            return wrapper.into_any_element();
        };
        let popup = if searchable {
            self.searchable_popup(
                &p,
                &id,
                options,
                &visible,
                selected,
                highlight,
                search.clone(),
                bounds,
                on_change,
                window,
                cx,
            )
        } else {
            self.plain_popup(
                &p, &id, options, selected, highlight, bounds, on_change, window, cx,
            )
        };
        wrapper.child(popup).into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn plain_popup(
        &mut self,
        p: &SettingsPalette,
        id: &SharedString,
        options: &[DropdownOption],
        selected: Option<usize>,
        highlight: Option<usize>,
        bounds: Bounds<Pixels>,
        on_change: ChangeHandler,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const ROW: f32 = 32.0;
        let p = *p;
        let rows = options.iter().enumerate().map(|(index, option)| {
            let is_selected = selected == Some(index);
            let highlighted = highlight == Some(index);
            let value = option.value.clone();
            let on_change = on_change.clone();
            let close_id = id.clone();
            div()
                .id((SharedString::from(format!("{id}-option")), index))
                .role(gpui::Role::ListBoxOption)
                .aria_label(SharedString::from(option.label.clone()))
                .aria_selected(is_selected)
                .w_full()
                .flex_shrink_0()
                .min_h(px(ROW))
                .px(px(8.0))
                .py(px(6.0))
                .flex()
                .items_center()
                .rounded(px(6.0))
                .text_size(px(14.0))
                .line_height(px(20.0))
                .cursor_default()
                .text_color(hsla(if is_selected {
                    p.popup_selected_foreground
                } else {
                    p.foreground
                }))
                .when(is_selected, |this| this.bg(hsla(p.popup_selected)))
                .when(!is_selected && highlighted, |this| {
                    this.bg(hsla(p.popup_hover))
                })
                .when(!is_selected, |this| {
                    this.hover(move |this| this.bg(hsla(p.popup_hover)))
                })
                .on_press(cx, move |page, window, cx| {
                    page.close_dropdown(&close_id, window, cx);
                    on_change(page, value.clone(), window, cx);
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(option.label.clone()),
                )
        });
        let offset = selected.unwrap_or(0) as f32 * ROW;
        let top = bounds.origin.y - px(6.0) - px(offset);
        let max_height = (window.viewport_size().height - px(16.0)).max(px(0.0));
        let close_id = id.clone();
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(bounds.origin.x, top.max(px(8.0))))
                .snap_to_window_with_margin(px(8.0))
                .child(
                    v_flex()
                        .id(SharedString::from(format!("{id}-popup")))
                        .occlude()
                        .w(bounds.size.width)
                        .max_h(max_height)
                        .overflow_y_scroll()
                        .p(px(4.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.popup_border))
                        .bg(hsla(p.popup_background))
                        .shadow_md()
                        .font_family(MODAL_UI_FONT)
                        .on_mouse_down_out(cx.listener(
                            move |page, event: &MouseDownEvent, window, cx| {
                                if bounds.contains(&event.position) {
                                    return;
                                }
                                page.close_dropdown(&close_id, window, cx);
                            },
                        ))
                        .children(rows),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn searchable_popup(
        &mut self,
        p: &SettingsPalette,
        id: &SharedString,
        options: &[DropdownOption],
        visible: &[usize],
        selected: Option<usize>,
        highlight: Option<usize>,
        search: Option<Entity<InputState>>,
        bounds: Bounds<Pixels>,
        on_change: ChangeHandler,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *p;
        let divider = if p.light {
            modal_rgba(0x000000, 0.12)
        } else {
            modal_rgba(0xffffff, 0.12)
        };
        let hover_fill = p.foreground_alpha(0.08);
        let rows: Vec<_> = visible
            .iter()
            .enumerate()
            .map(|(position, index)| {
                let option = &options[*index];
                let is_selected = selected == Some(*index);
                let highlighted = highlight == Some(position);
                let value = option.value.clone();
                let on_change = on_change.clone();
                let close_id = id.clone();
                let hover_id = id.clone();
                h_flex()
                    .id((SharedString::from(format!("{id}-option")), *index))
                    .role(gpui::Role::ListBoxOption)
                    .aria_label(SharedString::from(option.label.clone()))
                    .aria_selected(is_selected)
                    .w_full()
                    .flex_shrink_0()
                    .min_h(px(32.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .gap(px(8.0))
                    .items_center()
                    .rounded(px(6.0))
                    .text_size(px(13.0))
                    .line_height(px(20.0))
                    .cursor_default()
                    .text_color(hsla(if is_selected {
                        p.popup_selected_foreground
                    } else {
                        p.foreground
                    }))
                    .when(is_selected, |this| this.bg(hsla(p.popup_selected)))
                    .when(!is_selected && highlighted, |this| {
                        this.bg(hsla(hover_fill))
                    })
                    .on_mouse_move(cx.listener(
                        move |page, _: &gpui::MouseMoveEvent, _window, cx| {
                            if let Some(state) = page.dropdowns.get_mut(&hover_id)
                                && state.highlight != Some(position)
                            {
                                state.highlight = Some(position);
                                cx.notify();
                            }
                        },
                    ))
                    .on_press(cx, move |page, window, cx| {
                        page.close_dropdown(&close_id, window, cx);
                        on_change(page, value.clone(), window, cx);
                    })
                    .children(option.icon.as_deref().map(|icon| {
                        agent_icon(if icon == "custom" { None } else { Some(icon) }, &p)
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(option.label.clone()),
                    )
            })
            .collect();
        let has_query = search
            .as_ref()
            .is_some_and(|search| !search.read(cx).value().is_empty());
        let input_focused = search
            .as_ref()
            .is_some_and(|search| search.read(cx).focus_handle(cx).is_focused(window));
        let clear_input = search.clone();
        let search_field = search.as_ref().map(|search| {
            h_flex()
                .w_full()
                .h(px(32.0))
                .items_center()
                .rounded(px(6.0))
                .border_1()
                .border_color(hsla(if input_focused { p.ring } else { p.hairline }))
                .when(input_focused, |this| {
                    this.shadow(vec![gpui::BoxShadow {
                        color: hsla(css_fade(p.ring, 0.2)),
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(0.0),
                        spread_radius: px(3.0),
                        inset: false,
                    }])
                })
                .bg(hsla(p.foreground_alpha(0.045)))
                .child(
                    div().flex_1().min_w_0().pl(px(10.0)).pr(px(6.0)).child(
                        Input::new(search)
                            .with_size(ComponentSize::Small)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .px(px(0.0))
                            .py(px(0.0))
                            .text_size(px(13.0))
                            .text_color(hsla(p.foreground)),
                    ),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("{id}-search-addon")))
                        .flex_shrink_0()
                        .size(px(24.0))
                        .mr(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(has_query, |this| {
                            this.cursor_pointer().on_click(cx.listener(
                                move |_, _: &ClickEvent, window, cx| {
                                    if let Some(input) = clear_input.clone() {
                                        input.update(cx, |input, cx| {
                                            input.set_value("", window, cx);
                                            input.focus(window, cx);
                                        });
                                    }
                                },
                            ))
                        })
                        .child(if has_query {
                            settings_icon(icons::X, 16.0, p.muted).into_any_element()
                        } else {
                            div()
                                .opacity(0.5)
                                .child(settings_icon(icons::SEARCH, 16.0, p.muted))
                                .into_any_element()
                        }),
                )
        });
        let list: AnyElement = if visible.is_empty() {
            div()
                .w_full()
                .px(px(10.0))
                .py(px(24.0))
                .text_center()
                .text_size(px(13.0))
                .text_color(hsla(p.muted))
                .child("No matches found.")
                .into_any_element()
        } else {
            v_flex()
                .id(SharedString::from(format!("{id}-list")))
                .w_full()
                .max_h(px(288.0))
                .p(px(4.0))
                .overflow_y_scroll()
                .children(rows)
                .into_any_element()
        };
        let key_id = id.clone();
        let key_values: Vec<String> = visible
            .iter()
            .map(|index| options[*index].value.clone())
            .collect();
        let close_id = id.clone();
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(
                    bounds.origin.x,
                    bounds.origin.y + bounds.size.height + px(4.0),
                ))
                .snap_to_window_with_margin(px(8.0))
                .child(
                    v_flex()
                        .id(SharedString::from(format!("{id}-popup")))
                        .occlude()
                        .w(bounds.size.width)
                        .overflow_hidden()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(hsla(p.popup_border))
                        .bg(hsla(p.popup_background))
                        .shadow(vec![gpui::BoxShadow {
                            color: gpui::hsla(0.0, 0.0, 0.0, 0.35),
                            offset: point(px(0.0), px(12.0)),
                            blur_radius: px(28.0),
                            spread_radius: px(0.0),
                            inset: false,
                        }])
                        .font_family(MODAL_UI_FONT)
                        .text_color(hsla(p.foreground))
                        .on_mouse_down_out(cx.listener(
                            move |page, event: &MouseDownEvent, window, cx| {
                                if bounds.contains(&event.position) {
                                    return;
                                }
                                page.close_dropdown(&close_id, window, cx);
                            },
                        ))
                        .on_key_down(cx.listener(move |page, event: &KeyDownEvent, window, cx| {
                            let count = key_values.len();
                            let Some(state) = page.dropdowns.get_mut(&key_id) else {
                                return;
                            };
                            match event.keystroke.key.as_str() {
                                "down" if count > 0 => {
                                    state.highlight = Some(
                                        state
                                            .highlight
                                            .map_or(0, |index| (index + 1).min(count - 1)),
                                    );
                                }
                                "up" if count > 0 => {
                                    state.highlight = Some(
                                        state.highlight.map_or(0, |index| index.saturating_sub(1)),
                                    );
                                }
                                "enter" => {
                                    let chosen = state
                                        .highlight
                                        .and_then(|index| key_values.get(index))
                                        .cloned();
                                    cx.stop_propagation();
                                    page.close_dropdown(&key_id, window, cx);
                                    if let Some(value) = chosen {
                                        on_change(page, value, window, cx);
                                    }
                                    return;
                                }
                                "escape" => {
                                    cx.stop_propagation();
                                    page.close_dropdown(&key_id, window, cx);
                                    return;
                                }
                                _ => return,
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .children(search_field.map(|field| {
                            div()
                                .w_full()
                                .flex_shrink_0()
                                .p(px(8.0))
                                .border_b_1()
                                .border_color(hsla(divider))
                                .child(field)
                        }))
                        .child(list),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }
}
