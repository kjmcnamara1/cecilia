//! Pane split trees, as in vim windows.

use crate::command::Direction;

pub type PaneId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Children stacked top to bottom (`:split`).
    Stacked,
    /// Children side by side (`:vsplit`).
    SideBySide,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Leaf(PaneId),
    /// Children share the space in proportion to their weights.
    Split {
        axis: Axis,
        children: Vec<Node>,
        weights: Vec<f32>,
    },
}

/// A pane's fixed size along an axis (tool panes), in viewport units; `None` for panes that
/// share their split's room (score panes).
pub type Fixed<'a> = &'a dyn Fn(PaneId, Axis) -> Option<u16>;

/// No pane has a fixed size.
pub fn flexible(_: PaneId, _: Axis) -> Option<u16> {
    None
}

/// The smallest share of a split a pane can be resized to.
const MIN_SHARE: f32 = 0.05;

/// A border between two children of a split, which the mouse can drag.
#[derive(Debug, Clone, PartialEq)]
pub struct Divider {
    /// Child indices from the root down to the split.
    pub path: Vec<usize>,
    /// The border is after child `index`.
    pub index: usize,
    pub axis: Axis,
    /// Where it is: x for side-by-side children, y for stacked ones.
    pub pos: u16,
    /// The area of the split.
    pub area: Rect,
}

impl Node {
    /// The tree with every split's weights equal: its nesting alone.
    pub fn shape(&self) -> Node {
        match self {
            Node::Leaf(_) => self.clone(),
            Node::Split { axis, children, .. } => Node::Split {
                axis: *axis,
                weights: vec![1.0; children.len()],
                children: children.iter().map(Node::shape).collect(),
            },
        }
    }

    pub fn panes(&self) -> Vec<PaneId> {
        match self {
            Node::Leaf(id) => vec![*id],
            Node::Split { children, .. } => children.iter().flat_map(Node::panes).collect(),
        }
    }

