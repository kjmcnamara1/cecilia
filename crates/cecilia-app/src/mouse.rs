//! Selecting with the mouse: press on a beat, drag to others, release.
//!
//! The frontends resolve pointer positions to a [`Cursor`] (the beat under the pointer) and
//! call [`App::mouse_press`], [`App::mouse_drag`] and [`App::mouse_release`]; the state
//! machine, the threshold and the mode switching live here.
//!
//! The Visual selection works on the cursor's own track (and voice): its ends are bar and beat
//! positions. So a drag moves only the bar and beat of the cursor and keeps the track, staff
//! and string where the press put them; pointing at another track's staff selects the same
//! beats of the pressed track.

use crate::app::{App, Cursor};
use crate::command::Command;
use crate::config::Mode;
use crate::layout::PaneId;

/// What the press asked for beyond placing the cursor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseMods {
    /// Shift: extend the selection to the beat (Visual mode is entered if not in it).
    pub extend: bool,
    /// Alt or Ctrl: select whole bars (as `V`).
    pub bars: bool,
    /// A double click: selects the bar.
    pub double: bool,
}

/// A press that may become a drag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseSel {
    pub pane: PaneId,
    /// Where the press put the cursor: the selection's anchor once the drag starts.
    pub press: Cursor,
    pub bars: bool,
    /// The drag has left the pressed beat (or the press itself selected).
    pub active: bool,
}

impl App {
    /// A press of the left button on a beat of a score pane: `at` is the cursor for the beat
    /// (and string or line) under the pointer.
    ///
    /// A plain press places the cursor, as a click always did; in Visual mode that leaves it
    /// (a drag from the press then selects afresh). A drag then selects. With `extend` the selection runs from the
    /// anchor (the cursor, when not in Visual mode) to the beat; a double click selects the bar.
    pub fn mouse_press(&mut self, pane: PaneId, at: Cursor, mods: MouseMods) {
        self.mouse_sel = None;
        self.set_focus(pane);
        let Some(p) = self.panes.get(&pane) else {
            return;
        };
        let base = p.cursor;
        let can_select = matches!(self.mode, Mode::Normal | Mode::Insert | Mode::Visual);
        if mods.extend && can_select {
            if self.mode != Mode::Visual || self.ed.visual_anchor.is_none() {
                self.begin_visual(base, mods.bars);
            } else if mods.bars {
                self.ed.visual_bars = true;
            }
            self.set_cursor_beat(pane, at);
            self.mouse_sel = Some(MouseSel {
                pane,
                press: self.ed.visual_anchor.unwrap_or(base),
                bars: self.ed.visual_bars,
                active: true,
            });
            return;
        }
        if self.mode == Mode::Visual {
            self.run(Command::Normal);
        }
        self.set_cursor(pane, at);
        let press = self.panes[&pane].cursor;
        if mods.double && can_select {
            self.begin_visual(press, true);
        }
        if can_select {
            self.mouse_sel = Some(MouseSel {
                pane,
                press,
                bars: mods.bars || mods.double,
                active: mods.double,
            });
        }
    }

    /// The pointer moved with the button down, now over the beat of `at` (only its bar and beat
    /// count). Once it leaves the pressed beat, Visual mode starts with the anchor there.
    /// Returns whether anything changed.
    pub fn mouse_drag(&mut self, at: Cursor) -> bool {
        let Some(mut sel) = self.mouse_sel else {
            return false;
        };
        if !self.panes.contains_key(&sel.pane) || self.focused() != sel.pane {
            self.mouse_sel = None;
            return false;
        }
        if !sel.active {
            if (at.bar, at.beat) == (sel.press.bar, sel.press.beat) {
                return false;
            }
            self.begin_visual(sel.press, sel.bars);
            sel.active = true;
            self.mouse_sel = Some(sel);
        } else if self.mode != Mode::Visual {
            // Left Visual mode (Esc) during the drag: the rest of it does nothing.
            self.mouse_sel = None;
            return false;
        }
        let before = self.panes[&sel.pane].cursor;
        self.set_cursor_beat(sel.pane, at);
        self.panes[&sel.pane].cursor != before || !sel.active
    }

