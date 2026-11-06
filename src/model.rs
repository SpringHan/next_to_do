//! Persisted todo items and the rules that turn them into today's list.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{DateTime, Local, NaiveDate};
use serde::{Deserialize, Serialize};

/// How an item is positioned: by a deadline or by its predecessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Order-bounded: shown every day, placed after its predecessor.
    Order,
    /// Time-bounded: shown from its deadline day onwards.
    Time,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub done: bool,
    pub kind: Kind,
    /// Deadline of a time-bounded item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Local>>,
    /// Predecessor of an order-bounded item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev: Option<u64>,
}

impl Item {
    /// True when the item belongs to the list of `date`.
    ///
    /// Order-bounded items belong to every day; time-bounded items belong to the
    /// day of their deadline. A time-bounded item without a deadline is kept
    /// visible instead of disappearing from every day.
    pub fn is_visible(&self, date: NaiveDate) -> bool {
        match self.kind {
            Kind::Order => true,
            Kind::Time => self
                .deadline
                .as_ref()
                .map(|deadline| deadline.date_naive() == date)
                .unwrap_or(true),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Data {
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub items: Vec<Item>,
}

impl Data {
    pub fn load(path: &Path) -> Data {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                eprintln!("next_to_do: cannot parse {}: {err}", path.display());
                Data::default()
            }),
            Err(_) => Data::default(),
        }
    }

    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                eprintln!("next_to_do: cannot create {}: {err}", parent.display());
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(err) = std::fs::write(path, text) {
                    eprintln!("next_to_do: cannot write {}: {err}", path.display());
                }
            }
            Err(err) => eprintln!("next_to_do: cannot encode data: {err}"),
        }
    }

    pub fn get(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    /// Append a new item; order-bounded items are chained after the last one.
    pub fn add(&mut self, title: &str, kind: Kind) -> u64 {
        let id = self
            .next_id
            .max(self.items.iter().map(|item| item.id).max().unwrap_or(0) + 1);
        self.next_id = id + 1;
        let prev = match kind {
            Kind::Order => self.last_order(Some(id)),
            Kind::Time => None,
        };
        self.items.push(Item {
            id,
            title: title.to_string(),
            done: false,
            kind,
            deadline: None,
            prev,
        });
        id
    }

    /// Id of the last order-bounded item, so that new items are appended at the end.
    fn last_order(&self, except: Option<u64>) -> Option<u64> {
        self.items
            .iter()
            .rev()
            .find(|item| item.kind == Kind::Order && Some(item.id) != except)
            .map(|item| item.id)
    }

    /// Remove an item and splice its successors onto its own predecessor.
    pub fn remove(&mut self, id: u64) {
        let prev = self.get(id).and_then(|item| item.prev);
        self.items.retain(|item| item.id != id);
        for item in &mut self.items {
            if item.prev == Some(id) {
                item.prev = prev;
            }
        }
    }

    /// Move an item in front of the predecessor it is attached to.
    pub fn move_up(&mut self, id: u64) {
        let Some(prev_id) = self.get(id).and_then(|item| item.prev) else {
            return;
        };
        let Some(prev_item) = self.get(prev_id) else {
            return;
        };
        if prev_item.kind != Kind::Order {
            return;
        }
        let before = prev_item.prev;
        if let Some(item) = self.get_mut(id) {
            item.prev = before;
        }
        if let Some(item) = self.get_mut(prev_id) {
            item.prev = Some(id);
        }
    }

    /// Move an item behind its first successor.
    pub fn move_down(&mut self, id: u64) {
        let Some(child) = self
            .items
            .iter()
            .find(|item| item.prev == Some(id) && item.kind == Kind::Order)
            .map(|item| item.id)
        else {
            return;
        };
        let before = self.get(id).and_then(|item| item.prev);
        if before == Some(child) {
            return;
        }
        if let Some(item) = self.get_mut(id) {
            item.prev = Some(child);
        }
        if let Some(item) = self.get_mut(child) {
            item.prev = before;
        }
    }

    /// Turn an item into a time-bounded one and drop its position in the chain.
    pub fn set_deadline(&mut self, id: u64, deadline: DateTime<Local>) {
        if let Some(item) = self.get_mut(id) {
            item.kind = Kind::Time;
            item.deadline = Some(deadline);
            item.prev = None;
        }
    }

    /// Turn an item back into an order-bounded one; it is appended to the end.
    pub fn make_order(&mut self, id: u64) {
        let prev = self.last_order(Some(id));
        if let Some(item) = self.get_mut(id) {
            item.kind = Kind::Order;
            item.deadline = None;
            item.prev = prev;
        }
    }

    /// Ids of the items shown for `date`, in display order.
    ///
    /// Order-bounded chains without a visible predecessor come first, then the
    /// time-bounded items of that day sorted by deadline. An order-bounded item
    /// whose predecessor is a time-bounded item follows that item instead.
    pub fn ordered(&self, date: NaiveDate) -> Vec<u64> {
        let visible: Vec<&Item> = self
            .items
            .iter()
            .filter(|item| item.is_visible(date))
            .collect();
        let visible_ids: HashSet<u64> = visible.iter().map(|item| item.id).collect();

        let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
        let mut roots = Vec::new();
        for item in visible.iter().filter(|item| item.kind == Kind::Order) {
            match item.prev {
                Some(prev) if prev != item.id && visible_ids.contains(&prev) => {
                    children.entry(prev).or_default().push(item.id);
                }
                _ => roots.push(item.id),
            }
        }

        let mut ordered = Vec::with_capacity(visible.len());
        let mut seen = HashSet::with_capacity(visible.len());
        for root in roots {
            push_chain(root, &children, &mut seen, &mut ordered);
        }

        let mut timed: Vec<&Item> = visible
            .iter()
            .copied()
            .filter(|item| item.kind == Kind::Time)
            .collect();
        timed.sort_by(|a, b| a.deadline.cmp(&b.deadline).then(a.id.cmp(&b.id)));
        for item in timed {
            push_chain(item.id, &children, &mut seen, &mut ordered);
        }

        // Everything left over (cycles, dangling predecessors) still deserves a row.
        for item in visible {
            push_chain(item.id, &children, &mut seen, &mut ordered);
        }
        ordered
    }

    /// `(order-bounded, time-bounded)` counts of the unfinished items of `date`.
    pub fn counts(&self, date: NaiveDate) -> (usize, usize) {
        let mut order = 0;
        let mut time = 0;
        for item in self
            .items
            .iter()
            .filter(|item| !item.done && item.is_visible(date))
        {
            match item.kind {
                Kind::Order => order += 1,
                Kind::Time => time += 1,
            }
        }
        (order, time)
    }
}

/// Append `id` and, recursively, everything chained behind it.
fn push_chain(
    id: u64,
    children: &HashMap<u64, Vec<u64>>,
    seen: &mut HashSet<u64>,
    ordered: &mut Vec<u64>,
) {
    if !seen.insert(id) {
        return;
    }
    ordered.push(id);
    if let Some(followers) = children.get(&id) {
        for &follower in followers {
            push_chain(follower, children, seen, ordered);
        }
    }
}