    /// Splits `target`, placing `new` after it along `axis`. Returns false if not found.
    pub fn split(&mut self, target: PaneId, new: PaneId, axis: Axis) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                *self = Node::Split {
                    axis,
                    children: vec![Node::Leaf(target), Node::Leaf(new)],
                    weights: vec![1.0, 1.0],
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split {
                axis: a,
                children,
                weights,
            } => {
                if *a == axis {
                    if let Some(i) = children.iter().position(|c| *c == Node::Leaf(target)) {
                        // The split's panes share it equally again (fixed ones keep their size).
                        children.insert(i + 1, Node::Leaf(new));
                        weights.insert(i + 1, 1.0);
                        weights.iter_mut().for_each(|w| *w = 1.0);
                        return true;
                    }
                }
                children.iter_mut().any(|c| c.split(target, new, axis))
            }
        }
    }

    /// Removes a pane, collapsing splits left with one child. Returns false if `self` is that
    /// single leaf (the caller must handle removing the whole tree).
    pub fn remove(&mut self, target: PaneId) -> bool {
        self.remove_with(target, false)
    }

    /// Like [`Node::remove`]; with `equalize` the split that lost the pane gives its remaining
    /// children equal weights instead of handing the space to a neighbour. Other splits keep
    /// their weights.
    pub fn remove_with(&mut self, target: PaneId, equalize: bool) -> bool {
        let Node::Split {
            children, weights, ..
        } = self
        else {
            return !matches!(self, Node::Leaf(id) if *id == target);
        };
        if let Some(i) = children.iter().position(|c| *c == Node::Leaf(target)) {
            children.remove(i);
            // Its space goes to the neighbour that grows into it.
            let w = weights.remove(i);
            if equalize {
                weights.iter_mut().for_each(|w| *w = 1.0);
            } else if let Some(n) = weights.get_mut(i.saturating_sub(1)) {
                *n += w;
            }
        } else {
            for c in children.iter_mut() {
                c.remove_with(target, equalize);
            }
        }
        if children.len() == 1 {
            *self = children.pop().unwrap();
        }
        true
    }

    /// Computes pane rectangles, dividing each split by its weights.
    pub fn layout(&self, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
        self.layout_with(area, out, &flexible);
    }

    /// Computes pane rectangles: panes with a fixed size get it, the others share the rest of
    /// each split by their weights.
    pub fn layout_with(&self, area: Rect, out: &mut Vec<(PaneId, Rect)>, fixed: Fixed) {
        match self {
            Node::Leaf(id) => out.push((*id, area)),
            Node::Split {
                axis,
                children,
                weights,
            } => {
                let sizes = fixed_sizes(children, *axis, fixed);
                for (child, r) in children
                    .iter()
                    .zip(child_areas(*axis, weights, &sizes, area))
                {
                    child.layout_with(r, out, fixed);
                }
            }
        }
    }

    /// The node a path of child indices leads to.
    pub fn node_at(&self, path: &[usize]) -> Option<&Node> {
        let mut node = self;
        for &i in path {
            match node {
                Node::Split { children, .. } if i < children.len() => node = &children[i],
                _ => return None,
            }
        }
        Some(node)
    }

    /// The borders between children of every split, for dragging.
    pub fn dividers(&self, area: Rect, fixed: Fixed) -> Vec<Divider> {
        let mut out = Vec::new();
        self.collect_dividers(area, &mut Vec::new(), &mut out, fixed);
        out
    }

    /// The areas of the children of the split at a path, within its area.
    pub fn child_rects(&self, path: &[usize], area: Rect, fixed: Fixed) -> Vec<Rect> {
        match self.node_at(path) {
            Some(Node::Split {
                axis,
                children,
                weights,
            }) => child_areas(*axis, weights, &fixed_sizes(children, *axis, fixed), area),
            _ => Vec::new(),
        }
    }

    fn collect_dividers(
        &self,
        area: Rect,
        path: &mut Vec<usize>,
        out: &mut Vec<Divider>,
        fixed: Fixed,
    ) {
        let Node::Split {
            axis,
            children,
            weights,
        } = self
        else {
            return;
        };
        let sizes = fixed_sizes(children, *axis, fixed);
        let rects = child_areas(*axis, weights, &sizes, area);
        for (i, r) in rects.iter().enumerate() {
            if i + 1 < rects.len() {
                out.push(Divider {
                    path: path.clone(),
                    index: i,
                    axis: *axis,
                    pos: match axis {
                        Axis::Stacked => r.y + r.h,
                        Axis::SideBySide => r.x + r.w,
                    },
                    area,
                });
            }
            path.push(i);
            children[i].collect_dividers(*r, path, out, fixed);
            path.pop();
        }
    }

    /// Moves a divider to `pos` (the same coordinate as [`Divider::pos`]): the children on
    /// either side of it share their space anew.
    pub fn drag_divider(&mut self, d: &Divider, pos: u16) {
        let mut node = self;
        for &i in &d.path {
            match node {
                Node::Split { children, .. } if i < children.len() => node = &mut children[i],
                _ => return,
            }
        }
        let Node::Split { weights, .. } = node else {
            return;
        };
        if d.index + 1 >= weights.len() {
            return;
        }
        let (start, total) = match d.axis {
            Axis::Stacked => (d.area.y, d.area.h),
            Axis::SideBySide => (d.area.x, d.area.w),
        };
        normalize(weights);
        let before: f32 = weights[..d.index].iter().sum();
        let pair = weights[d.index] + weights[d.index + 1];
        let f = (pos.saturating_sub(start) as f32 / total.max(1) as f32) - before;
        let first = f.clamp(MIN_SHARE.min(pair / 2.0), pair - MIN_SHARE.min(pair / 2.0));
        weights[d.index] = first;
        weights[d.index + 1] = pair - first;
    }

    /// Resizes `target` along `axis` by its innermost split on that axis: to `to` or by `by`
    /// percent of that split, taking the space from (or giving it to) the next pane. False
    /// when no split of that axis holds it.
    pub fn resize(&mut self, target: PaneId, axis: Axis, to: Option<f32>, by: f32) -> bool {
        let Node::Split {
            axis: a,
            children,
            weights,
        } = self
        else {
            return false;
        };
        let Some(i) = children.iter().position(|c| c.panes().contains(&target)) else {
            return false;
        };
        if children[i].resize(target, axis, to, by) {
            return true;
        }
        if *a != axis {
            return false;
        }
        normalize(weights);
        let j = if i + 1 < weights.len() { i + 1 } else { i - 1 };
        let pair = weights[i] + weights[j];
        let want = to.map_or(weights[i] + by / 100.0, |t| t / 100.0);
        let lo = MIN_SHARE.min(pair / 2.0);
        weights[i] = want.clamp(lo, pair - lo);
        weights[j] = pair - weights[i];
        true
    }

    /// The split directly holding pane `target`, and its index there.
    fn parent_of(&mut self, target: PaneId) -> Option<(&mut Vec<Node>, usize)> {
        let Node::Split { children, .. } = self else {
            return None;
        };
        if let Some(i) = children.iter().position(|c| *c == Node::Leaf(target)) {
            return Some((children, i));
        }
        children.iter_mut().find_map(|c| c.parent_of(target))
    }

    /// `<C-w>x`: swaps a pane with the next one of its split (the previous one for the last).
    pub fn exchange(&mut self, target: PaneId) -> bool {
        let Some((children, i)) = self.parent_of(target) else {
            return false;
        };
        let j = if i + 1 < children.len() { i + 1 } else { i - 1 };
        children.swap(i, j);
        true
    }

    /// `<C-w>r` / `<C-w>R`: rotates the panes of a pane's split one place on (the last one
    /// becomes the first), or back.
    pub fn rotate(&mut self, target: PaneId, forward: bool) -> bool {
        let Some((children, _)) = self.parent_of(target) else {
            return false;
        };
        if forward {
            children.rotate_right(1);
        } else {
            children.rotate_left(1);
        }
        true
    }

    /// `<C-w>H` `J` `K` `L`: moves a pane to the far left, bottom, top or right of the whole
    /// view, spanning its full height or width.
    pub fn move_to_edge(&mut self, target: PaneId, dir: Direction) -> bool {
        if !self.panes().contains(&target) || matches!(self, Node::Leaf(_)) {
            return false;
        }
        self.remove(target);
        let axis = match dir {
            Direction::Left | Direction::Right => Axis::SideBySide,
            Direction::Up | Direction::Down => Axis::Stacked,
        };
        let first = matches!(dir, Direction::Left | Direction::Up);
        match self {
            Node::Split {
                axis: a,
                children,
                weights,
            } if *a == axis => {
                // Another pane along the same line: an equal share.
                let share = weights.iter().sum::<f32>() / weights.len().max(1) as f32;
                let at = if first { 0 } else { children.len() };
                children.insert(at, Node::Leaf(target));
                weights.insert(at, share);
            }
            _ => {
                let old = std::mem::replace(self, Node::Leaf(target));
                let mut children = vec![old];
                children.insert(if first { 0 } else { 1 }, Node::Leaf(target));
                *self = Node::Split {
                    axis,
                    children,
                    weights: vec![1.0, 1.0],
                };
            }
        }
        true
    }

    /// Gives every pane of every split an equal share.
    pub fn equalize(&mut self) {
        if let Node::Split {
            children, weights, ..
        } = self
        {
            weights.iter_mut().for_each(|w| *w = 1.0);
            children.iter_mut().for_each(Node::equalize);
        }
    }
}

