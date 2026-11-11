//! The text viewer: a popup in the middle of the window showing a long text (a notification from
//! the history) as a read-only text area. It behaves like the lyrics text area without insert
//! and replace mode: normal mode moves the caret, `v` and `V` select, `y` copies, `<Esc>` closes,
//! and the mouse places the caret, drags a selection and closes it with the "x" in the corner.

use std::cell::{Cell, RefCell};

use crate::app::App;
use crate::config::Mode;
use crate::keys::{Key, KeyCode};
use crate::notify::NoticeIcon;
use crate::properties::{Field, Motion, PropEdit, Target, Typed, caret_in, parse_typed};
use crate::textobj;

/// The popup's state: the heading and the text, held in the lyrics text area's editing type so
/// the caret, the selection and the motions are the same.
#[derive(Debug)]
pub struct Viewer {
    pub subject: String,
    pub icon: NoticeIcon,
    pub error: bool,
    pub edit: PropEdit,
    /// The lines the text is wrapped to as last drawn (character ranges), for moving by visual
    /// line.
    pub lines: RefCell<Vec<(usize, usize)>>,
    /// The first line shown, and the caret when it last followed it.
    scroll: Cell<usize>,
    seen: Cell<usize>,
    /// How far the popup was dragged from the middle (the frontend's units, clamped by it as
    /// it draws).
    pub offset: Cell<(f64, f64)>,
    /// The mode to go back to when it closes (while it is open, selecting is Visual mode).
    back: Mode,
}

impl Viewer {
    pub fn new(subject: String, text: String, icon: NoticeIcon, error: bool, back: Mode) -> Viewer {
        let mut edit = PropEdit::new(Field::Lyrics, text);
        edit.insert = false;
        edit.caret = 0;
        Viewer {
            subject,
            icon,
            error,
            edit,
            lines: RefCell::new(Vec::new()),
            scroll: Cell::new(0),
            seen: Cell::new(usize::MAX),
            offset: Cell::new((0.0, 0.0)),
            back,
        }
    }

    /// The first line to draw when `shown` lines fit: the wheel's position, moved when the caret
    /// has left the window since the last draw.
    pub fn window(&self, total: usize, shown: usize) -> usize {
        let mut skip = self.scroll.get().min(total.saturating_sub(shown));
        let caret = self.edit.caret;
        if caret != self.seen.get() {
            self.seen.set(caret);
            let row = caret_in(&self.lines.borrow(), caret).0;
            if row < skip {
                skip = row;
            } else if row >= skip + shown {
                skip = row + 1 - shown;
            }
        }
        self.scroll.set(skip);
        skip
    }

    /// The wheel: scrolls `by` lines (the next draw clamps it).
    pub fn scroll_by(&self, by: isize) {
        self.scroll
            .set((self.scroll.get() as isize + by).max(0) as usize);
    }

