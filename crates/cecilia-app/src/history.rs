//! Branching undo history (vim's undo tree). Each node holds the change that leads to it from
//! its parent: either the bars it touched or, for structural edits, whole-score snapshots.

use cecilia_core::design::DesignOverrides;
use cecilia_core::{Bar, Score};
use serde::{Deserialize, Serialize};

use crate::Cursor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarChange {
    pub track: usize,
    pub bar: usize,
    pub before: Bar,
    pub after: Bar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Change {
    Bars(Vec<BarChange>),
    Score {
        before: Box<Score>,
        after: Box<Score>,
    },
    /// A change of the page-layout design only.
    Design {
        before: DesignOverrides,
        after: DesignOverrides,
    },
}

impl Change {
    fn apply(&self, score: &mut Score, forward: bool) {
        match self {
            Change::Bars(changes) => {
                for c in changes {
                    if let Some(bar) = score
                        .tracks
                        .get_mut(c.track)
                        .and_then(|t| t.bars.get_mut(c.bar))
                    {
                        *bar = if forward {
                            c.after.clone()
                        } else {
                            c.before.clone()
                        };
                    }
                }
            }
            Change::Score { before, after } => {
                *score = if forward {
                    (**after).clone()
                } else {
                    (**before).clone()
                }
            }
            Change::Design { before, after } => {
                score.design = if forward { after } else { before }.clone();
            }
        }
    }
}

/// One change equivalent to applying `a` then `b`.
fn merge(a: Change, b: Change) -> Change {
    match (a, b) {
        (Change::Design { before, .. }, Change::Design { after, .. }) => {
            Change::Design { before, after }
        }
        // A design change beside others (never done on purpose: design edits are their own
        // undo steps).
        (a @ Change::Design { .. }, b) | (a, b @ Change::Design { .. }) => merge_mixed(a, b),
        (Change::Bars(mut a), Change::Bars(b)) => {
            for c in b {
                match a.iter_mut().find(|x| x.track == c.track && x.bar == c.bar) {
                    Some(x) => x.after = c.after,
                    None => a.push(c),
                }
            }
            Change::Bars(a)
        }
        (a @ Change::Bars(_), Change::Score { before, after }) => {
            let mut original = *before;
            a.apply(&mut original, false);
            Change::Score {
                before: Box::new(original),
                after,
            }
        }
        (Change::Score { before, after }, b @ Change::Bars(_)) => {
            let mut end = *after;
            b.apply(&mut end, true);
            Change::Score {
                before,
                after: Box::new(end),
            }
        }
        (Change::Score { before, .. }, Change::Score { after, .. }) => {
            Change::Score { before, after }
        }
    }
}

/// Merges a design change with a bar or score change by way of two whole-score snapshots.
fn merge_mixed(a: Change, b: Change) -> Change {
    // Without the score at hand, the bars of a bar change are folded into the snapshots the
    // other change carries; a design change next to a bar change never happens (design edits
    // are their own undo steps), so this keeps the later change.
    match (a, b) {
        (Change::Score { before, after }, Change::Design { after: design, .. }) => {
            let mut after = after;
            after.design = design;
            Change::Score { before, after }
        }
        (Change::Design { before: design, .. }, Change::Score { mut before, after }) => {
            before.design = design;
            Change::Score { before, after }
        }
        (_, b) => b,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    change: Option<Change>,
    cursor_before: Cursor,
    cursor_after: Cursor,
    seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    nodes: Vec<Node>,
    current: usize,
    /// Sequence number of the most recent change (0 = the original state).
    seq: u64,
    /// The state last written to (or read from) the file, if it is still one of the nodes:
    /// undoing or redoing back to it means the file is unchanged.
    saved: Option<usize>,
}

impl Default for History {
    fn default() -> Self {
        let root = Node {
            parent: None,
            children: vec![],
            change: None,
            cursor_before: Cursor::default(),
            cursor_after: Cursor::default(),
            seq: 0,
        };
        Self {
            nodes: vec![root],
            current: 0,
            seq: 0,
            saved: Some(0),
        }
    }
}

impl History {
    /// Records a change already applied to the score.
    pub fn push(&mut self, change: Change, cursor_before: Cursor, cursor_after: Cursor) {
        self.seq += 1;
        let id = self.nodes.len();
        self.nodes.push(Node {
            parent: Some(self.current),
            children: vec![],
            change: Some(change),
            cursor_before,
            cursor_after,
            seq: self.seq,
        });
        self.nodes[self.current].children.push(id);
        self.current = id;
    }

    /// Folds a change into the current node (one undo step for a whole insert session).
    pub fn amend(&mut self, change: Change, cursor_after: Cursor) {
        // The saved state is about to change in place: it no longer matches the file.
        if self.saved == Some(self.current) {
            self.saved = None;
        }
        let node = &mut self.nodes[self.current];
        let Some(old) = node.change.take() else {
            // Nothing to amend at the root: record normally.
            let before = node.cursor_after;
            return self.push(change, before, cursor_after);
        };
        node.change = Some(merge(old, change));
        node.cursor_after = cursor_after;
    }

    pub fn current_seq(&self) -> u64 {
        self.nodes[self.current].seq
    }

    /// Records that the current state is what the file now holds.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.current);
    }

    /// Whether the score is in the state last saved (or opened).
    pub fn at_saved(&self) -> bool {
        self.saved == Some(self.current)
    }

    pub fn can_undo(&self) -> bool {
        self.current != 0
    }

    pub fn can_redo(&self) -> bool {
        !self.nodes[self.current].children.is_empty()
    }

    /// Undoes the current change; returns the cursor to restore.
    pub fn undo(&mut self, score: &mut Score) -> Option<Cursor> {
        let node = &self.nodes[self.current];
        let parent = node.parent?;
        node.change.as_ref()?.apply(score, false);
        let cursor = node.cursor_before;
        self.current = parent;
        Some(cursor)
    }

    /// Redoes the most recently made child of the current state.
    pub fn redo(&mut self, score: &mut Score) -> Option<Cursor> {
        let child = *self.nodes[self.current]
            .children
            .iter()
            .max_by_key(|&&c| self.nodes[c].seq)?;
        self.nodes[child].change.as_ref()?.apply(score, true);
        self.current = child;
        Some(self.nodes[child].cursor_after)
    }

    fn ancestors(&self, mut n: usize) -> Vec<usize> {
        let mut out = vec![n];
        while let Some(p) = self.nodes[n].parent {
            out.push(p);
            n = p;
        }
        out
    }

    /// Moves to the state with sequence number `seq` across branches (vim's g- / g+).
    pub fn goto_seq(&mut self, score: &mut Score, seq: u64) -> Option<Cursor> {
        let target = self.nodes.iter().position(|n| n.seq == seq)?;
        let from = self.ancestors(self.current);
        let to = self.ancestors(target);
        let common = *from.iter().find(|n| to.contains(n))?;
        let mut cursor = None;
        while self.current != common {
            cursor = self.undo(score);
        }
        let path: Vec<usize> = to.iter().take_while(|&&n| n != common).copied().collect();
        for &n in path.iter().rev() {
            self.nodes[n].change.as_ref()?.apply(score, true);
            self.current = n;
            cursor = Some(self.nodes[n].cursor_after);
        }
        cursor
    }

    pub fn older(&mut self, score: &mut Score) -> Option<Cursor> {
        let seq = self.current_seq().checked_sub(1)?;
        // The nearest existing older state.
        let seq = (0..=seq)
            .rev()
            .find(|s| self.nodes.iter().any(|n| n.seq == *s))?;
        self.goto_seq(score, seq)
    }

    pub fn newer(&mut self, score: &mut Score) -> Option<Cursor> {
        let seq = self.current_seq() + 1;
        let seq = (seq..=self.seq).find(|s| self.nodes.iter().any(|n| n.seq == *s))?;
        self.goto_seq(score, seq)
    }

    /// Number of changes in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len() - 1
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cecilia_core::*;

    fn score() -> Score {
        Score {
            master_bars: vec![MasterBar::default()],
            tracks: vec![Track {
                bars: vec![Bar::default()],
                ..Track::default()
            }],
            ..Score::default()
        }
    }

    fn set(s: &mut Score, h: &mut History, fret: u8) {
        let before = s.tracks[0].bars[0].clone();
        s.tracks[0].bars[0].voices = vec![Voice {
            beats: vec![Beat {
                notes: vec![Note::new(0, fret)],
                ..Beat::default()
            }],
        }];
        let after = s.tracks[0].bars[0].clone();
        h.push(
            Change::Bars(vec![BarChange {
                track: 0,
                bar: 0,
                before,
                after,
            }]),
            Cursor::default(),
            Cursor::default(),
        );
    }

    fn fret(s: &Score) -> Option<u8> {
        s.tracks[0].bars[0]
            .voices
            .first()
            .map(|v| v.beats[0].notes[0].fret)
    }

    #[test]
    fn undo_redo_and_branches() {
        let mut s = score();
        let mut h = History::default();
        set(&mut s, &mut h, 1);
        set(&mut s, &mut h, 2);
        h.undo(&mut s);
        assert_eq!(fret(&s), Some(1));
        // A new change after undo starts a branch.
        set(&mut s, &mut h, 3);
        h.undo(&mut s);
        h.redo(&mut s);
        assert_eq!(fret(&s), Some(3));
        // g- walks back in time across branches: 3 -> 2 -> 1.
        h.older(&mut s);
        assert_eq!(fret(&s), Some(2));
        h.older(&mut s);
        assert_eq!(fret(&s), Some(1));
        h.newer(&mut s);
        h.newer(&mut s);
        assert_eq!(fret(&s), Some(3));
        h.goto_seq(&mut s, 0);
        assert_eq!(fret(&s), None);
    }
}