    /// The button went up: the selection stays (Visual mode), the drag is over.
    pub fn mouse_release(&mut self) {
        self.mouse_sel = None;
    }

    /// A mouse selection is being dragged (a press on a score that is not yet released).
    pub fn mouse_selecting(&self) -> Option<PaneId> {
        self.mouse_sel.map(|s| s.pane)
    }

    /// The track a mouse drag started on (drags select within it).
    pub fn mouse_track(&self) -> Option<usize> {
        self.mouse_sel.map(|s| s.press.track)
    }

    fn begin_visual(&mut self, anchor: Cursor, bars: bool) {
        if self.mode == Mode::Insert {
            self.run(Command::Normal);
        }
        self.pending.clear();
        self.count = None;
        self.ed.awaiting = None;
        self.ed.visual_anchor = Some(anchor);
        self.ed.visual_bars = bars;
        self.mode = Mode::Visual;
    }

    fn set_cursor(&mut self, pane: PaneId, at: Cursor) {
        if let Some(p) = self.panes.get_mut(&pane) {
            p.cursor = at;
        }
        self.normalize_cursor(pane);
    }

    /// Moves a pane's cursor to the bar and beat of `at`, on the track and line it is on.
    fn set_cursor_beat(&mut self, pane: PaneId, at: Cursor) {
        if let Some(p) = self.panes.get_mut(&pane) {
            p.cursor.bar = at.bar;
            p.cursor.beat = at.beat;
        }
        self.normalize_cursor(pane);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{keys, test_app};

    fn app() -> App {
        let mut app = test_app();
        app.run(Command::New);
        // Beats in bar 1: a few notes.
        keys(&mut app, "i1<Space>l2<Space>l3<Space>l4<Space>l5<Esc>");
        app
    }

    fn at(app: &App, bar: usize, beat: usize) -> Cursor {
        Cursor {
            bar,
            beat,
            ..app.pane().cursor
        }
    }

    #[test]
    fn a_plain_click_moves_the_cursor_and_does_not_select() {
        let mut app = app();
        let pane = app.focused();
        let c = at(&app, 0, 2);
        app.mouse_press(pane, c, MouseMods::default());
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.pane().cursor.beat, 2);
        // Wiggling inside the same beat is still a click.
        assert!(!app.mouse_drag(c));
        app.mouse_release();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.selection().is_none());
    }

    #[test]
    fn dragging_to_another_beat_starts_a_visual_selection_and_follows() {
        let mut app = app();
        let pane = app.focused();
        let c = at(&app, 0, 1);
        app.mouse_press(pane, c, MouseMods::default());
        assert!(app.mouse_drag(at(&app, 0, 3)));
        let (a, b, bars) = app.selection().unwrap();
        assert_eq!((a.beat, b.beat, bars), (1, 3, false));
        assert_eq!(app.mode, Mode::Visual);
        // Back past the anchor, and on into another bar.
        assert!(app.mouse_drag(at(&app, 0, 0)));
        assert_eq!(app.selection().unwrap().1.beat, 0);
        assert!(!app.mouse_drag(at(&app, 0, 0)));
        app.mouse_release();
        // The selection stays after the release, and further moves do nothing.
        assert_eq!(app.mode, Mode::Visual);
        assert!(!app.mouse_drag(at(&app, 0, 3)));
        assert_eq!(app.selection().unwrap().0.beat, 1);
    }

    #[test]
    fn a_drag_selects_across_bars_on_the_pressed_track() {
        let mut app = app();
        let pane = app.focused();
        app.run(Command::BarInsert { after: true });
        app.run(Command::Normal);
        let press = at(&app, 0, 0);
        app.mouse_press(pane, press, MouseMods::default());
        // Pointing at another track keeps the selection on the pressed one.
        let other = Cursor {
            track: 1,
            ..at(&app, 1, 0)
        };
        app.mouse_drag(other);
        let (a, b, _) = app.selection().unwrap();
        assert_eq!((a.bar, b.bar), (0, 1));
        assert_eq!(b.track, 0);
    }

    #[test]
    fn dragging_from_visual_mode_re_anchors_at_the_press() {
        let mut app = app();
        let pane = app.focused();
        keys(&mut app, "0vll");
        assert_eq!(app.selection().unwrap().0.beat, 0);
        // A plain click in Visual mode moves the cursor and returns to Normal mode.
        app.mouse_press(pane, at(&app, 0, 3), MouseMods::default());
        app.mouse_release();
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.selection().is_none());
        assert_eq!(app.pane().cursor.beat, 3);
        // A drag from a beat starts afresh there.
        app.mouse_press(pane, at(&app, 0, 2), MouseMods::default());
        app.mouse_drag(at(&app, 0, 1));
        let (a, b, _) = app.selection().unwrap();
        assert_eq!((a.beat, b.beat), (2, 1));
    }

    #[test]
    fn shift_click_extends_from_the_cursor_and_enters_visual() {
        let mut app = app();
        let pane = app.focused();
        app.run(Command::Normal);
        keys(&mut app, "0");
        let shift = MouseMods {
            extend: true,
            ..Default::default()
        };
        app.mouse_press(pane, at(&app, 0, 3), shift);
        let (a, b, bars) = app.selection().unwrap();
        assert_eq!((a.beat, b.beat, bars), (0, 3, false));
        // A second shift-click moves the end, the anchor stays; dragging goes on from there.
        app.mouse_press(pane, at(&app, 0, 1), shift);
        app.mouse_drag(at(&app, 0, 3));
        app.mouse_release();
        let (a, b, _) = app.selection().unwrap();
        assert_eq!((a.beat, b.beat), (0, 3));
    }

    #[test]
    fn bar_wise_drags_and_double_clicks_select_bars() {
        let mut app = app();
        let pane = app.focused();
        let bars = MouseMods {
            bars: true,
            ..Default::default()
        };
        app.mouse_press(pane, at(&app, 0, 1), bars);
        app.mouse_drag(at(&app, 0, 2));
        assert!(app.selection().unwrap().2);
        app.mouse_release();
        app.run(Command::Normal);
        let double = MouseMods {
            double: true,
            ..Default::default()
        };
        app.mouse_press(pane, at(&app, 0, 2), double);
        app.mouse_release();
        assert!(app.selection().unwrap().2);
        assert_eq!(app.mode, Mode::Visual);
    }

    #[test]
    fn a_drag_from_insert_mode_leaves_it_for_visual() {
        let mut app = app();
        let pane = app.focused();
        keys(&mut app, "i");
        assert_eq!(app.mode, Mode::Insert);
        app.mouse_press(pane, at(&app, 0, 0), MouseMods::default());
        app.mouse_drag(at(&app, 0, 2));
        assert_eq!(app.mode, Mode::Visual);
    }

    #[test]
    fn escape_during_a_drag_ends_it() {
        let mut app = app();
        let pane = app.focused();
        app.mouse_press(pane, at(&app, 0, 0), MouseMods::default());
        app.mouse_drag(at(&app, 0, 2));
        keys(&mut app, "<Esc>");
        assert_eq!(app.mode, Mode::Normal);
        assert!(!app.mouse_drag(at(&app, 0, 3)));
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn the_selection_can_be_operated_on_after_the_release() {
        let mut app = app();
        let pane = app.focused();
        app.mouse_press(pane, at(&app, 0, 1), MouseMods::default());
        app.mouse_drag(at(&app, 0, 2));
        app.mouse_release();
        keys(&mut app, "y");
        assert_eq!(app.mode, Mode::Normal);
    }
}
