use gpui::{
    App, AppContext as _, Bounds, BoxShadow, Context, DragMoveEvent, Empty, Entity, EntityId,
    FocusHandle, Focusable, Hsla, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, ParentElement as _, Pixels, Point, Render, RenderOnce, Rgba,
    StatefulInteractiveElement as _, Styled as _, Window, div, hsla, linear_color_stop,
    linear_gradient, point, px, rgba,
};

use crate::ElementExt as _;

use super::ColorPickerState;

const DEFAULT_SIZE: Pixels = px(200.);
// Leaves one pixel for the saturation/value interaction area.
const MINIMUM_SIZE: Pixels = px(37.);
const SPECTRUM_BORDER: Pixels = px(12.);
const HUE_HEIGHT: Pixels = px(24.);
const POINTER_SIZE: Pixels = px(28.);
const FOCUSED_POINTER_SIZE: Pixels = px(30.8);
const KEYBOARD_STEP: f32 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorControl {
    Spectrum,
    Hue,
}

#[derive(Clone)]
struct ColorPickerDrag {
    entity_id: EntityId,
    control: ColorControl,
}

impl Render for ColorPickerDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Hsv {
    h: f32,
    s: f32,
    v: f32,
}

impl Hsv {
    fn from_hsla(color: Hsla) -> Self {
        let rgba = Rgba::from(color);
        let max = rgba.r.max(rgba.g.max(rgba.b));
        let min = rgba.r.min(rgba.g.min(rgba.b));
        let delta = max - min;
        let h = if delta == 0. {
            color.h
        } else {
            Hsla::from(rgba).h
        };

        Self {
            h: h.rem_euclid(1.),
            s: if max == 0. { 0. } else { delta / max },
            v: max,
        }
    }

    fn to_hsla(self, alpha: f32) -> Hsla {
        let h = self.h.rem_euclid(1.) * 6.;
        let sector = h.floor() as usize;
        let fraction = h - h.floor();
        let p = self.v * (1. - self.s);
        let q = self.v * (1. - fraction * self.s);
        let t = self.v * (1. - (1. - fraction) * self.s);
        let (r, g, b) = match sector % 6 {
            0 => (self.v, t, p),
            1 => (q, self.v, p),
            2 => (p, self.v, t),
            3 => (p, q, self.v),
            4 => (t, p, self.v),
            _ => (self.v, p, q),
        };

        Hsla::from(Rgba {
            r,
            g,
            b,
            a: alpha.clamp(0., 1.),
        })
    }
}

pub(super) struct ColorPickerPanelState {
    hsv: Hsv,
    alpha: f32,
    spectrum_bounds: Bounds<Pixels>,
    hue_bounds: Bounds<Pixels>,
    spectrum_focus: FocusHandle,
    hue_focus: FocusHandle,
}

impl ColorPickerPanelState {
    pub(super) fn new(cx: &mut Context<ColorPickerState>) -> Self {
        Self {
            hsv: Hsv {
                h: 0.,
                s: 0.,
                v: 0.,
            },
            alpha: 1.,
            spectrum_bounds: Bounds::default(),
            hue_bounds: Bounds::default(),
            spectrum_focus: cx.focus_handle(),
            hue_focus: cx.focus_handle(),
        }
    }

    pub(super) fn set_value(&mut self, value: Option<Hsla>) {
        if let Some(value) = value {
            self.hsv = Hsv::from_hsla(value);
            self.alpha = value.a;
        }
    }

    fn value(&self) -> Hsla {
        self.hsv.to_hsla(self.alpha)
    }

    fn update_from_position(&mut self, control: ColorControl, position: Point<Pixels>) -> bool {
        let bounds = match control {
            ColorControl::Spectrum => self.spectrum_bounds,
            ColorControl::Hue => self.hue_bounds,
        };
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return false;
        }

