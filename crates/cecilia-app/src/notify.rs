//! Notifications: the messages of [`App::info`] and [`App::error`] as toast cards the frontends
//! draw over the score panes (or the window), stacked, merged when repeated, timed out by
//! severity, and kept in a persistent history for the `:notifications` picker.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::config::NotificationsConfig;
use crate::layout::Rect;

/// The kind of thing a notification is about; drawn as its icon, colored by severity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeIcon {
    #[default]
    Info,
    Error,
    Success,
    Command,
    Workspace,
    File,
    Option,
    Audio,
}

impl NoticeIcon {
    pub const NAMES: &'static [&'static str] = &[
        "info",
        "error",
        "success",
        "command",
        "workspace",
        "file",
        "option",
        "audio",
    ];

    pub fn parse(s: &str) -> Option<NoticeIcon> {
        Some(match s {
            "info" => NoticeIcon::Info,
            "error" => NoticeIcon::Error,
            "success" | "ok" => NoticeIcon::Success,
            "command" => NoticeIcon::Command,
            "workspace" => NoticeIcon::Workspace,
            "file" => NoticeIcon::File,
            "option" | "setting" => NoticeIcon::Option,
            "audio" | "sound" => NoticeIcon::Audio,
            _ => return None,
        })
    }

    /// A character for the terminal.
    pub fn glyph(self) -> &'static str {
        match self {
            NoticeIcon::Info => "ℹ",
            NoticeIcon::Error => "✖",
            NoticeIcon::Success => "✔",
            NoticeIcon::Command => "❯",
            NoticeIcon::Workspace => "▣",
            NoticeIcon::File => "▤",
            NoticeIcon::Option => "⚙",
            NoticeIcon::Audio => "♪",
        }
    }
}

/// A notification to raise. Only the text is needed; the rest is worked out from it.
#[derive(Debug, Clone, Default)]
pub struct Notice {
    pub text: String,
    pub error: bool,
    pub subject: Option<String>,
    pub icon: Option<NoticeIcon>,
    /// Seconds on screen (0: until dismissed); the severity's timeout when unset.
    pub timeout: Option<f64>,
}

impl Notice {
    pub fn new(text: impl Into<String>, error: bool) -> Notice {
        Notice {
            text: text.into(),
            error,
            ..Notice::default()
        }
    }
}

/// A card on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: u64,
    pub subject: String,
    /// The message as given.
    pub text: String,
    /// Where the body starts in `text`: a `subject: ` prefix is the heading, not the body.
    pub body_at: usize,
    pub error: bool,
    pub icon: NoticeIcon,
    /// A question waiting for a key: stays until answered, never merges, not in the history.
    pub prompt: bool,
    /// The buttons of a prompt: a label and the key a click on it stands for.
    pub choices: Vec<(&'static str, crate::keys::KeyCode)>,
    /// How many notifications this card stands for (repeats are merged into it).
    pub count: u32,
    shown: Instant,
    timeout: Option<Duration>,
}

impl Notification {
    fn expired(&self, now: Instant) -> bool {
        self.timeout
            .is_some_and(|t| now.saturating_duration_since(self.shown) >= t)
    }

    /// The message under the heading.
    pub fn body(&self) -> &str {
        self.text.get(self.body_at..).unwrap_or(&self.text)
    }

    /// The share of the timeout still left, 1 down to 0 (`None` for a card that stays).
    pub fn remaining_fraction(&self, now: Instant) -> Option<f32> {
        let t = self.timeout?;
        let gone = now.saturating_duration_since(self.shown).as_secs_f32();
        Some((1.0 - gone / t.as_secs_f32()).clamp(0.0, 1.0))
    }
}

/// One line of the history: a card, with its repeats counted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Names the card (lines with the same key are the same entry, the last one counts).
    #[serde(default)]
    pub key: u64,
    /// Seconds since the Unix epoch, of the latest repeat.
    pub ts: u64,
    pub error: bool,
    #[serde(default)]
    pub subject: String,
    pub text: String,
    #[serde(default)]
    pub body_at: usize,
    #[serde(default)]
    pub icon: NoticeIcon,
    #[serde(default = "one")]
    pub count: u32,
}

fn one() -> u32 {
    1
}

impl HistoryEntry {
    /// The message under the heading.
    pub fn body(&self) -> &str {
        self.text.get(self.body_at..).unwrap_or(&self.text)
    }
}

