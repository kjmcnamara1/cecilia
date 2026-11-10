//! How often and how lately things were used, ranked the way zoxide ranks directories: each
//! item keeps a count and the time it was last used; its score is the count times a weight
//! that falls as the last use gets older. Used to order tab completion of commands and the
//! buffer, recent file and workspace pickers.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// What is being ranked; each has its own list of items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Commands,
    Buffers,
    Recent,
    Workspaces,
}

impl Domain {
    fn name(self) -> &'static str {
        match self {
            Domain::Commands => "commands",
            Domain::Buffers => "buffers",
            Domain::Recent => "recent",
            Domain::Workspaces => "workspaces",
        }
    }
}

/// When the counts of a domain add up to more than this, all are scaled down (and the ones
/// that fall below 1 forgotten), so old habits fade and the file stays small.
const MAX_TOTAL: f64 = 1000.0;

/// Item → (count, Unix time of the last use), per domain.
type Table = BTreeMap<String, BTreeMap<String, (f64, u64)>>;

#[derive(Debug, Default)]
pub struct Frecency {
    table: Table,
    /// Where the table is kept between sessions (set by the frontends, never by tests).
    file: Option<PathBuf>,
}

/// The time now, in seconds since the Unix epoch.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn read(path: &PathBuf) -> Table {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

impl Frecency {
    /// Keeps the table in `path` from now on, loading what is there.
    pub fn keep(&mut self, path: PathBuf) {
        self.table = read(&path);
        self.file = Some(path);
    }

    /// The count times a weight for how long ago the item was last used (zoxide's: ×4 within
    /// the hour, ×2 within the day, ×½ within the week, ×¼ after); 0 for an unknown item.
    pub fn score(&self, domain: Domain, key: &str, now: u64) -> f64 {
        let Some(&(count, last)) = self.table.get(domain.name()).and_then(|d| d.get(key)) else {
            return 0.0;
        };
        count
            * match now.saturating_sub(last) {
                s if s < 3600 => 4.0,
                s if s < 86400 => 2.0,
                s if s < 604_800 => 0.5,
                _ => 0.25,
            }
    }

    /// Notes a use of an item. With a file, what other sessions wrote since is merged in first,
    /// and the result is written back (by renaming a temporary file over it).
    pub fn bump(&mut self, domain: Domain, key: &str, now: u64) {
        if let Some(path) = &self.file {
            self.table = read(path);
        }
        let items = self.table.entry(domain.name().into()).or_default();
        let entry = items.entry(key.into()).or_insert((0.0, now));
        entry.0 += 1.0;
        entry.1 = now;
        let total: f64 = items.values().map(|e| e.0).sum();
        if total > MAX_TOTAL {
            let scale = 0.9 * MAX_TOTAL / total;
            items.values_mut().for_each(|e| e.0 *= scale);
            items.retain(|_, e| e.0 >= 1.0);
        }
        if let Some(path) = &self.file {
            // Failing to save must never raise a notification.
            let tmp = path.with_extension("tmp");
            if let Ok(text) = serde_json::to_string(&self.table) {
                if std::fs::write(&tmp, text).is_ok() {
                    let _ = std::fs::rename(&tmp, path);
                }
            }
        }
    }

    /// Puts the items with the highest scores first; equal ones keep their order.
    pub fn sort<T>(&self, domain: Domain, items: &mut [T], key: impl Fn(&T) -> String) {
        let now = unix_now();
        // Scores are never negative, so their bits order like the numbers.
        items.sort_by_cached_key(|i| std::cmp::Reverse(self.score(domain, &key(i), now).to_bits()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_and_frequent_things_score_higher() {
        let mut f = Frecency::default();
        let day = 86400;
        let now = 100 * day;
        // Used often, long ago; used once, just now; used twice, two days ago.
        for _ in 0..10 {
            f.bump(Domain::Commands, "old", now - 30 * day);
        }
        f.bump(Domain::Commands, "new", now);
        f.bump(Domain::Commands, "mid", now - 2 * day);
        f.bump(Domain::Commands, "mid", now - 2 * day);
        let s = |k| f.score(Domain::Commands, k, now);
        assert_eq!(s("new"), 4.0);
        assert_eq!(s("mid"), 1.0);
        assert_eq!(s("old"), 2.5);
        assert_eq!(s("unknown"), 0.0);
        // Another domain is separate.
        assert_eq!(f.score(Domain::Buffers, "new", now), 0.0);
    }

    #[test]
    fn sort_is_stable_for_equal_scores() {
        let mut f = Frecency::default();
        f.bump(Domain::Buffers, "b", unix_now());
        let mut items = vec!["a", "c", "b", "d"];
        f.sort(Domain::Buffers, &mut items, |s| s.to_string());
        assert_eq!(items, ["b", "a", "c", "d"]);
    }

    #[test]
    fn old_counts_age_away_and_the_file_is_shared() {
        let dir = std::env::temp_dir().join(format!("cecilia-frecency-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("frecency.json");
        let mut a = Frecency::default();
        a.keep(file.clone());
        let mut b = Frecency::default();
        b.keep(file.clone());
        a.bump(Domain::Recent, "x", 10);
        b.bump(Domain::Recent, "y", 10);
        // b merged what a wrote before adding its own.
        let mut c = Frecency::default();
        c.keep(file);
        assert!(c.score(Domain::Recent, "x", 10) > 0.0);
        assert!(c.score(Domain::Recent, "y", 10) > 0.0);
        // Past the cap, counts shrink and the rarely used are forgotten.
        let mut f = Frecency::default();
        f.bump(Domain::Commands, "rare", 10);
        for _ in 0..1100 {
            f.bump(Domain::Commands, "often", 10);
        }
        assert_eq!(f.score(Domain::Commands, "rare", 10), 0.0);
        assert!(f.score(Domain::Commands, "often", 10) < 4.0 * MAX_TOTAL);
        std::fs::remove_dir_all(dir).ok();
    }
}
