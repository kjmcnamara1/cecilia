//! Design mode: adjusting the page layout by hand. The adjustments are the score's
//! [`DesignOverrides`]; this module holds the commands that change them and the preview used
//! while a mouse drag is in progress (one drag is one undo step).

use cecilia_core::design::{AUTO_WIDTH, DesignOverrides, MAX_WIDTH, MIN_WIDTH};

use crate::app::{App, PaneKind};
use crate::command::Amount;

/// What a design command does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignCmd {
    /// Design mode on, off or toggled.
    Mode(Option<bool>),
    /// Width of the cursor bar, in percent of its automatic width (`None`: automatic).
    BarWidth(Option<Amount>),
    /// Bars on the cursor's line (`None`: automatic).
    BarsPerLine(Option<Amount>),
    /// A new line starts at the cursor bar.
    LineBreak,
    /// The cursor bar goes back onto the previous line.
    LineJoin,
    /// Moves the cursor beat sideways, in tenths of a staff space (`None`: back in place).
    BeatNudge(Option<Amount>),
    /// Discards every adjustment.
    Reset,
}

impl DesignCmd {
    pub fn parse(name: &str, arg: Option<&str>) -> Result<Self, String> {
        let amount = |what: &str| -> Result<Option<Amount>, String> {
            match arg {
                Some("auto" | "reset") => Ok(None),
                Some(a) => Amount::parse(a)
                    .map(Some)
                    .ok_or_else(|| format!("{name}: expected {what}")),
                None => Err(format!("{name}: expected {what}")),
            }
        };
        Ok(match name {
            "design" => DesignCmd::Mode(match arg {
                None | Some("toggle") => None,
                Some("on" | "true" | "yes") => Some(true),
                Some("off" | "false" | "no") => Some(false),
                Some(other) => {
                    return Err(format!("design: expected on, off or toggle, not '{other}'"));
                }
            }),
            "bar-width" => DesignCmd::BarWidth(amount("N, +N, -N (percent) or auto")?),
            "bars-per-line" => DesignCmd::BarsPerLine(amount("N, +N, -N or auto")?),
            "beat-nudge" => DesignCmd::BeatNudge(amount("N, +N, -N (tenths of a space) or reset")?),
            "line-break" => DesignCmd::LineBreak,
            "line-join" => DesignCmd::LineJoin,
            _ => DesignCmd::Reset,
        })
    }
}

impl App {
    /// Design mode is on and the focused pane shows the page layout.
    pub fn design_active(&self) -> bool {
        self.design && self.focused_kind() == PaneKind::Score && self.pane().layout == "page"
    }

    /// The focused score's design overrides.
    pub fn design_overrides(&self) -> DesignOverrides {
        self.buffer().score.design.clone()
    }

    /// The bars of the line the cursor is on in the page layout, as the frontend reported
    /// them; just the cursor's bar when it didn't.
    pub fn design_line(&self) -> std::ops::Range<usize> {
        let bar = self.pane().cursor.bar;
        if self.pane().layout == "page" {
            if let Some(info) = self.view_info.get(&self.focused()) {
                if let Some(row) = info.rows.iter().find(|r| r.contains(&bar)) {
                    return row.clone();
                }
            }
        }
        bar..bar + 1
    }

    /// Records `design` as the score's, as one undo step.
    pub fn set_design(&mut self, design: DesignOverrides) {
        let group = self.ed.group.take();
        self.edit(|cx| {
            *cx.design() = design;
            Ok(())
        });
        self.ed.group = group;
    }

    /// Puts exactly `count` bars on the line that starts at bar `start`, as one undo step
    /// (the "+" and "-" buttons at the end of a line).
    pub fn design_line_bars(&mut self, start: usize, count: usize) {
        let bars = self.buffer().score.master_bars.len();
        if start >= bars {
            return;
        }
        let mut design = self.design_overrides();
        design.set_line_start(start, Some(Some(count.clamp(1, bars - start) as u32)));
        self.set_design(design);
    }

    /// A mouse drag begins: its previews are not undo steps.
    pub fn design_begin(&mut self) {
        if self.design_drag.is_none() {
            self.design_drag = Some(self.buffer().score.design.clone());
        }
    }

    /// Shows `design` while a drag is going on (the score changes, the history does not).
    pub fn design_preview(&mut self, design: DesignOverrides) {
        if self.design_drag.is_none() {
            self.design_begin();
        }
        let index = self.pane().buffer;
        let buffer = &mut self.buffers[index];
        if buffer.score.design != design {
            buffer.score.design = design;
            buffer.version += 1;
        }
    }