/// The cards on screen, and the history.
#[derive(Debug, Default)]
pub struct Notifications {
    /// Oldest first.
    visible: Vec<Notification>,
    next_id: u64,
    history: Vec<HistoryEntry>,
    /// Where the history is kept between sessions (set by the frontends, never by tests).
    history_file: Option<PathBuf>,
    errors: u64,
}

/// Digit runs stand for any number, so "bar 3 deleted" and "bar 4 deleted" are alike.
fn template(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut digits = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !digits {
                out.push('#');
            }
            digits = true;
        } else {
            digits = false;
            out.push(c);
        }
    }
    out
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn title_case(s: &str) -> String {
    s.split([' ', '-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A heading, the offset of the body and an icon for a message nobody labelled: a `topic: `
/// prefix is the heading ("workspace saved: x" → "Workspace Saved"), else a general one.
fn classify(text: &str, error: bool) -> (String, usize, NoticeIcon) {
    let prefix = text.split_once(": ").filter(|(p, rest)| {
        !rest.is_empty()
            && p.len() <= 28
            && p.starts_with(|c: char| c.is_ascii_alphabetic())
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_'))
    });
    let lower = text.to_lowercase();
    let head = prefix.map_or(lower.as_str(), |(p, _)| p);
    let icon = if head.contains("command") {
        NoticeIcon::Command
    } else if head.contains("workspace") {
        NoticeIcon::Workspace
    } else if prefix.is_none() && lower.contains(" = ") || matches!(head, "set" | "get") {
        NoticeIcon::Option
    } else if [
        "saved", "written", "wrote", "opened", "file", "reload", "recover",
    ]
    .iter()
    .any(|k| head.contains(k))
    {
        NoticeIcon::File
    } else if ["audio", "tempo", "sync", "play", "metronome", "speed"]
        .iter()
        .any(|k| head.contains(k))
    {
        NoticeIcon::Audio
    } else if error {
        NoticeIcon::Error
    } else {
        NoticeIcon::Info
    };
    match prefix {
        Some((p, _)) => (title_case(p), p.len() + 2, icon),
        None => {
            let subject = if icon == NoticeIcon::Option {
                "Option"
            } else if error {
                "Error"
            } else {
                "Notice"
            };
            (subject.to_string(), 0, icon)
        }
    }
}

impl Notifications {
    fn timeout(cfg: &NotificationsConfig, error: bool) -> Option<Duration> {
        let secs = if error {
            cfg.error_timeout
        } else {
            cfg.info_timeout
        };
        Self::secs(secs)
    }

    fn secs(secs: f64) -> Option<Duration> {
        (secs > 0.0 && secs.is_finite()).then(|| Duration::from_secs_f64(secs.min(3600.0)))
    }

    /// Adds a notification of just a text and a severity.
    pub fn push(&mut self, text: String, error: bool, now: Instant, cfg: &NotificationsConfig) {
        self.raise(Notice::new(text, error), now, cfg);
    }

    /// Adds a notification, merging it into a similar card still on screen.
    pub fn raise(&mut self, n: Notice, now: Instant, cfg: &NotificationsConfig) {
        self.expire(now);
        if n.error {
            self.errors += 1;
        }
        let (subject, body_at, icon) = match n.subject {
            Some(s) => (
                s,
                0,
                n.icon.unwrap_or(if n.error {
                    NoticeIcon::Error
                } else {
                    NoticeIcon::Info
                }),
            ),
            None => {
                let (s, at, i) = classify(&n.text, n.error);
                (s, at, n.icon.unwrap_or(i))
            }
        };
        let timeout = match n.timeout {
            Some(secs) => Self::secs(secs),
            None => Self::timeout(cfg, n.error),
        };
        let shape = template(&n.text);
        let merged = self.visible.iter_mut().rev().find(|c| {
            !c.prompt && c.error == n.error && c.subject == subject && template(&c.text) == shape
        });
        let (key, count) = match merged {
            Some(card) => {
                card.count += 1;
                card.text = n.text.clone();
                card.body_at = body_at;
                card.icon = icon;
                card.shown = now;
                card.timeout = timeout;
                (card.id, card.count)
            }
            None => {
                if self.next_id == 0 {
                    // Seeded from the clock so the keys of the history never repeat.
                    self.next_id = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(1, |d| d.as_nanos() as u64);
                }
                self.next_id += 1;
                self.visible.push(Notification {
                    id: self.next_id,
                    subject: subject.clone(),
                    text: n.text.clone(),
                    body_at,
                    error: n.error,
                    icon,
                    prompt: false,
                    choices: Vec::new(),
                    count: 1,
                    shown: now,
                    timeout,
                });
                (self.next_id, 1)
            }
        };
        self.record(
            HistoryEntry {
                key,
                ts: unix_now(),
                error: n.error,
                subject,
                text: n.text,
                body_at,
                icon,
                count,
            },
            cfg,
        );
    }

    /// Shows a question that stays until [`dismiss_prompts`](Self::dismiss_prompts).
    pub fn prompt(
        &mut self,
        text: String,
        choices: &[(&'static str, crate::keys::KeyCode)],
        now: Instant,
    ) {
        self.dismiss_prompts();
        self.next_id += 1;
        self.visible.push(Notification {
            id: self.next_id,
            subject: "Question".into(),
            text,
            body_at: 0,
            error: false,
            icon: NoticeIcon::Info,
            prompt: true,
            choices: choices.to_vec(),
            count: 1,
            shown: now,
            timeout: None,
        });
    }

    pub fn dismiss_prompts(&mut self) {
        self.visible.retain(|c| !c.prompt);
    }

    pub fn dismiss(&mut self, id: u64) {
        self.visible.retain(|c| c.id != id);
    }

    pub fn dismiss_all(&mut self) {
        self.visible.clear();
    }

    /// Dismisses the cards `f` is true for.
    pub fn dismiss_where(&mut self, f: impl Fn(&Notification) -> bool) {
        self.visible.retain(|c| !f(c));
    }

    pub fn expire(&mut self, now: Instant) {
        self.visible.retain(|c| !c.expired(now));
    }

    /// How many errors were raised so far: a changed count tells that something failed.
    pub fn errors(&self) -> u64 {
        self.errors
    }

    /// The cards to draw, newest first (at most `max`), and how many more there are.
    pub fn shown(&self, now: Instant, max: usize) -> (Vec<&Notification>, usize) {
        let live: Vec<&Notification> = self
            .visible
            .iter()
            .rev()
            .filter(|c| !c.expired(now))
            .collect();
        let more = live.len().saturating_sub(max);
        (live.into_iter().take(max).collect(), more)
    }

    /// The history, oldest first.
    pub fn history(&self) -> &[HistoryEntry] {
        &self.history
    }

    /// Records an entry; a repeat of a card moves its entry to the end with the new count.
    fn record(&mut self, entry: HistoryEntry, cfg: &NotificationsConfig) {
        if cfg.history_size == 0 {
            return;
        }
        if let Some(path) = &self.history_file {
            // Failing to write the history must never raise another notification.
            if let Ok(line) = serde_json::to_string(&entry) {
                use std::io::Write;
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .and_then(|mut f| writeln!(f, "{line}"));
            }
        }
        if let Some(at) = self.history.iter().rposition(|e| e.key == entry.key) {
            self.history.remove(at);
        }
        self.history.push(entry);
        let extra = self.history.len().saturating_sub(cfg.history_size);
        self.history.drain(..extra);
    }

    /// Keeps the history in `path` from now on, loading what is there (and trimming the file
    /// to `limit` entries when it grew past twice that).
    pub fn keep_history(&mut self, path: PathBuf, limit: usize) {
        let mut at: HashMap<u64, usize> = HashMap::new();
        let mut entries: Vec<Option<HistoryEntry>> = Vec::new();
        let mut loose = u64::MAX;
        for mut e in std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<HistoryEntry>(l).ok())
        {
            if e.key == 0 {
                // From before entries had keys: each is its own.
                e.key = loose;
                loose -= 1;
            }
            if let Some(old) = at.insert(e.key, entries.len()) {
                entries[old] = None;
            }
            entries.push(Some(e));
        }
        let mut entries: Vec<HistoryEntry> = entries.into_iter().flatten().collect();
        if entries.len() > limit * 2 {
            entries.drain(..entries.len() - limit);
            let text: String = entries
                .iter()
                .filter_map(|e| serde_json::to_string(e).ok())
                .map(|l| l + "\n")
                .collect();
            let _ = std::fs::write(&path, text);
        }
        let extra = entries.len().saturating_sub(limit);
        entries.drain(..extra);
        entries.append(&mut self.history);
        self.history = entries;
        self.history_file = Some(path);
    }
}

/// Where the history lives: `$XDG_STATE_HOME/cecilia/notifications.jsonl`, else
/// `~/.local/state/cecilia/notifications.jsonl`.
pub fn default_history_file() -> Option<PathBuf> {
    state_file("notifications.jsonl")
}

/// A file in cecilia's state directory (the swap directory's parent).
pub(crate) fn state_file(name: &str) -> Option<PathBuf> {
    Some(crate::default_swap_dir()?.parent()?.join(name))
}

/// A time as the history shows it: how long ago ("now", "5m", "2h", "3d"), or the date and time
/// where the user is ("2026-11-10 14:03").
pub fn format_time(ts: u64, now: u64, absolute: bool) -> String {
    if !absolute {
        return match now.saturating_sub(ts) {
            s if s < 60 => "now".to_string(),
            s if s < 3600 => format!("{}m ago", s / 60),
            s if s < 86400 => format!("{}h ago", s / 3600),
            s => format!("{}d ago", s / 86400),
        };
    }
    let (y, mo, d, h, mi) = local_time(ts);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

/// Year, month, day, hour and minute of a Unix time in the local time zone (UTC without one).
fn local_time(ts: u64) -> (i64, u32, u32, u32, u32) {
    #[cfg(unix)]
    {
        let t = ts as libc::time_t;
        // SAFETY: `tm` is plain data that localtime_r fills in; both pointers are valid.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if !unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return (
                i64::from(tm.tm_year) + 1900,
                tm.tm_mon as u32 + 1,
                tm.tm_mday as u32,
                tm.tm_hour as u32,
                tm.tm_min as u32,
            );
        }
    }
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let days = (ts / 86400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    let secs = ts % 86400;
    (y, m, d, (secs / 3600) as u32, (secs % 3600 / 60) as u32)
}

// ---- placement ----

/// Which edge or corner the cards stack against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    /// -1 left, 0 centre, 1 right.
    pub h: i8,
    /// -1 top, 0 middle, 1 bottom.
    pub v: i8,
}

impl Anchor {
    pub fn parse(s: &str) -> Anchor {
        let (h, v) = match s {
            "top_left" => (-1, -1),
            "top" => (0, -1),
            "right" => (1, 0),
            "bottom_right" => (1, 1),
            "bottom" => (0, 1),
            "bottom_left" => (-1, 1),
            "left" => (-1, 0),
            _ => (1, -1),
        };
        Anchor { h, v }
    }
}

/// A placed card (any unit: cells or pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Places cards of the given sizes (newest first) inside `area` against `anchor`, `gap` apart.
/// Cards at the top (or middle) stack downward with the newest first; at the bottom they stack
/// upward with the newest nearest the edge. Cards that do not fit are left out.
pub fn place_stack(area: Placed, anchor: Anchor, sizes: &[(f64, f64)], gap: f64) -> Vec<Placed> {
    let mut fit = 0;
    let mut used = 0.0;
    for &(_, h) in sizes {
        let next = used + if fit > 0 { gap } else { 0.0 } + h;
        if next > area.h && fit > 0 {
            break;
        }
        used = next;
        fit += 1;
    }
    let mut y = match anchor.v {
        v if v < 0 => area.y,
        0 => area.y + (area.h - used).max(0.0) / 2.0,
        _ => area.y + (area.h - used).max(0.0),
    };
    let mut out: Vec<Placed> = sizes[..fit]
        .iter()
        .map(|&(w, h)| {
            let w = w.min(area.w);
            let x = match anchor.h {
                h if h < 0 => area.x,
                0 => area.x + (area.w - w) / 2.0,
                _ => area.x + area.w - w,
            };
            let r = Placed { x, y, w, h };
            y += h + gap;
            r
        })
        .collect();
    if anchor.v > 0 {
        // Newest at the bottom: the list was laid out top-down, so mirror it.
        out.reverse();
        let mut y = area.y + (area.h - used).max(0.0);
        for (r, &(_, h)) in out.iter_mut().zip(sizes[..fit].iter().rev()) {
            r.y = y;
            y += h + gap;
        }
        out.reverse();
    }
    out
}

/// Wraps `text` to lines of at most `width` characters, at most `max_lines` of them (the last
/// one ending in an ellipsis when text is cut).
pub fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        while word.chars().count() > width {
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            lines.push(head);
        }
        let used = line.chars().count();
        if used > 0 && used + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            let keep: String = last.chars().take(width.saturating_sub(1)).collect();
            *last = format!("{keep}…");
        }
    }
    lines
}

