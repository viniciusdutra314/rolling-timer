//! A thin desktop adapter. Accounting callbacks enqueue fallible actions;
//! chart pointer callbacks only update cursor state and request a repaint.
mod charts;
mod palette;
pub use charts::ChartsWindow;

use crate::{
    Result,
    domain::{Settings, Timestamp, Window, format_duration},
    engine::{Engine, Snapshot, Store},
    preferences::Theme,
};
use fltk::{
    app,
    button::Button,
    enums::{Align, CallbackTrigger, Font},
    frame::Frame,
    input::Input,
    menu::Choice,
    prelude::*,
    window::Window as DesktopWindow,
};

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Toggle,
    Apply,
    Close,
    Theme,
    Charts,
    HideCharts,
}

// Both windows route keyboard commands to the same fallible event dispatcher.
fn handle_shortcut(sender: app::Sender<Action>, event: fltk::enums::Event) -> bool {
    if event == fltk::enums::Event::Shortcut && app::event_key() == fltk::enums::Key::Escape {
        sender.send(Action::HideCharts);
        return true;
    }
    if event == fltk::enums::Event::Shortcut
        && app::event_key() == fltk::enums::Key::from_char('q')
        && app::event_state().contains(fltk::enums::Shortcut::Command)
    {
        sender.send(Action::Close);
        true
    } else {
        false
    }
}