    /// A key in normal or visual mode. False: `<Esc>` with nothing left to leave, so close.
    fn key(&mut self, key: Key) -> bool {
        let lines = self.lines.borrow().clone();
        let e = &mut self.edit;
        let plain = !key.ctrl && !key.alt && !key.sup;
        let visual = e.visual.is_some();
        let ctrl = |c: char| key.ctrl && key.code == KeyCode::Char(c);
        match key.code {
            KeyCode::Esc if !e.pending.is_empty() => {
                e.pending.clear();
                return true;
            }
            KeyCode::Esc if visual || e.selection().is_some() => {
                e.visual = None;
                e.anchor = None;
                return true;
            }
            KeyCode::Esc => return false,
            _ if ctrl('a') => {
                e.select_all();
                return true;
            }
            _ if ctrl('c') => {
                if let Some(t) = e.selected_text() {
                    crate::clipboard::set(&t);
                }
                return true;
            }
            _ => {}
        }
        let typed = match key.code {
            // With text selected by the mouse, `y` copies it at once.
            KeyCode::Char('y')
                if plain && !visual && e.pending.is_empty() && e.selection().is_some() =>
            {
                Typed::Cmd('y')
            }
            KeyCode::Char(c) if plain => {
                let mut keys: Vec<char> = e.pending.chars().collect();
                keys.push(c);
                let t = parse_typed(&keys, visual);
                e.pending = if t == Typed::More {
                    keys.into_iter().collect()
                } else {
                    String::new()
                };
                t
            }
            KeyCode::Left if key.ctrl => Typed::Move(Motion::Back(false)),
            KeyCode::Right if key.ctrl => Typed::Move(Motion::Word(false)),
            KeyCode::Left => Typed::Move(Motion::Left),
            KeyCode::Right => Typed::Move(Motion::Right),
            KeyCode::Up => Typed::Move(Motion::Up),
            KeyCode::Down => Typed::Move(Motion::Down),
            KeyCode::Home => Typed::Move(Motion::LineStart),
            KeyCode::End => Typed::Move(Motion::LineEnd),
            _ => Typed::Bad,
        };
        let range = e
            .selection()
            .map(|(a, b)| (a, b, e.visual.is_some_and(|v| v.1)));
        match typed {
            Typed::Move(m) => {
                // A selection made with the mouse ends with the first key that moves.
                e.anchor = None;
                e.caret = e.target(m, &lines);
                e.rest();
            }
            Typed::Object(around, o) if visual => {
                let linewise = e.visual.is_some_and(|v| v.1);
                if let Some((a, b, whole)) = e.object_range(around, o).filter(|r| r.0 < r.1) {
                    e.visual = Some((a, linewise || whole));
                    e.caret = b - 1;
                }
            }
            // Only `y` of the operators works on a read-only text.
            Typed::Op('y', target) => {
                let r = match target {
                    Target::Line => {
                        let (a, b) = textobj::line_range(&e.chars(), e.caret, e.caret);
                        Some((a, b, true))
                    }
                    Target::Move(m) => e.motion_range('y', m),
                    Target::Object(around, o) => e.object_range(around, o),
                };
                if let Some(r) = r {
                    yank(e, r);
                }
            }
            Typed::Cmd(c) => match c {
                'y' => {
                    if let Some(r) = range {
                        yank(e, r);
                    }
                }
                'Y' => {
                    let (a, b) = textobj::line_range(&e.chars(), e.caret, e.caret);
                    yank(e, (a, b, true));
                }
                'v' | 'V' => {
                    let lines = c == 'V';
                    e.anchor = None;
                    e.visual = match e.visual {
                        Some((start, was)) => (was != lines).then_some((start, lines)),
                        None => Some((e.caret, lines)),
                    };
                }
                'o' if visual => {
                    if let Some((start, lines)) = e.visual {
                        e.visual = Some((e.caret, lines));
                        e.caret = start;
                    }
                }
                _ => {}
            },
            _ => {}
        }
        e.caret = e.caret.min(e.len());
        true
    }
}

/// Copies a range (from, to, whole lines) and leaves the selection.
fn yank(e: &mut PropEdit, (a, b, lines): (usize, usize, bool)) {
    let c = e.chars();
    let (a, b) = (a.min(c.len()), b.min(c.len()));
    let mut taken: String = c[a..b].iter().collect();
    if lines && !taken.ends_with('\n') {
        taken.push('\n');
    }
    if !taken.is_empty() {
        crate::clipboard::set(&taken);
    }
    e.anchor = None;
    e.visual = None;
    if !lines || textobj::line_bounds(&c, e.caret).0 != a {
        e.caret = a;
    }
    e.rest();
}

impl App {
    /// Shows a notification of the history in a popup (by its key).
    pub(crate) fn notify_view(&mut self, key: u64) {
        let Some(e) = self.notifications.history().iter().find(|e| e.key == key) else {
            return;
        };
        self.viewer = Some(Viewer::new(
            e.subject.clone(),
            e.body().to_string(),
            e.icon,
            e.error,
            self.mode,
        ));
    }

    pub fn viewer(&self) -> Option<&Viewer> {
        self.viewer.as_ref()
    }

    /// Closes the viewer, back to the mode it was opened from.
    pub fn close_viewer(&mut self) {
        if let Some(v) = self.viewer.take() {
            self.mode = v.back;
        }
    }

    /// The mode follows the selection: Visual while text is selected, else the mode before.
    fn viewer_sync_mode(&mut self) {
        if let Some(v) = &self.viewer {
            let selecting = v.edit.visual.is_some() || v.edit.selection().is_some();
            self.mode = if selecting { Mode::Visual } else { v.back };
        }
    }

    /// A key while the viewer is open: it takes every key.
    pub(crate) fn viewer_key(&mut self, key: Key) {
        if self.viewer.as_mut().is_some_and(|v| !v.key(key)) {
            self.close_viewer();
        }
        self.viewer_sync_mode();
    }

    /// Drags the popup by a distance (the frontend's units).
    pub fn viewer_move(&mut self, dx: f64, dy: f64) {
        if let Some(v) = &self.viewer {
            let (x, y) = v.offset.get();
            v.offset.set((x + dx, y + dy));
        }
    }

    /// A triple click selects the whole text.
    pub fn viewer_triple_click(&mut self) {
        if let Some(v) = self.viewer.as_mut() {
            v.edit.select_all();
        }
        self.viewer_sync_mode();
    }

    /// A press in the text: the caret goes there and a selection may begin.
    pub fn viewer_click(&mut self, at: usize) {
        if let Some(e) = self.viewer.as_mut().map(|v| &mut v.edit) {
            e.caret = at.min(e.len());
            e.anchor = Some(e.caret);
            e.visual = None;
            e.pending.clear();
            e.rest();
        }
        self.viewer_sync_mode();
    }