        let x = normalized(position.x, bounds.left(), bounds.size.width);
        let y = normalized(position.y, bounds.top(), bounds.size.height);
        let previous = self.hsv;
        update_hsv_from_position(&mut self.hsv, control, x, y);
        self.hsv != previous
    }

    fn adjust_with_keyboard(&mut self, control: ColorControl, event: &KeyDownEvent) -> bool {
        let previous = self.hsv;
        match (control, event.keystroke.key.as_str()) {
            (ColorControl::Spectrum, "left") => {
                self.hsv.s = (self.hsv.s - KEYBOARD_STEP).max(0.);
            }
            (ColorControl::Spectrum, "right") => {
                self.hsv.s = (self.hsv.s + KEYBOARD_STEP).min(1.);
            }
            (ColorControl::Spectrum, "up") => {
                self.hsv.v = (self.hsv.v + KEYBOARD_STEP).min(1.);
            }
            (ColorControl::Spectrum, "down") => {
                self.hsv.v = (self.hsv.v - KEYBOARD_STEP).max(0.);
            }
            (ColorControl::Hue, "left") => {
                self.hsv.h = (self.hsv.h - KEYBOARD_STEP).max(0.);
            }
            (ColorControl::Hue, "right") => {
                self.hsv.h = (self.hsv.h + KEYBOARD_STEP).min(1.);
            }
            (ColorControl::Hue, "up" | "down") => {}
            _ => return false,
        }
        self.hsv != previous
    }
}

/// An inline saturation/value and hue color picker.
///
/// The panel shares a [`ColorPickerState`] with [`super::ColorPicker`], so both
/// presentations remain synchronized and emit the same color change events.
#[derive(IntoElement)]
pub struct ColorPickerPanel {
    state: Entity<ColorPickerState>,
    size: Pixels,
}

impl ColorPickerPanel {
    /// Creates an inline color picker for the given [`ColorPickerState`].
    pub fn new(state: &Entity<ColorPickerState>) -> Self {
        Self {
            state: state.clone(),
            size: DEFAULT_SIZE,
        }
    }

    /// Sets the panel's outer edge length. The panel always remains square.
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.size = size.into().max(MINIMUM_SIZE);
        self
    }
}

impl Focusable for ColorPickerPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.read(cx).panel.spectrum_focus.clone()
    }
}