    /// The drag ends: whatever it left is one undo step.
    pub fn design_end(&mut self) {
        let Some(original) = self.design_drag.take() else {
            return;
        };
        let index = self.pane().buffer;
        let now = std::mem::replace(&mut self.buffers[index].score.design, original);
        self.set_design(now);
    }

    pub(crate) fn run_design(&mut self, cmd: DesignCmd) {
        if let DesignCmd::Mode(on) = cmd {
            self.design = on.unwrap_or(!self.design);
            let page = self.pane().layout == "page";
            self.info(match (self.design, page) {
                (true, true) => "design mode on",
                (true, false) => "design mode on (it works in the page layout: :layout page)",
                _ => "design mode off",
            });
            return;
        }
        if self.buffer().score.tracks.is_empty() {
            return self.error("no score to adjust");
        }
        let cursor = self.pane().cursor;
        let mut design = self.design_overrides();
        let bars = self.buffer().score.master_bars.len();
        let message = match cmd {
            DesignCmd::BarWidth(amount) => {
                let now = design.width(cursor.bar) as i64 / 10;
                let permille = amount.map_or(AUTO_WIDTH as i64, |a| a.apply(now) * 10);
                let permille = permille.clamp(MIN_WIDTH as i64, MAX_WIDTH as i64) as u32;
                design.set_width(cursor.bar, permille);
                match permille {
                    AUTO_WIDTH => format!("bar {}: automatic width", cursor.bar + 1),
                    p => format!("bar {}: width {}%", cursor.bar + 1, p / 10),
                }
            }
            DesignCmd::BarsPerLine(amount) => {
                let line = self.design_line();
                let start = line.start;
                match amount {
                    None => {
                        design.set_line_start(start, None);
                        format!("line from bar {}: automatic", start + 1)
                    }
                    Some(a) => {
                        let n = a.apply(line.len() as i64).clamp(1, bars as i64) as u32;
                        design.set_line_start(start, Some(Some(n)));
                        format!("line from bar {}: {n} bars", start + 1)
                    }
                }
            }
            DesignCmd::LineBreak => {
                if cursor.bar == 0 {
                    return self.error("line-break: the first bar always starts a line");
                }
                let count = design.line_start(cursor.bar).and_then(|l| l.count);
                design.set_line_start(cursor.bar, Some(count));
                format!("new line at bar {}", cursor.bar + 1)
            }
            DesignCmd::LineJoin => {
                let line = self.design_line();
                if design.line_start(cursor.bar).is_some() {
                    design.set_line_start(cursor.bar, None);
                    format!("bar {} no longer starts a line by decree", cursor.bar + 1)
                } else if line.start == cursor.bar && cursor.bar > 0 {
                    // Pull the bar up: the previous line holds one more.
                    let before = (0..cursor.bar)
                        .rev()
                        .find(|&b| design.line_start(b).is_some())
                        .unwrap_or(0);
                    let prev = self
                        .view_info
                        .get(&self.focused())
                        .and_then(|i| i.rows.iter().find(|r| r.end == cursor.bar).cloned())
                        .unwrap_or(before..cursor.bar);
                    design.set_line_start(prev.start, Some(Some(prev.len() as u32 + 1)));
                    format!("bar {} joins the previous line", cursor.bar + 1)
                } else {
                    return self.error("line-join: the bar is not at the start of a line");
                }
            }
            DesignCmd::BeatNudge(amount) => {
                let (t, b, v, i) = (cursor.track, cursor.bar, cursor.voice, cursor.beat);
                let now = design.offset(t, b, v, i) as i64;
                let tenths = amount.map_or(0, |a| a.apply(now)).clamp(-200, 200) as i32;
                design.set_offset(t, b, v, i, tenths);
                format!(
                    "beat {}.{}: moved {:+.1} spaces",
                    b + 1,
                    i + 1,
                    tenths as f64 / 10.0
                )
            }
            DesignCmd::Reset => {
                design = DesignOverrides::default();
                "page layout back to automatic".to_string()
            }
            DesignCmd::Mode(_) => unreachable!(),
        };
        if design != self.buffer().score.design {
            self.set_design(design);
        }
        self.info(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Config};
    use cecilia_core::*;

    fn app(bars: usize) -> App {
        let mut app = App::new(Config::default(), None);
        app.run(Command::New);
        let b = app.pane().buffer;
        let score = &mut app.buffers[b].score;
        while score.master_bars.len() < bars {
            let mb = score.master_bars[0].clone();
            score.master_bars.push(mb);
            for t in &mut score.tracks {
                let bar = t.bars[0].clone();
                t.bars.push(bar);
            }
        }
        app
    }

    fn design(app: &App) -> DesignOverrides {
        app.buffer().score.design.clone()
    }

    fn run(app: &mut App, line: &str) {
        app.run(Command::parse(line).unwrap());
    }

    #[test]
    fn commands_parse() {
        assert_eq!(
            Command::parse("bar-width +10").unwrap(),
            Command::Design(DesignCmd::BarWidth(Some(Amount::By(10))))
        );
        assert_eq!(
            Command::parse("bar-width auto").unwrap(),
            Command::Design(DesignCmd::BarWidth(None))
        );
        assert_eq!(
            Command::parse("design").unwrap(),
            Command::Design(DesignCmd::Mode(None))
        );
        assert_eq!(
            Command::parse("beat-nudge -3").unwrap(),
            Command::Design(DesignCmd::BeatNudge(Some(Amount::By(-3))))
        );
        assert!(Command::parse("bars-per-line").is_err());
        assert!(Command::parse("design maybe").is_err());
    }

    #[test]
    fn widths_nudges_and_breaks_are_undoable() {
        let mut app = app(6);
        run(&mut app, "bar-width 150");
        assert_eq!(design(&app).width(0), 1500);
        run(&mut app, "bar-width -20");
        assert_eq!(design(&app).width(0), 1300);
        run(&mut app, "bar-width auto");
        assert!(design(&app).is_empty());
        run(&mut app, "u");
        assert_eq!(design(&app).width(0), 1300);
        app.run(Command::Undo);
        assert_eq!(design(&app).width(0), 1500);
        app.run(Command::Redo);
        assert_eq!(design(&app).width(0), 1300);

        run(&mut app, "beat-nudge 4");
        run(&mut app, "beat-nudge +2");
        assert_eq!(design(&app).offset(0, 0, 0, 0), 6);
        run(&mut app, "beat-nudge reset");
        assert_eq!(design(&app).offset(0, 0, 0, 0), 0);

        app.run(Command::BarNext);
        run(&mut app, "line-break");
        assert_eq!(design(&app).line_start(1).unwrap().count, None);
        run(&mut app, "bars-per-line 3");
        assert_eq!(design(&app).line_start(1).unwrap().count, Some(3));
        run(&mut app, "bars-per-line auto");
        assert!(design(&app).line_start(1).is_none());
        run(&mut app, "line-break");
        run(&mut app, "line-join");
        assert!(design(&app).line_starts.is_empty());
        run(&mut app, "design-reset");
        assert!(design(&app).is_empty());
        // The first bar cannot start a line by decree.
        app.run(Command::GotoFirst);
        run(&mut app, "line-break");
        assert!(design(&app).line_starts.is_empty());
    }

    #[test]
    fn lines_follow_the_reported_rows() {
        let mut app = app(8);
        run(&mut app, "layout page");
        let id = app.focused();
        app.set_view_info(
            id,
            Some(crate::ViewInfo {
                rows: vec![0..3, 3..6, 6..8],
                ..Default::default()
            }),
        );
        app.run(Command::GotoBar(5));
        run(&mut app, "bars-per-line 2");
        assert_eq!(design(&app).line_start(3).unwrap().count, Some(2));
        run(&mut app, "bars-per-line +1");
        assert_eq!(design(&app).line_start(3).unwrap().count, Some(4));
        // Joining the first bar of a line to the line above it.
        app.run(Command::GotoBar(7));
        run(&mut app, "design-reset");
        run(&mut app, "line-join");
        assert_eq!(design(&app).line_start(3).unwrap().count, Some(4));
        assert_eq!(design(&app).line_start(0), None);
    }

    #[test]
    fn a_drag_is_one_undo_step() {
        let mut app = app(4);
        app.design_begin();
        for w in [1100, 1200, 1300] {
            let mut d = design(&app);
            d.set_width(1, w);
            app.design_preview(d);
        }
        assert_eq!(design(&app).width(1), 1300);
        assert!(!app.buffer().history.can_undo());
        app.design_end();
        assert!(app.buffer().modified);
        app.run(Command::Undo);
        assert!(design(&app).is_empty());
        assert!(!app.buffer().history.can_undo());
        app.run(Command::Redo);
        assert_eq!(design(&app).width(1), 1300);
        // A drag that changed nothing records nothing.
        app.design_begin();
        app.design_end();
        app.run(Command::Undo);
        assert!(design(&app).is_empty());
    }

    #[test]
    fn mode_needs_the_page_layout_and_has_keys() {
        let mut app = app(2);
        assert!(!app.design_active());
        run(&mut app, "design on");
        assert!(app.design && !app.design_active());
        run(&mut app, "layout page");
        assert!(app.design_active());
        let keys = crate::keys::parse_keys(">", &[]).unwrap();
        for k in keys {
            app.handle_key(k);
        }
        assert_eq!(design(&app).width(0), 1100);
        // The status line says so.
        assert!(
            app.statusline(None, false)
                .left
                .iter()
                .any(|s| s.text == "design")
        );
        for k in crate::keys::parse_keys("<leader>d", &app.config.leader).unwrap() {
            app.handle_key(k);
        }
        assert!(!app.design);
        // The keys do nothing outside design mode.
        for k in crate::keys::parse_keys(">", &[]).unwrap() {
            app.handle_key(k);
        }
        assert_eq!(design(&app).width(0), 1100);
    }

    #[test]
    fn bar_edits_shift_the_design() {
        let mut app = app(4);
        app.run(Command::GotoBar(3));
        run(&mut app, "bar-width 130");
        app.run(Command::GotoFirst);
        app.run(Command::BarInsert { after: false });
        assert_eq!(design(&app).width(3), 1300);
        assert_eq!(design(&app).width(2), AUTO_WIDTH);
        app.run(Command::Undo);
        assert_eq!(design(&app).width(2), 1300);
        app.run(Command::Redo);
        app.run(Command::GotoFirst);
        app.run(Command::BarDelete);
        assert_eq!(design(&app).width(2), 1300);
    }

    #[test]
    fn design_survives_the_native_format() {
        let mut app = app(3);
        run(&mut app, "bar-width 120");
        run(&mut app, "beat-nudge 5");
        app.run(Command::BarNext);
        app.run(Command::BarNext);
        run(&mut app, "line-break");
        let score = app.buffer().score.clone();
        let bytes = cecilia_formats::native::write_cecilia(&score).unwrap();
        let back = cecilia_formats::native::read_cecilia(&bytes).unwrap();
        assert_eq!(back.design, score.design);
        assert!(!back.design.is_empty());
        // A score without any writes none.
        let plain = cecilia_formats::native::write_cecilia(&Score::default()).unwrap();
        assert!(
            cecilia_formats::native::read_cecilia(&plain)
                .unwrap()
                .design
                .is_empty()
        );
    }

    #[test]
    fn design_keys_map_to_the_six_actions_only_in_design_mode() {
        let mut app = app(8);
        run(&mut app, "layout page");
        let id = app.focused();
        app.set_view_info(
            id,
            Some(crate::ViewInfo {
                rows: vec![0..3, 3..6, 6..8],
                ..Default::default()
            }),
        );
        let press = |app: &mut App, keys: &str| {
            for k in crate::keys::parse_keys(keys, &app.config.leader).unwrap() {
                app.handle_key(k);
            }
        };
        // Outside design mode they do nothing to the layout, and + / - keep their meaning.
        press(&mut app, "+>");
        assert!(design(&app).is_empty());
        run(&mut app, "design on");
        press(&mut app, "+");
        assert_eq!(design(&app).line_start(0).unwrap().count, Some(4));
        press(&mut app, "--");
        assert_eq!(design(&app).line_start(0).unwrap().count, Some(2));
        press(&mut app, "<A-=>");
        assert!(design(&app).line_starts.is_empty());
        press(&mut app, ">>");
        assert_eq!(design(&app).width(0), 1200);
        press(&mut app, "<");
        assert_eq!(design(&app).width(0), 1100);
        press(&mut app, "=");
        assert!(design(&app).bar_widths.is_empty());
        press(&mut app, "<A-l><A-l><A-h>");
        assert_eq!(design(&app).offset(0, 0, 0, 0), 2);
        press(&mut app, "<A-0>");
        assert!(design(&app).beat_offsets.is_empty());
        // Back out of design mode, + / - are the duration keys again.
        run(&mut app, "design off");
        press(&mut app, "-");
        assert!(design(&app).is_empty());
    }

    #[test]
    fn line_buttons_set_the_bars_of_a_line() {
        let mut app = app(8);
        app.design_line_bars(3, 4);
        assert_eq!(design(&app).line_start(3).unwrap().count, Some(4));
        app.design_line_bars(3, 99);
        assert_eq!(design(&app).line_start(3).unwrap().count, Some(5));
        app.run(Command::Undo);
        assert_eq!(design(&app).line_start(3).unwrap().count, Some(4));
        app.run(Command::Undo);
        assert!(design(&app).is_empty());
    }
}