    /// A double click selects the word.
    pub fn viewer_double_click(&mut self, at: usize) {
        if let Some(e) = self.viewer.as_mut().map(|v| &mut v.edit) {
            e.visual = None;
            e.select_word(at);
        }
        self.viewer_sync_mode();
    }

    /// The pointer moves with the button down: the selection grows to `at`.
    pub fn viewer_drag(&mut self, at: usize) {
        if let Some(e) = self.viewer.as_mut().map(|v| &mut v.edit) {
            e.caret = at.min(e.len());
        }
        self.viewer_sync_mode();
    }

    /// The button comes up: a press without a drag selects nothing.
    pub fn viewer_release(&mut self) {
        if let Some(e) = self.viewer.as_mut().map(|v| &mut v.edit)
            && e.anchor == Some(e.caret)
        {
            e.anchor = None;
        }
        self.viewer_sync_mode();
    }
}

#[cfg(test)]
mod tests {
    use crate::app::tests::keys;
    use crate::{App, Command, Config};

    fn open(text: &str) -> App {
        let mut app = App::new(Config::default(), None);
        app.run(Command::parse(&format!("notify -s Head {text}")).unwrap());
        app.notifications.dismiss_all();
        let key = app.notifications.history()[0].key;
        app.run(Command::NotifyView(key));
        app
    }

    #[test]
    fn a_history_entry_opens_in_a_popup_instead_of_a_card() {
        let app = open("one two three");
        let v = app.viewer().unwrap();
        assert_eq!(
            (v.subject.as_str(), v.edit.text.as_str()),
            ("Head", "one two three")
        );
        assert!(app.notifications_shown().0.is_empty());
    }

    #[test]
    fn normal_mode_moves_selects_and_yanks_but_never_edits() {
        let mut app = open("one two three");
        keys(&mut app, "wvey");
        assert_eq!(crate::clipboard::get(), "two");
        // The editing keys do nothing, and there is no insert mode.
        keys(&mut app, "iAxdwxp~");
        let v = app.viewer().unwrap();
        assert_eq!(v.edit.text, "one two three");
        assert!(!v.edit.insert);
        keys(&mut app, "0yw");
        assert_eq!(crate::clipboard::get(), "one ");
        keys(&mut app, "yy");
        assert_eq!(crate::clipboard::get(), "one two three\n");
        keys(&mut app, "<C-a><C-c>");
        assert_eq!(crate::clipboard::get(), "one two three");
    }

    #[test]
    fn escape_leaves_the_selection_then_closes() {
        let mut app = open("one two");
        keys(&mut app, "v");
        keys(&mut app, "<Esc>");
        assert!(app.viewer().is_some_and(|v| v.edit.visual.is_none()));
        keys(&mut app, "<Esc>");
        assert!(app.viewer().is_none());
    }

    #[test]
    fn the_mouse_places_the_caret_and_selects() {
        let mut app = open("one two three");
        app.viewer_click(4);
        app.viewer_drag(7);
        app.viewer_release();
        assert_eq!(app.viewer().unwrap().edit.selection(), Some((4, 7)));
        keys(&mut app, "y");
        assert_eq!(crate::clipboard::get(), "two");
        app.viewer_double_click(9);
        assert_eq!(
            app.viewer().unwrap().edit.selected_text().as_deref(),
            Some("three")
        );
        app.viewer_click(2);
        app.viewer_release();
        assert!(app.viewer().unwrap().edit.selection().is_none());
        app.close_viewer();
        assert!(app.viewer().is_none());
    }

    #[test]
    fn selecting_is_visual_mode_and_closing_restores_the_mode() {
        let mut app = open("one two three");
        let before = app.mode;
        keys(&mut app, "v");
        assert_eq!(app.mode, crate::Mode::Visual);
        keys(&mut app, "<Esc>");
        assert_eq!(app.mode, before);
        app.viewer_click(0);
        app.viewer_drag(3);
        assert_eq!(app.mode, crate::Mode::Visual);
        app.viewer_triple_click();
        assert_eq!(
            app.viewer().unwrap().edit.selected_text().as_deref(),
            Some("one two three")
        );
        keys(&mut app, "v<Esc><Esc>");
        assert!(app.viewer().is_none());
        assert_eq!(app.mode, before);
    }

    #[test]
    fn the_popup_can_be_moved() {
        let mut app = open("one");
        app.viewer_move(3.0, -2.0);
        app.viewer_move(1.0, 0.0);
        assert_eq!(app.viewer().unwrap().offset.get(), (4.0, -2.0));
    }

    #[test]
    fn the_viewer_takes_the_keys_the_editor_would() {
        let mut app = open("one two");
        keys(&mut app, ":q<CR>");
        assert!(app.viewer().is_some(), "`:` is not a command line here");
    }
}