impl RenderOnce for ColorPickerPanel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let entity_id = self.state.entity_id();
        let state = self.state.read(cx);
        let size = self.size;
        let spectrum_height = size - HUE_HEIGHT;
        let interactive_height = spectrum_height - SPECTRUM_BORDER;
        let hsv = state.panel.hsv;
        let selected_color = state.panel.value();
        let spectrum_focus = state.panel.spectrum_focus.clone().tab_stop(true);
        let spectrum_focused = spectrum_focus.is_focused(window);
        let hue_focus = state.panel.hue_focus.clone().tab_stop(true);
        let hue_focused = hue_focus.is_focused(window);

        let spectrum_state = self.state.downgrade();
        let spectrum = div()
            .id(("color-picker-panel-spectrum", entity_id))
            .relative()
            .w_full()
            .h(spectrum_height)
            .flex_shrink_0()
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_t(px(8.))
                    .bg(hsla(hsv.h, 1., 0.5, 1.))
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_t(px(8.))
                            .bg(linear_gradient(
                                90.,
                                linear_color_stop(hsla(0., 0., 1., 1.), 0.),
                                linear_color_stop(hsla(0., 0., 1., 0.), 1.),
                            )),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_t(px(8.))
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(rgba(0x00000000), 0.),
                                linear_color_stop(rgba(0x000000ff), 1.),
                            )),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .h(SPECTRUM_BORDER)
                            .bg(rgba(0x000000ff)),
                    ),
            )
            .child(
                div()
                    .id(("color-picker-panel-spectrum-interaction", entity_id))
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .h(interactive_height)
                    .track_focus(&spectrum_focus)
                    .on_prepaint(move |bounds, _, cx| {
                        if let Err(error) = spectrum_state.update(cx, |state, _| {
                            state.panel.spectrum_bounds = bounds;
                        }) {
                            log::debug!("failed to update color spectrum bounds: {error}");
                        }
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(
                            &self.state,
                            |state, event: &MouseDownEvent, window, cx| {
                                state.panel.spectrum_focus.focus(window, cx);
                                update_from_position(
                                    state,
                                    ColorControl::Spectrum,
                                    event.position,
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            },
                        ),
                    )
                    .on_key_down(window.listener_for(
                        &self.state,
                        |state, event: &KeyDownEvent, window, cx| {
                            adjust_with_keyboard(state, ColorControl::Spectrum, event, window, cx);
                        },
                    ))
                    .on_drag(
                        ColorPickerDrag {
                            entity_id,
                            control: ColorControl::Spectrum,
                        },
                        |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        },
                    )
                    .on_drag_move(window.listener_for(
                        &self.state,
                        move |state, event: &DragMoveEvent<ColorPickerDrag>, window, cx| {
                            let drag = event.drag(cx);
                            if drag.entity_id == entity_id && drag.control == ColorControl::Spectrum
                            {
                                update_from_position(
                                    state,
                                    ColorControl::Spectrum,
                                    event.event.position,
                                    window,
                                    cx,
                                );
                            }
                        },
                    )),
            );

        let hue_state = self.state.downgrade();
        let hue = div()
            .id(("color-picker-panel-hue", entity_id))
            .relative()
            .w_full()
            .h(HUE_HEIGHT)
            .flex_shrink_0()
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .overflow_hidden()
                    .rounded_b(px(8.))
                    .children(hue_segments()),
            )
            .child(
                div()
                    .id(("color-picker-panel-hue-interaction", entity_id))
                    .absolute()
                    .inset_0()
                    .track_focus(&hue_focus)
                    .on_prepaint(move |bounds, _, cx| {
                        if let Err(error) = hue_state.update(cx, |state, _| {
                            state.panel.hue_bounds = bounds;
                        }) {
                            log::debug!("failed to update color hue bounds: {error}");
                        }
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(
                            &self.state,
                            |state, event: &MouseDownEvent, window, cx| {
                                state.panel.hue_focus.focus(window, cx);
                                update_from_position(
                                    state,
                                    ColorControl::Hue,
                                    event.position,
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            },
                        ),
                    )
                    .on_key_down(window.listener_for(
                        &self.state,
                        |state, event: &KeyDownEvent, window, cx| {
                            adjust_with_keyboard(state, ColorControl::Hue, event, window, cx);
                        },
                    ))
                    .on_drag(
                        ColorPickerDrag {
                            entity_id,
                            control: ColorControl::Hue,
                        },
                        |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        },
                    )
                    .on_drag_move(window.listener_for(
                        &self.state,
                        move |state, event: &DragMoveEvent<ColorPickerDrag>, window, cx| {
                            let drag = event.drag(cx);
                            if drag.entity_id == entity_id && drag.control == ColorControl::Hue {
                                update_from_position(
                                    state,
                                    ColorControl::Hue,
                                    event.event.position,
                                    window,
                                    cx,
                                );
                            }
                        },
                    )),
            );

        div()
            .id(("color-picker-panel", entity_id))
            .relative()
            .flex()
            .flex_col()
            .w(size)
            .h(size)
            .child(spectrum)
            .child(hue)
            // Handles are painted last so neither track can cover them.
            .child(pointer(
                px(hsv.s * size.to_f64() as f32),
                px((1. - hsv.v) * interactive_height.to_f64() as f32),
                selected_color,
                spectrum_focused,
            ))
            .child(pointer(
                px(hsv.h * size.to_f64() as f32),
                spectrum_height + HUE_HEIGHT / 2.,
                hsla(hsv.h, 1., 0.5, 1.),
                hue_focused,
            ))
    }
}

fn update_from_position(
    state: &mut ColorPickerState,
    control: ColorControl,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut Context<ColorPickerState>,
) {
    if state.panel.update_from_position(control, position) {
        let value = state.panel.value();
        state.update_value_from_panel(value, true, window, cx);
    }
}