pub struct Desktop {
    pub window: DesktopWindow,
    pub toggle: Button,
    pub apply: Button,
    pub window_hours: Input,
    pub limit_hours: Input,
    pub used: Frame,
    pub remaining: Frame,
    pub status: Frame,
    pub theme_choice: Choice,
    pub charts_button: Button,
    pub charts: Option<ChartsWindow>,
    sender: app::Sender<Action>,
    theme: Theme,
    recording: bool,
    save_theme: Box<dyn FnMut(Theme) -> Result<()>>,
    had_tick_error: bool,
}
impl Desktop {
    pub fn new(
        settings: Settings,
        sender: app::Sender<Action>,
        theme: Theme,
        save_theme: impl FnMut(Theme) -> Result<()> + 'static,
    ) -> Self {
        let background = theme.palette().background;
        let foreground = theme.palette().text;
        // In fltk-rs 1.5.23, new(..., Some(label)) transfers a CString without
        // freeing it. with_label calls copy_label, which FLTK owns and releases.
        let mut window = DesktopWindow::new(200, 200, 420, 495, None).with_label("Rolling Timer");
        window.set_color(background);
        let mut title = Frame::new(20, 15, 380, 30, None).with_label("Rolling Timer");
        title.set_label_font(Font::HelveticaBold);
        title.set_label_size(19);
        let mut used = Frame::new(20, 55, 380, 60, None).with_label("00:00:00");
        used.set_label_font(Font::HelveticaBold);
        used.set_label_size(44);
        let mut caption =
            Frame::new(20, 115, 380, 20, None).with_label("used in the sliding window");
        caption.set_label_size(13);
        let mut remaining = Frame::new(20, 140, 380, 25, None).with_label("No usage limit set");
        remaining.set_label_size(15);
        let mut toggle = Button::new(115, 180, 190, 42, None).with_label("Start");
        toggle.set_label_color(background);
        toggle.set_label_size(17);
        toggle.emit(sender, Action::Toggle);
        let mut window_hours = Input::new(175, 240, 105, 28, None).with_label("Window (hours)");
        window_hours.set_value(&settings.window.hours_text());
        window_hours.set_tooltip("Positive decimal hours. Usage expires continuously.");
        let mut limit_hours = Input::new(175, 278, 105, 28, None).with_label("Limit (hours)");
        limit_hours.set_value(&settings.limit.map(Window::hours_text).unwrap_or_default());
        limit_hours.set_tooltip("Optional. Leave blank to track usage without a quota.");
        let mut apply = Button::new(300, 240, 95, 66, None).with_label("Apply");
        apply.emit(sender, Action::Apply);
        for input in [&mut window_hours, &mut limit_hours] {
            input.set_text_color(foreground);
            input.set_trigger(CallbackTrigger::EnterKey);
            input.emit(sender, Action::Apply);
        }
        let mut theme_choice = Choice::new(175, 320, 220, 28, None).with_label("Theme");
        theme_choice.add_choice("Dark|Light|Mocha|Cappuccino");
        theme_choice.emit(sender, Action::Theme);
        let mut charts_button = Button::new(115, 365, 190, 32, None).with_label("Usage charts");
        charts_button.emit(sender, Action::Charts);
        let mut status = Frame::new(20, 415, 380, 65, None)
            .with_label("Stopped. Sessions are saved automatically.");
        status.set_label_size(13);
        status.set_align(Align::Inside | Align::Wrap);
        window.end();
        // A keyboard exit also lets automated desktop tests exercise the same
        // fallible close path as a window-manager close request.
        window.handle(move |_, event| handle_shortcut(sender, event));
        window.set_callback(move |_| sender.send(Action::Close));
        let mut desktop = Self {
            window,
            toggle,
            apply,
            window_hours,
            limit_hours,
            used,
            remaining,
            status,
            theme_choice,
            charts_button,
            charts: None,
            sender,
            theme,
            recording: false,
            save_theme: Box::new(save_theme),
            had_tick_error: false,
        };
        desktop.apply_palette(theme);
        desktop
    }
    pub fn theme(&self) -> Theme {
        self.theme
    }
    fn apply_palette(&mut self, theme: Theme) {
        self.theme = theme;
        self.theme_choice.set_value(theme.index());
        let palette = theme.palette();
        let (r, g, b) = palette.background.to_rgb();
        app::background(r, g, b);
        let (r, g, b) = palette.surface.to_rgb();
        app::background2(r, g, b);
        let (r, g, b) = palette.text.to_rgb();
        app::foreground(r, g, b);
        self.window.set_color(palette.background);
        for mut widget in self.window.clone().into_iter() {
            widget.set_label_color(palette.text);
        }
        for input in [&mut self.window_hours, &mut self.limit_hours] {
            input.set_color(palette.surface);
            input.set_text_color(palette.text);
            input.set_selection_color(palette.accent);
        }
        self.theme_choice.set_color(palette.surface);
        self.theme_choice.set_text_color(palette.text);
        self.theme_choice.set_selection_color(palette.accent);
        for button in [&mut self.apply, &mut self.charts_button] {
            button.set_color(palette.surface);
        }
        self.toggle.set_label_color(palette.button_text);
        self.toggle.set_color(if self.recording {
            palette.stop
        } else {
            palette.accent
        });
        if let Some(charts) = &mut self.charts {
            charts.redraw(theme);
        }
        self.window.redraw();
    }
    fn change_theme(&mut self) -> Result<()> {
        let result = Theme::from_index(self.theme_choice.value()).and_then(|theme| {
            (self.save_theme)(theme)?;
            self.apply_palette(theme);
            Ok(())
        });
        // A failed save must not advertise an uncommitted choice in the menu.
        if result.is_err() {
            self.theme_choice.set_value(self.theme.index());
        }
        result?;
        self.report("Theme saved.");
        Ok(())
    }
    pub fn show(&mut self) {
        self.window.show();
    }
    pub fn render(&mut self, snapshot: Snapshot) {
        self.recording = snapshot.recording;
        self.used.set_label(&format_duration(snapshot.used));
        self.remaining.set_label(
            &snapshot
                .remaining
                .map(|value| format!("{} available", format_duration(value)))
                .unwrap_or_else(|| "No usage limit set".into()),
        );
        self.toggle
            .set_label(if snapshot.recording { "Stop" } else { "Start" });
        self.toggle.set_color(if snapshot.recording {
            self.theme.palette().stop
        } else {
            self.theme.palette().accent
        });
        self.toggle.redraw();
    }
    pub fn report(&mut self, message: &str) {
        // Explicit action feedback supersedes any older periodic error.
        self.had_tick_error = false;
        // FLTK strings cross an FFI boundary; never pass embedded NULs from an
        // external error message to a widget constructor/setter.
        self.status.set_label(&message.replace('\0', "�"));
    }
    fn report_tracking(&mut self, recording: bool) {
        self.report(if recording {
            "Recording. Keep this app open while using your timer."
        } else {
            "Stopped. Older usage continues to expire."
        });
    }
    pub fn refresh<S: Store>(&mut self, engine: &mut Engine<S>, now: Timestamp) -> Result<()> {
        let snapshot =
            if let Some(charts) = self.charts.as_mut().filter(|charts| charts.window.shown()) {
                let timeline = engine.timeline(now)?;
                let used = timeline.used_now();
                let snapshot = Snapshot {
                    used,
                    remaining: timeline.available(used),
                    recording: engine.recording(),
                };
                charts.update(timeline, self.theme);
                snapshot
            } else {
                engine.tick(now)?
            };
        self.render(snapshot);
        Ok(())
    }
    /// Returns true only after a successful close checkpoint. Errors leave both
    /// the app and inputs available for correction/retry.
    pub fn dispatch<S: Store>(
        &mut self,
        engine: &mut Engine<S>,
        action: Action,
        now: Result<Timestamp>,
    ) -> Result<bool> {
        let now = match action {
            Action::Theme => {
                self.change_theme()?;
                return Ok(false);
            }
            Action::Charts => {
                let now = now?;
                let timeline = engine.timeline(now)?;
                let charts = self
                    .charts
                    .get_or_insert_with(|| ChartsWindow::new(self.sender, timeline.clone()));
                charts.update(timeline, self.theme);
                charts.window.show();
                now
            }
            Action::HideCharts => {
                if let Some(charts) = &mut self.charts {
                    charts.window.hide();
                }
                return Ok(false);
            }
            Action::Toggle => {
                let now = now?;
                engine.toggle(now)?;
                now
            }
            Action::Apply => {
                let now = now?;
                let window = Window::parse_hours(&self.window_hours.value())?;
                let limit_text = self.limit_hours.value();
                let limit = if limit_text.trim().is_empty() {
                    None
                } else {
                    Some(Window::parse_hours(&limit_text)?)
                };
                engine.change_settings(Settings { window, limit })?;
                self.report("Settings saved.");
                now
            }
            Action::Close => {
                engine.close(now?)?;
                self.window.hide();
                return Ok(true);
            }
        };
        self.refresh(engine, now)?;
        if matches!(action, Action::Toggle) {
            self.report_tracking(engine.recording());
        }
        Ok(false)
    }
    /// Error presentation belongs to the desktop adapter. Accepting a fallible
    /// timestamp lets the caller report clock failures through the same path.
    pub fn handle_action<S: Store>(
        &mut self,
        engine: &mut Engine<S>,
        action: Action,
        now: Result<Timestamp>,
    ) -> bool {
        match self.dispatch(engine, action, now) {
            Ok(closed) => closed,
            Err(error) => {
                self.report(&error.to_string());
                false
            }
        }
    }
    pub fn update<S: Store>(&mut self, engine: &mut Engine<S>, now: Result<Timestamp>) {
        match now.and_then(|now| self.refresh(engine, now)) {
            Ok(()) => {
                // A successful retry must not leave a stale storage/clock error
                // on screen. Input-validation errors persist until an action.
                if self.had_tick_error {
                    self.report_tracking(engine.recording());
                }
            }
            Err(error) => {
                self.report(&error.to_string());
                self.had_tick_error = true;
                if let Some(charts) = &mut self.charts {
                    charts.summary.set_label(&format!(
                        "Could not refresh charts: {}",
                        error.to_string().replace('\0', "�")
                    ));
                }
            }
        }
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        // Dropping Rust widget handles only drops their trackers, not the root
        // C++ window. Deleting the root also deletes its child widgets. Run the
        // deferred callback cleanup while the UI thread and FLTK are still alive.
        if !self.window.was_deleted() {
            app::delete_widget(self.window.clone());
            app::check();
        }
    }
}
