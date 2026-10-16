//! Software-rendered plots; the adapter converts UTC instants to OS local time.
use super::{Action, palette::Palette};
use crate::{
    chart_axes::{date_label, minute_scale, time_ticks},
    charts::Timeline,
    domain::format_duration,
    preferences::Theme,
};
use fltk::{
    app, draw,
    enums::{Align, Event, Font},
    frame::Frame,
    prelude::*,
    window::Window,
};
use std::{cell::Cell, rc::Rc};
type Cursor = Rc<Cell<Option<f64>>>;

pub struct ChartsWindow {
    pub window: Window,
    /// Instructions and errors only; numerical readouts belong to the plots.
    pub summary: Frame,
    pub history: Frame,
    pub forecast: Frame,
    history_cursor: Cursor,
    cursor: Cursor,
    timeline: Timeline,
}
impl ChartsWindow {
    pub fn new(sender: app::Sender<Action>, timeline: Timeline) -> Self {
        let window = Window::new(55, 20, 910, 710, None).with_label("Usage history and forecast");
        let mut summary = Frame::new(20, 5, 870, 25, None)
            .with_label("Local time from your system. Move over a chart to inspect it.");
        summary.set_label_size(13);
        let mut history = Frame::new(15, 35, 880, 325, None);
        let mut forecast = Frame::new(15, 370, 880, 325, None);
        let cursor = Rc::new(Cell::new(None));
        let history_cursor = Rc::new(Cell::new(None));
        bind_cursor(&mut history, history_cursor.clone());
        bind_cursor(&mut forecast, cursor.clone());
        window.end();
        let mut result = Self {
            window,
            summary,
            history,
            forecast,
            cursor,
            history_cursor,
            timeline,
        };
        result.window.set_callback(|window| window.hide());
        result
            .window
            .handle(move |_, event| super::handle_shortcut(sender, event));
        result
    }
    pub fn update(&mut self, timeline: Timeline, theme: Theme) {
        self.timeline = timeline;
        self.summary
            .set_label("Local time from your system. Move over a chart to inspect it.");
        self.redraw(theme);
    }
    /// Embedded callers can inspect the same curve without synthesizing events.
    pub fn inspect_forecast(&mut self, fraction: Option<f64>) {
        self.cursor.set(
            fraction
                .filter(|value| value.is_finite())
                .map(|value| value.clamp(0.0, 1.0)),
        );
        self.forecast.redraw();
    }
    pub fn forecast_readout(&self) -> String {
        forecast_readout(&self.timeline, self.cursor.get().unwrap_or(0.0))
    }
    pub fn redraw(&mut self, theme: Theme) {
        let palette = theme.palette();
        self.window.set_color(palette.background);
        self.summary.set_label_color(palette.text);
        let past = self.timeline.clone();
        let history_cursor = self.history_cursor.clone();
        self.history
            .draw(move |frame| plot(frame, &past, false, palette, history_cursor.get()));
        let future = self.timeline.clone();
        let cursor = self.cursor.clone();
        self.forecast
            .draw(move |frame| plot(frame, &future, true, palette, cursor.get()));
        self.window.redraw();
    }
}
impl Drop for ChartsWindow {
    fn drop(&mut self) {
        if !self.window.was_deleted() {
            app::delete_widget(self.window.clone());
            app::check();
        }
    }
}
fn bind_cursor(frame: &mut Frame, cursor: Cursor) {
    frame.handle(move |frame, event| match event {
        Event::Move | Event::Drag | Event::Push => {
            let fraction = (app::event_x() - frame.x() - 65) as f64 / (frame.w() - 90) as f64;
            cursor.set(Some(fraction.clamp(0.0, 1.0)));
            frame.redraw();
            true
        }
        Event::Leave => {
            cursor.set(None);
            frame.redraw();
            true
        }
        _ => false,
    });
}
fn forecast_readout(timeline: &Timeline, fraction: f64) -> String {
    let offset = (timeline.settings.window.millis() as f64 * fraction) as u64;
    let used = timeline.usage_after(offset);
    let quota = timeline
        .available(used)
        .map(|value| format!("  Available {}", format_duration(value)))
        .unwrap_or_default();
    format!(
        "{}  Used {}  Released {}{}",
        date_label(timeline.now.millis() + offset as i64, &chrono::Local, true),
        format_duration(used),
        format_duration(timeline.used_now().saturating_sub(used)),
        quota
    )
}
fn plot(
    frame: &mut Frame,
    timeline: &Timeline,
    future: bool,
    palette: Palette,
    cursor: Option<f64>,
) {
    let left = frame.x() + 65;
    let top = frame.y() + 80;
    let width = frame.w() - 90;
    let height = frame.h() - 160;
    let span = timeline.settings.window.millis() as i64;
    let start = if future {
        timeline.now.millis()
    } else {
        (timeline.now.millis() - span).max(0)
    };
    let end = if future {
        start + span
    } else {
        timeline.now.millis()
    };
    let duration = (end - start).max(1) as f64;
    let points = if future {
        &timeline.forecast
    } else {
        &timeline.history
    };
    let maximum = points.iter().map(|point| point.used).max().unwrap_or(0);
    let (ceiling, step) = minute_scale(maximum);
    let x =
        |at: i64| left + (((at - start) as f64 / duration).clamp(0.0, 1.0) * width as f64) as i32;
    let y = |used: u64| {
        top + height - (used as f64 / (ceiling as f64 * 60_000.0) * height as f64) as i32
    };
    draw::push_clip(frame.x(), frame.y(), frame.w(), frame.h());
    draw::set_draw_color(palette.background);
    draw::draw_rectf(frame.x(), frame.y(), frame.w(), frame.h());
    draw::set_font(Font::HelveticaBold, 15);
    draw::set_draw_color(palette.text);
    draw::draw_text2(
        if future {
            "When your minutes return (if you stop using now)"
        } else {
            "When you used your time"
        },
        left,
        frame.y(),
        width,
        24,
        Align::Left,
    );
    draw::set_font(Font::Helvetica, 13);
    if future {
        draw::set_draw_color(palette.accent);
        draw::draw_text2("Used time", left, frame.y() + 26, 120, 20, Align::Left);
        draw::set_draw_color(palette.stop);
        draw::draw_text2(
            "Released time",
            left + 130,
            frame.y() + 26,
            140,
            20,
            Align::Left,
        );
    } else {
        draw::draw_text2(
            "Minutes recorded in each time bucket",
            left,
            frame.y() + 26,
            width,
            20,
            Align::Left,
        );
    }
    for value in (0..=ceiling).step_by(step as usize) {
        let py = y(value * 60_000);
        draw::set_draw_color(palette.grid);
        draw::draw_line(left, py, left + width, py);
        draw::set_draw_color(palette.text);
        draw::draw_text2(&value.to_string(), frame.x(), py - 9, 54, 18, Align::Right);
    }
    draw::draw_text2("min", frame.x(), top - 24, 54, 20, Align::Right);
    let ticks = time_ticks(start, end, &chrono::Local);
    let label_stride = ticks.len().div_ceil((width / 95).max(1) as usize).max(1);
    for (index, &at) in ticks.iter().enumerate() {
        let px = x(at);
        draw::set_draw_color(palette.grid);
        draw::draw_line(px, top, px, top + height);
        if index % label_stride == 0 {
            draw::set_draw_color(palette.text);
            draw::draw_text2(
                &date_label(at, &chrono::Local, false),
                (px - 40).clamp(left, left + width - 80),
                top + height + 8,
                80,
                36,
                Align::Center,
            );
        }
    }
    draw::set_draw_color(palette.accent);
    if future {
        draw::set_line_style(draw::LineStyle::Solid, 2);
        for pair in points.windows(2) {
            if let [a, b] = pair {
                draw::draw_line(x(a.at), y(a.used), x(b.at), y(b.used));
            }
        }
        draw::set_draw_color(palette.stop);
        draw::set_line_style(draw::LineStyle::Dash, 2);
        for pair in points.windows(2) {
            if let [a, b] = pair {
                draw::draw_line(
                    x(a.at),
                    y(timeline.used_now().saturating_sub(a.used)),
                    x(b.at),
                    y(timeline.used_now().saturating_sub(b.used)),
                );
            }
        }
        draw::set_line_style(draw::LineStyle::Solid, 0);
        draw::set_draw_color(palette.text);
        let clear_x = x(timeline.clears_at);
        draw::draw_line(clear_x, top, clear_x, top + height);
        draw::draw_text2(
            &format!(
                "All usage expires: {}",
                date_label(timeline.clears_at, &chrono::Local, true)
            ),
            left,
            frame.y() + frame.h() - 28,
            width,
            24,
            Align::Left,
        );
    } else {
        let mut previous = left;
        for point in points {
            let px = x(point.at);
            let py = y(point.used);
            draw::draw_rectf(
                previous + 1,
                py,
                (px - previous - 1).max(1),
                top + height - py,
            );
            previous = px;
        }
    }
    // Readouts stay inside the chart's header to avoid clipping at either edge.
    let fraction = cursor.unwrap_or(0.0);
    let at = start + (duration * fraction) as i64;
    draw::set_draw_color(palette.text);
    if let Some(fraction) = cursor {
        let px = left + (width as f64 * fraction) as i32;
        draw::set_line_style(draw::LineStyle::Dot, 1);
        draw::draw_line(px, top, px, top + height);
        draw::set_line_style(draw::LineStyle::Solid, 0);
    }
    let readout = if future {
        forecast_readout(timeline, fraction)
    } else {
        let used = points
            .iter()
            .find(|point| point.at >= at)
            .map(|point| point.used)
            .unwrap_or(0);
        format!(
            "{}  Recorded in this bucket: {}",
            date_label(at, &chrono::Local, true),
            format_duration(used)
        )
    };
    draw::draw_text2(&readout, left, frame.y() + 50, width, 24, Align::Left);
    draw::pop_clip();
}