// ---- App ----

impl App {
    /// The cards to draw now, newest first, and how many more there are.
    pub fn notifications_shown(&self) -> (Vec<&Notification>, usize) {
        self.notifications.shown(
            Instant::now(),
            self.config.file.notifications.max_visible.max(1),
        )
    }

    /// Whether any card is on screen (the frontends keep redrawing while there is).
    pub fn notifications_active(&self) -> bool {
        self.notifications
            .visible
            .iter()
            .any(|c| !c.expired(Instant::now()))
    }

    /// Where the cards go: the area of the score panes or the whole window, and the anchor.
    pub fn notification_place(&self) -> (Rect, Anchor) {
        let cfg = &self.config.file.notifications;
        let area = if cfg.area == "scores" {
            self.pane_rects()
                .into_iter()
                .filter(|(id, _)| {
                    self.panes
                        .get(id)
                        .is_some_and(|p| p.kind == crate::PaneKind::Score)
                })
                .map(|(_, r)| r)
                .reduce(|a, b| {
                    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
                    Rect {
                        x,
                        y,
                        w: (a.x + a.w).max(b.x + b.w) - x,
                        h: (a.y + a.h).max(b.y + b.h) - y,
                    }
                })
        } else {
            None
        };
        (area.unwrap_or(self.viewport), Anchor::parse(&cfg.anchor))
    }