fn adjust_with_keyboard(
    state: &mut ColorPickerState,
    control: ColorControl,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<ColorPickerState>,
) {
    if state.panel.adjust_with_keyboard(control, event) {
        let value = state.panel.value();
        state.update_value_from_panel(value, true, window, cx);
    }
    if matches!(
        event.keystroke.key.as_str(),
        "left" | "right" | "up" | "down"
    ) {
        cx.stop_propagation();
    }
}

fn pointer(left: Pixels, top: Pixels, color: Hsla, focused: bool) -> impl IntoElement {
    let size = if focused {
        FOCUSED_POINTER_SIZE
    } else {
        POINTER_SIZE
    };
    div()
        .absolute()
        .left(left)
        .top(top)
        .ml(-size / 2.)
        .mt(-size / 2.)
        .size(size)
        .rounded_full()
        .bg(color)
        .border_2()
        .border_color(rgba(0xffffffff))
        .shadow(vec![BoxShadow {
            color: rgba(0x00000033).into(),
            offset: point(px(0.), px(2.)),
            blur_radius: px(4.),
            spread_radius: px(0.),
            inset: false,
        }])
}

fn hue_segments() -> impl Iterator<Item = impl IntoElement> {
    const HUES: [f32; 7] = [0., 1. / 6., 2. / 6., 3. / 6., 4. / 6., 5. / 6., 1.];
    HUES.windows(2).enumerate().map(|(index, pair)| {
        let segment = div().flex_1().h_full().bg(linear_gradient(
            90.,
            linear_color_stop(hsla(pair[0], 1., 0.5, 1.), 0.),
            linear_color_stop(hsla(pair[1], 1., 0.5, 1.), 1.),
        ));
        match index {
            0 => segment.rounded_bl(px(8.)),
            5 => segment.rounded_br(px(8.)),
            _ => segment,
        }
    })
}

fn normalized(value: Pixels, start: Pixels, length: Pixels) -> f32 {
    (((value - start).to_f64() / length.to_f64()) as f32).clamp(0., 1.)
}

fn update_hsv_from_position(hsv: &mut Hsv, control: ColorControl, x: f32, y: f32) {
    match control {
        ColorControl::Spectrum => {
            hsv.s = x;
            hsv.v = 1. - y;
        }
        ColorControl::Hue => hsv.h = x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }

    #[test]
    fn hsv_hsla_round_trip_preserves_rgba() {
        for color in [
            hsla(0., 1., 0.5, 1.),
            hsla(0.31, 0.73, 0.42, 0.8),
            hsla(0.83, 0.57, 0.68, 0.35),
            hsla(0., 0., 0.25, 1.),
        ] {
            let output = Hsv::from_hsla(color).to_hsla(color.a);
            let expected = Rgba::from(color);
            let actual = Rgba::from(output);
            assert_close(actual.r, expected.r);
            assert_close(actual.g, expected.g);
            assert_close(actual.b, expected.b);
            assert_close(actual.a, expected.a);
        }
    }

    #[test]
    fn normalized_positions_are_clamped() {
        assert_eq!(normalized(px(-10.), px(0.), px(100.)), 0.);
        assert_eq!(normalized(px(25.), px(0.), px(100.)), 0.25);
        assert_eq!(normalized(px(120.), px(0.), px(100.)), 1.);
    }

    #[test]
    fn hue_drag_changes_only_hue() {
        let mut hsv = Hsv {
            h: 0.1,
            s: 0.4,
            v: 0.7,
        };
        update_hsv_from_position(&mut hsv, ColorControl::Hue, 0.8, 0.);
        assert_eq!(
            hsv,
            Hsv {
                h: 0.8,
                s: 0.4,
                v: 0.7,
            }
        );
        update_hsv_from_position(&mut hsv, ColorControl::Hue, 0.2, 1.);
        assert_eq!(
            hsv,
            Hsv {
                h: 0.2,
                s: 0.4,
                v: 0.7,
            }
        );
    }
}