/// Makes weights add up to 1 (all equal if they are unusable).
fn normalize(weights: &mut [f32]) {
    let sum: f32 = weights.iter().filter(|w| w.is_finite() && **w > 0.0).sum();
    for w in weights.iter_mut() {
        *w = if sum > 0.0 && w.is_finite() && *w > 0.0 {
            *w / sum
        } else {
            0.0
        };
    }
    if weights.iter().all(|w| *w == 0.0) {
        let n = weights.len().max(1) as f32;
        weights.iter_mut().for_each(|w| *w = 1.0 / n);
    }
}

/// The fixed sizes of a split's children along its axis (only panes can be fixed).
fn fixed_sizes(children: &[Node], axis: Axis, fixed: Fixed) -> Vec<Option<u16>> {
    children
        .iter()
        .map(|c| match c {
            Node::Leaf(id) => fixed(*id, axis),
            Node::Split { .. } => None,
        })
        .collect()
}

/// The areas of a split's children: fixed ones their size (shrunk only if they don't fit),
/// the others the rest in proportion to their weights, rounded so they tile `area`.
fn child_areas(axis: Axis, weights: &[f32], sizes: &[Option<u16>], area: Rect) -> Vec<Rect> {
    let total = match axis {
        Axis::Stacked => area.h,
        Axis::SideBySide => area.w,
    } as f32;
    let flex: Vec<bool> = (0..weights.len())
        .map(|i| sizes.get(i).copied().flatten().is_none())
        .collect();
    let any_flex = flex.iter().any(|f| *f);
    // Fixed children keep their size while flexible ones keep some room.
    let fixed_total: f32 = sizes.iter().flatten().map(|&s| s as f32).sum();
    let room = if any_flex { total * 0.9 } else { total };
    let squeeze = if fixed_total > room && fixed_total > 0.0 {
        room / fixed_total
    } else {
        1.0
    };
    let mut flex_w: Vec<f32> = weights
        .iter()
        .zip(&flex)
        .map(|(w, f)| if *f { *w } else { 0.0 })
        .collect();
    normalize(&mut flex_w);
    let rest = (total - fixed_total * squeeze).max(0.0);
    let lengths: Vec<f32> = (0..weights.len())
        .map(|i| match sizes.get(i).copied().flatten() {
            Some(s) => s as f32 * squeeze,
            None => flex_w[i] * rest,
        })
        .collect();
    // With no flexible child, the fixed ones fill the split.
    let scale = if any_flex {
        1.0
    } else {
        total / lengths.iter().sum::<f32>().max(1.0)
    };
    let mut out = Vec::with_capacity(lengths.len());
    let mut acc = 0.0;
    let mut offset = 0u16;
    let total = total as u16;
    for (i, len) in lengths.iter().enumerate() {
        acc += len * scale;
        let end = if i + 1 == lengths.len() {
            total
        } else {
            (acc.round() as u16).clamp(offset, total)
        };
        let size = end - offset;
        out.push(match axis {
            Axis::Stacked => Rect {
                y: area.y + offset,
                h: size,
                ..area
            },
            Axis::SideBySide => Rect {
                x: area.x + offset,
                w: size,
                ..area
            },
        });
        offset = end;
    }
    out
}