    /// A click on a card: it goes, unless it is a question (only its buttons answer that).
    pub fn dismiss_notification(&mut self, id: u64) {
        if !self
            .notifications
            .visible
            .iter()
            .any(|c| c.id == id && c.prompt)
        {
            self.notifications.dismiss(id);
        }
    }

    /// A click on button `choice` of the question on screen: the key it stands for answers it.
    pub fn answer_prompt(&mut self, choice: usize) {
        let code = self
            .notifications
            .visible
            .iter()
            .find(|c| c.prompt)
            .and_then(|c| c.choices.get(choice))
            .map(|&(_, code)| code);
        if let Some(code) = code {
            self.handle_key(crate::keys::Key::new(code));
        }
    }

    /// `:notify-dismiss`: clears the cards (not a question waiting for its answer).
    pub(crate) fn notify_dismiss(&mut self) {
        self.notifications.dismiss_where(|c| !c.prompt);
    }

    /// Starts keeping the history between sessions (the frontends call this once).
    pub fn keep_notification_history(&mut self) {
        let limit = self.config.file.notifications.history_size;
        if limit > 0
            && let Some(path) = default_history_file()
        {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            self.notifications.keep_history(path, limit);
        }
    }

    /// Forgets every notification, for tests.
    #[cfg(test)]
    pub(crate) fn clear_notifications(&mut self) {
        self.notifications.visible.clear();
        self.notifications.history.clear();
    }

