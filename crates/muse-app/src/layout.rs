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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Leaf(PaneId),
    Split { axis: Axis, children: Vec<Node> },
}

impl Node {
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
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { axis: a, children } => {
                if *a == axis {
                    if let Some(i) = children.iter().position(|c| *c == Node::Leaf(target)) {
                        children.insert(i + 1, Node::Leaf(new));
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
        let Node::Split { children, .. } = self else {
            return !matches!(self, Node::Leaf(id) if *id == target);
        };
        if let Some(i) = children.iter().position(|c| *c == Node::Leaf(target)) {
            children.remove(i);
        } else {
            for c in children.iter_mut() {
                c.remove(target);
            }
        }
        if children.len() == 1 {
            *self = children.pop().unwrap();
        }
        true
    }

    /// Computes pane rectangles by dividing space equally.
    pub fn layout(&self, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
        match self {
            Node::Leaf(id) => out.push((*id, area)),
            Node::Split { axis, children } => {
                let n = children.len() as u16;
                let total = match axis {
                    Axis::Stacked => area.h,
                    Axis::SideBySide => area.w,
                };
                let mut offset = 0;
                for (i, child) in children.iter().enumerate() {
                    let i = i as u16;
                    let size = total / n + u16::from(i < total % n);
                    let r = match axis {
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
                    };
                    child.layout(r, out);
                    offset += size;
                }
            }
        }
    }
}

/// Finds the pane adjacent to `from` in `dir`, preferring the one aligned with its centre.
pub fn neighbour(rects: &[(PaneId, Rect)], from: PaneId, dir: Direction) -> Option<PaneId> {
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
            Some(((dist, off_centre), *id))
        })
        .min()
        .map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(neighbour(&rects, 1, Direction::Right), Some(2));
        assert_eq!(neighbour(&rects, 2, Direction::Down), Some(3));
        assert_eq!(neighbour(&rects, 3, Direction::Left), Some(1));
        assert_eq!(neighbour(&rects, 1, Direction::Left), None);

        assert!(root.remove(2));
        assert_eq!(root.panes(), vec![1, 3]);
        assert!(root.remove(3));
        assert_eq!(root, Node::Leaf(1));
        assert!(!root.remove(1));
    }

    #[test]
    fn same_axis_split_adds_sibling() {
        let mut root = Node::Leaf(1);
        root.split(1, 2, Axis::SideBySide);
        root.split(1, 3, Axis::SideBySide);
        assert_eq!(root.panes(), vec![1, 3, 2]);
    }
}