/// Finds the pane adjacent to `from` in `dir`. Among the panes touching its edge the most
/// recently focused wins (`recency` is higher for later focus, 0 for never focused); ties go to
/// the one aligned with its centre.
pub fn neighbour(
    rects: &[(PaneId, Rect)],
    from: PaneId,
    dir: Direction,
    recency: impl Fn(PaneId) -> u64,
) -> Option<PaneId> {
    let (_, cur) = *rects.iter().find(|(id, _)| *id == from)?;
    let (cx, cy) = (
        cur.x as i32 + cur.w as i32 / 2,
        cur.y as i32 + cur.h as i32 / 2,
    );
    rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|(id, r)| {
            let (x0, y0, x1, y1) = (
                r.x as i32,
                r.y as i32,
                (r.x + r.w) as i32,
                (r.y + r.h) as i32,
            );
            let overlaps_v = y0 < (cur.y + cur.h) as i32 && y1 > cur.y as i32;
            let overlaps_h = x0 < (cur.x + cur.w) as i32 && x1 > cur.x as i32;
            let dist = match dir {
                Direction::Left if overlaps_v && x1 <= cur.x as i32 => cur.x as i32 - x1,
                Direction::Right if overlaps_v && x0 >= (cur.x + cur.w) as i32 => {
                    x0 - (cur.x + cur.w) as i32
                }
                Direction::Up if overlaps_h && y1 <= cur.y as i32 => cur.y as i32 - y1,
                Direction::Down if overlaps_h && y0 >= (cur.y + cur.h) as i32 => {
                    y0 - (cur.y + cur.h) as i32
                }
                _ => return None,
            };
            let off_centre = match dir {
                Direction::Left | Direction::Right => (cy - (y0 + y1) / 2).abs(),
                Direction::Up | Direction::Down => (cx - (x0 + x1) / 2).abs(),
            };
            Some(((dist, std::cmp::Reverse(recency(*id)), off_centre), *id))
        })
        .min()
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbour_prefers_most_recently_focused() {
        let r = |x, y, w, h| Rect { x, y, w, h };
        let rects = [
            (1, r(0, 0, 10, 5)),
            (2, r(0, 5, 10, 5)),
            (3, r(10, 0, 10, 10)),
        ];
        let rec = |id| if id == 2 { 5 } else { 0 };
        assert_eq!(neighbour(&rects, 3, Direction::Left, rec), Some(2));
        assert_eq!(
            neighbour(&rects, 3, Direction::Left, |id| id as u64 % 2),
            Some(1)
        );
    }

    #[test]
    fn split_layout_and_navigate() {
        let mut root = Node::Leaf(1);
        assert!(root.split(1, 2, Axis::SideBySide));
        assert!(root.split(2, 3, Axis::Stacked));
        let mut rects = vec![];
        root.layout(
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 40,
            },
            &mut rects,
        );
        assert_eq!(rects.len(), 3);
        assert_eq!(
            rects[0].1,
            Rect {
                x: 0,
                y: 0,
                w: 50,
                h: 40
            }
        );
        assert_eq!(
            rects[2].1,
            Rect {
                x: 50,
                y: 20,
                w: 50,
                h: 20
            }
        );
        assert_eq!(neighbour(&rects, 1, Direction::Right, |_| 0), Some(2));
        assert_eq!(neighbour(&rects, 2, Direction::Down, |_| 0), Some(3));
        assert_eq!(
            neighbour(&rects, 3, Direction::Left, |id| id as u64 % 2),
            Some(1)
        );
        assert_eq!(neighbour(&rects, 1, Direction::Left, |_| 0), None);

        assert!(root.remove(2));
        assert_eq!(root.panes(), vec![1, 3]);
        assert!(root.remove(3));
        assert_eq!(root, Node::Leaf(1));
        assert!(!root.remove(1));
    }

    #[test]
    fn remove_with_equalize_only_touches_the_parent_split() {
        // Left column of 1, 2, 3 (weights 1, 2, 1 after a manual resize), right pane 4.
        let col = Node::Split {
            axis: Axis::Stacked,
            children: vec![Node::Leaf(1), Node::Leaf(2), Node::Leaf(3)],
            weights: vec![1.0, 2.0, 1.0],
        };
        let mut root = Node::Split {
            axis: Axis::SideBySide,
            children: vec![col, Node::Leaf(4)],
            weights: vec![3.0, 1.0],
        };
        let mut plain = root.clone();
        plain.remove(1);
        assert!(root.remove_with(1, true));
        let Node::Split {
            children, weights, ..
        } = &root
        else {
            panic!("root collapsed");
        };
        assert_eq!(weights, &vec![3.0, 1.0]);
        let Node::Split { weights: w, .. } = &children[0] else {
            panic!("column collapsed");
        };
        assert_eq!(w, &vec![1.0, 1.0]);
        let Node::Split {
            children: pc,
            weights: pw,
            ..
        } = &plain
        else {
            panic!()
        };
        let Node::Split { weights: w, .. } = &pc[0] else {
            panic!()
        };
        assert_eq!(w, &vec![3.0, 1.0]);
        assert_eq!(pw, &vec![3.0, 1.0]);
    }

    #[test]
    fn same_axis_split_adds_sibling() {
        let mut root = Node::Leaf(1);
        root.split(1, 2, Axis::SideBySide);
        root.split(1, 3, Axis::SideBySide);
        assert_eq!(root.panes(), vec![1, 3, 2]);
    }

    #[test]
    fn panes_exchange_rotate_and_move_to_an_edge() {
        let mut root = Node::Leaf(1);
        root.split(1, 2, Axis::SideBySide);
        root.split(2, 3, Axis::SideBySide);
        assert_eq!(root.panes(), [1, 2, 3]);
        assert!(root.exchange(1));
        assert_eq!(root.panes(), [2, 1, 3]);
        assert!(root.exchange(3));
        assert_eq!(root.panes(), [2, 3, 1]);
        assert!(root.rotate(1, true));
        assert_eq!(root.panes(), [1, 2, 3]);
        assert!(root.rotate(1, false));
        assert_eq!(root.panes(), [2, 3, 1]);
        // To the bottom: under all the others, across the whole width.
        assert!(root.move_to_edge(2, Direction::Down));
        let Node::Split { axis, children, .. } = &root else {
            panic!()
        };
        assert_eq!((*axis, children.len()), (Axis::Stacked, 2));
        assert_eq!(children[1], Node::Leaf(2));
        assert_eq!(root.panes(), [3, 1, 2]);
        // To the left: beside everything.
        assert!(root.move_to_edge(2, Direction::Left));
        assert_eq!(root.panes(), [2, 3, 1]);
        // Alone, nothing moves.
        let mut one = Node::Leaf(1);
        assert!(!one.move_to_edge(1, Direction::Up) && !one.exchange(1));
    }

    #[test]
    fn resizing_by_command_and_by_dragging() {
        let area = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 40,
        };
        let mut root = Node::Leaf(1);
        root.split(1, 2, Axis::SideBySide);
        root.split(2, 3, Axis::Stacked);
        let rects = |root: &Node| {
            let mut v = vec![];
            root.layout(area, &mut v);
            v
        };
        // Wider by 20%: taken from the pane beside it.
        assert!(root.resize(1, Axis::SideBySide, None, 20.0));
        assert_eq!(rects(&root)[0].1.w, 70);
        // Pane 3's height is in its stacked split.
        assert!(root.resize(3, Axis::Stacked, Some(25.0), 0.0));
        assert_eq!(rects(&root)[2].1.h, 10);
        assert!(!root.resize(1, Axis::Stacked, None, 10.0));
        // Never below the minimum share.
        root.resize(1, Axis::SideBySide, None, 90.0);
        assert_eq!(rects(&root)[0].1.w, 95);
        // Dividers: the vertical border, then the horizontal one in the right half.
        let d = root.dividers(area, &flexible);
        assert_eq!(d.len(), 2);
        assert_eq!((d[0].axis, d[0].pos), (Axis::SideBySide, 95));
        root.drag_divider(&d[0], 40);
        assert_eq!(rects(&root)[0].1.w, 40);
        let d = root.dividers(area, &flexible);
        assert_eq!(
            (d[1].axis, d[1].pos, d[1].path.clone()),
            (Axis::Stacked, 30, vec![1])
        );
        root.drag_divider(&d[1], 20);
        assert_eq!(rects(&root)[1].1.h, 20);
        root.equalize();
        assert_eq!(rects(&root)[0].1.w, 50);
        // Closing a pane gives its space to the one before it.
        root.resize(3, Axis::Stacked, Some(25.0), 0.0);
        root.remove(3);
        assert_eq!(rects(&root)[1].1.h, 40);
    }
}