    /// The newest notification raised, for tests.
    #[cfg(test)]
    pub(crate) fn last_notification(&self) -> Option<&HistoryEntry> {
        self.notifications.history.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> NotificationsConfig {
        NotificationsConfig::default()
    }

    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    #[test]
    fn identical_and_digit_variants_merge() {
        let (mut n, t0) = (Notifications::default(), Instant::now());
        n.push("bar 3 deleted".into(), false, t0, &cfg());
        n.push("bar 4 deleted".into(), false, t0, &cfg());
        n.push("saved".into(), false, t0, &cfg());
        n.push("saved".into(), false, t0, &cfg());
        let (cards, more) = n.shown(t0, 5);
        assert_eq!(more, 0);
        assert_eq!(cards.len(), 2);
        assert_eq!((cards[0].text.as_str(), cards[0].count), ("saved", 2));
        assert_eq!(
            (cards[1].text.as_str(), cards[1].count),
            ("bar 4 deleted", 2)
        );
        // The history stacks them the same way.
        assert_eq!(n.history().len(), 2);
    }

    #[test]
    fn severities_do_not_merge() {
        let (mut n, t0) = (Notifications::default(), Instant::now());
        n.push("x".into(), false, t0, &cfg());
        n.push("x".into(), true, t0, &cfg());
        assert_eq!(n.shown(t0, 5).0.len(), 2);
        assert_eq!(n.errors(), 1);
    }

    #[test]
    fn timeouts_follow_the_severity_and_zero_stays() {
        let (mut n, t0) = (Notifications::default(), Instant::now());
        let mut c = cfg();
        c.info_timeout = 2.0;
        c.error_timeout = 0.0;
        n.push("info".into(), false, t0, &c);
        n.push("err".into(), true, t0, &c);
        assert_eq!(n.shown(t0 + secs(1.9), 5).0.len(), 2);
        let (cards, _) = n.shown(t0 + secs(2.0), 5);
        assert_eq!(cards.len(), 1);
        assert!(cards[0].error);
        assert_eq!(cards[0].remaining_fraction(t0 + secs(1000.0)), None);
        n.expire(t0 + secs(2.0));
        assert_eq!(n.visible.len(), 1);
    }

    #[test]
    fn the_bar_drains_and_a_merge_refills_it() {
        let (mut n, t0) = (Notifications::default(), Instant::now());
        n.push("a".into(), false, t0, &cfg());
        let f = |n: &Notifications, at| n.shown(at, 5).0[0].remaining_fraction(at).unwrap();
        assert_eq!(f(&n, t0), 1.0);
        assert!((f(&n, t0 + secs(2.0)) - 0.5).abs() < 1e-6);
        assert!(f(&n, t0 + secs(3.99)) < 0.01);
        n.push("a".into(), false, t0 + secs(3.0), &cfg());
        assert_eq!(f(&n, t0 + secs(3.0)), 1.0);
    }

    #[test]
    fn overflow_is_counted_and_prompts_stay_and_do_not_merge() {
        let (mut n, t0) = (Notifications::default(), Instant::now());
        for i in 0..4 {
            n.push(format!("m{}x", (b'a' + i) as char), false, t0, &cfg());
        }
        let (cards, more) = n.shown(t0, 3);
        assert_eq!((cards.len(), more), (3, 1));
        assert_eq!(cards[0].text, "mdx");
        n.prompt("sure?".into(), &[], t0);
        n.prompt("sure?".into(), &[], t0);
        assert_eq!(n.visible.iter().filter(|c| c.prompt).count(), 1);
        assert!(n.shown(t0 + secs(3600.0), 9).0[0].prompt);
        n.dismiss_where(|c| !c.prompt);
        assert_eq!(n.visible.len(), 1);
        n.dismiss_prompts();
        assert!(n.visible.is_empty());
    }

    #[test]
    fn history_is_capped_and_kept_in_a_file() {
        let dir = std::env::temp_dir().join(format!("cecilia-notify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("n.jsonl");
        let _ = std::fs::remove_file(&file);
        let mut c = cfg();
        c.history_size = 2;
        let (mut n, t0) = (Notifications::default(), Instant::now());
        n.keep_history(file.clone(), 2);
        for t in ["one", "two", "three"] {
            n.push(t.into(), t == "two", t0, &c);
        }
        assert_eq!(n.history().len(), 2);
        assert_eq!(n.history()[0].text, "two");
        assert_eq!(std::fs::read_to_string(&file).unwrap().lines().count(), 3);
        let mut again = Notifications::default();
        again.keep_history(file.clone(), 2);
        let texts: Vec<_> = again.history().iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["two", "three"]);
        assert!(again.history()[0].error);
        // No history at all.
        let mut off = Notifications::default();
        c.history_size = 0;
        off.push("x".into(), false, t0, &c);
        assert!(off.history().is_empty());
    }

    #[test]
    fn stacks_sit_against_their_anchor() {
        let area = Placed {
            x: 10.0,
            y: 20.0,
            w: 100.0,
            h: 60.0,
        };
        let sizes = [(40.0, 10.0), (50.0, 10.0), (30.0, 10.0)];
        let at = |a: &str| place_stack(area, Anchor::parse(a), &sizes, 2.0);
        let r = at("top_right");
        assert_eq!((r[0].x, r[0].y), (70.0, 20.0));
        assert_eq!((r[1].x, r[1].y), (60.0, 32.0));
        let r = at("top_left");
        assert_eq!((r[0].x, r[0].y), (10.0, 20.0));
        let r = at("bottom_right");
        assert_eq!((r[0].x, r[0].y), (70.0, 80.0 - 10.0));
        assert_eq!(r[2].y, 80.0 - 34.0);
        let r = at("bottom");
        assert_eq!(r[0].x, 10.0 + 30.0);
        let r = at("left");
        assert_eq!((r[0].x, r[0].y), (10.0, 20.0 + (60.0 - 34.0) / 2.0));
        let r = at("right");
        assert_eq!(r[1].x, 60.0);
        // Cards that do not fit are left out; they stay inside the area.
        let tall = [(40.0, 30.0), (40.0, 30.0), (40.0, 30.0)];
        let r = place_stack(area, Anchor::parse("bottom"), &tall, 2.0);
        assert_eq!(r.len(), 1);
        assert!(r[0].y + r[0].h <= area.y + area.h);
        // Unknown names fall back to the default corner.
        assert_eq!(Anchor::parse("nowhere"), Anchor { h: 1, v: -1 });
    }

    #[test]
    fn a_question_has_buttons_that_answer_it() {
        use crate::keys::KeyCode;
        let mut app = App::new(crate::Config::default(), None);
        app.delete_workspace(Some("nope-not-there"), false);
        app.ask(
            "sure?".into(),
            &[("Yes", KeyCode::Char('y')), ("No", KeyCode::Esc)],
        );
        let id = app
            .notifications_shown()
            .0
            .iter()
            .find(|c| c.prompt)
            .unwrap()
            .id;
        // A click on the card itself leaves the question standing.
        app.dismiss_notification(id);
        assert!(app.notifications_shown().0.iter().any(|c| c.prompt));
        app.confirm_reload = Some(0);
        app.answer_prompt(1);
        assert!(app.confirm_reload.is_none());
        assert!(!app.notifications_shown().0.iter().any(|c| c.prompt));
    }

    #[test]
    fn text_wraps_and_cuts() {
        assert_eq!(wrap("a bb ccc dddd", 6, 5), ["a bb", "ccc", "dddd"]);
        assert_eq!(wrap("abcdefghij", 4, 5), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("one two three four", 5, 2), ["one", "two…"]);
    }

    #[test]
    fn messages_are_notifications_with_options_commands_and_a_picker() {
        use crate::{Command, Config, keys};
        let mut app = App::new(Config::default(), None);
        app.info("first thing");
        app.error("second thing");
        let (cards, more) = app.notifications_shown();
        assert_eq!((cards.len(), more), (2, 0));
        assert!(cards[0].error && app.notifications_active());
        // The history picker lists the newest first and filters by the text.
        app.run(Command::parse("notifications").unwrap());
        let p = app.palette().unwrap();
        assert_eq!(p.items[0].text, "Error: second thing");
        assert_eq!(p.items[0].description.as_deref(), Some("now"));
        for k in keys::parse_keys("first", &[]).unwrap() {
            app.handle_key(k);
        }
        assert_eq!(app.palette().unwrap().items.len(), 1);
        app.handle_key(keys::parse_keys("<Esc>", &[]).unwrap()[0]);
        // Placement options, and a bad one is refused.
        app.run(Command::parse("set notifications.anchor left").unwrap());
        assert_eq!(app.notification_place().1, Anchor { h: -1, v: 0 });
        app.run(Command::parse("set notifications.anchor middle").unwrap());
        assert!(app.last_notification().unwrap().error);
        app.run(Command::parse("set notifications.error_timeout 0").unwrap());
        app.run(Command::parse("notify-dismiss").unwrap());
        assert!(!app.notifications_active());
        // A question stays until answered.
        app.ask("sure?".into(), &[]);
        app.run(Command::parse("notify-dismiss").unwrap());
        assert!(app.notifications_active());
        app.answered();
        assert!(!app.notifications_active());
    }

    #[test]
    fn messages_get_a_subject_and_an_icon() {
        let c = classify("unknown command: nosuch", true);
        assert_eq!(
            (c.0.as_str(), c.1, c.2),
            ("Unknown Command", 17, NoticeIcon::Command)
        );
        let c = classify("workspace opened: demo", false);
        assert_eq!(
            (c.0.as_str(), c.2),
            ("Workspace Opened", NoticeIcon::Workspace)
        );
        let c = classify("ui.tab_width = 2", false);
        assert_eq!((c.0.as_str(), c.1, c.2), ("Option", 0, NoticeIcon::Option));
        assert_eq!(classify("boom", true).0, "Error");
        assert_eq!(classify("boom", true).2, NoticeIcon::Error);
        assert_eq!(classify("hello there", false).0, "Notice");
        // A sentence with a colon far in is not a topic.
        assert_eq!(
            classify("this is a very long sentence indeed: yes", false).0,
            "Notice"
        );
        let (mut n, t0) = (Notifications::default(), Instant::now());
        n.push("unknown command: nosuch".into(), true, t0, &cfg());
        let card = n.shown(t0, 5).0[0].clone();
        assert_eq!(
            (card.subject.as_str(), card.body()),
            ("Unknown Command", "nosuch")
        );
    }

    #[test]
    fn notify_takes_a_message_and_optional_settings() {
        use crate::Command;
        let Ok(Command::Notify(a)) = Command::parse("notify hello  world") else {
            panic!("parse")
        };
        assert_eq!(
            (a.message.as_str(), a.subject, a.timeout, a.error, a.icon),
            ("hello world", None, None, None, None)
        );
        let Ok(Command::Notify(a)) =
            Command::parse("notify -s \"Big news\" -t 2.5 -u error -i audio all of it")
        else {
            panic!("parse")
        };
        assert_eq!(a.message, "all of it");
        assert_eq!(a.subject.as_deref(), Some("Big news"));
        assert_eq!(
            (a.timeout, a.error, a.icon),
            (Some(2.5), Some(true), Some(NoticeIcon::Audio))
        );
        for bad in [
            "notify",
            "notify -s x",
            "notify -t soon hi",
            "notify -u loud hi",
            "notify -i nope hi",
            "notify \"unclosed",
        ] {
            assert!(Command::parse(bad).is_err(), "{bad}");
        }
        // The card carries them; a timeout of 0 stays.
        let mut app = App::new(crate::Config::default(), None);
        app.run(Command::parse("notify -s Hi -t 0 -u error there").unwrap());
        let (cards, _) = app.notifications_shown();
        assert_eq!(
            (cards[0].subject.as_str(), cards[0].body(), cards[0].error),
            ("Hi", "there", true)
        );
        assert_eq!(cards[0].remaining_fraction(Instant::now()), None);
    }

    #[test]
    fn repeats_stack_in_the_history_and_enter_shows_one_again() {
        let (mut n, t0) = (Notifications::default(), Instant::now());
        for t in ["bar 1 deleted", "other", "bar 2 deleted", "bar 3 deleted"] {
            n.push(t.into(), false, t0, &cfg());
        }
        let h = n.history();
        assert_eq!(h.len(), 2);
        assert_eq!((h[1].text.as_str(), h[1].count), ("bar 3 deleted", 3));
        assert_eq!(h[0].text, "other");
        // After the cards are gone, a repeat is a new entry.
        n.expire(t0 + secs(100.0));
        n.push("bar 4 deleted".into(), false, t0 + secs(100.0), &cfg());
        assert_eq!(n.history().len(), 3);

        let mut app = App::new(crate::Config::default(), None);
        app.run(crate::Command::parse("notify -s Hello -i file hi there").unwrap());
        app.notifications.dismiss_all();
        let key = app.notifications.history()[0].key;
        app.run(crate::Command::NotifyView(key));
        // It opens in a popup, not as a card, and is not recorded again.
        let v = app.viewer().unwrap();
        assert_eq!(
            (v.subject.as_str(), v.edit.text.as_str(), v.icon),
            ("Hello", "hi there", NoticeIcon::File)
        );
        assert!(app.notifications_shown().0.is_empty());
        assert_eq!(app.notifications.history().len(), 1);
    }

    #[test]
    fn the_history_file_collapses_repeats_and_times_show_either_way() {
        let dir = std::env::temp_dir().join(format!("cecilia-notify2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("n.jsonl");
        let _ = std::fs::remove_file(&file);
        let (mut n, t0) = (Notifications::default(), Instant::now());
        n.keep_history(file.clone(), 50);
        for t in ["x 1", "y", "x 2"] {
            n.push(t.into(), false, t0, &cfg());
        }
        let mut again = Notifications::default();
        again.keep_history(file, 50);
        let h = again.history();
        assert_eq!((h.len(), h[1].text.as_str(), h[1].count), (2, "x 2", 2));
        assert_eq!(format_time(100, 130, false), "now");
        assert_eq!(format_time(100, 100 + 5 * 60, false), "5m ago");
        assert_eq!(format_time(100, 100 + 3 * 3600, false), "3h ago");
        assert_eq!(format_time(100, 100 + 2 * 86400, false), "2d ago");
        let abs = format_time(1_700_000_000, 0, true);
        assert!(abs.starts_with("2023-11-1") && abs.len() == 16, "{abs}");
        // The picker follows the option.
        let mut app = App::new(crate::Config::default(), None);
        app.info("hi");
        app.run(crate::Command::parse("set notifications.history_time absolute").unwrap());
        app.run(crate::Command::parse("notifications").unwrap());
        let d = app.palette().unwrap().items[0].description.clone().unwrap();
        assert!(d.starts_with("20") && d.contains(':'), "{d}");
        // The row has the notification's icon (not a swatch) and a bold subject.
        let items = app.palette().unwrap().items;
        let item = items.iter().find(|i| i.text == "Notice: hi").unwrap();
        assert_eq!(item.notice, Some((NoticeIcon::Info, false)));
        assert!(item.swatch.is_none() && item.bold == "Notice:".len());
        // The popup look is options, with limits.
        for c in ["set popups.opacity 60", "set popups.shadow_blur 30"] {
            app.run(crate::Command::parse(c).unwrap());
        }
        assert_eq!(app.config.file.popups.opacity, 60.0);
        assert_eq!(app.config.file.popups.shadow_blur, 30.0);
        app.run(crate::Command::parse("set popups.opacity 101").unwrap());
        assert!(app.last_notification().unwrap().error);
        assert_eq!(app.config.file.popups.opacity, 60.0);
    }
}
