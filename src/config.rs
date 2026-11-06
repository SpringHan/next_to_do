//! Configuration directory, key bindings and their parsing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gtk4::gdk;
use serde::{Deserialize, Serialize};

/// Everything the user can trigger from the main window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Hide,
    MoveUp,
    MoveDown,
    ToggleDone,
    Add,
    EditTitle,
    Delete,
    SetDeadline,
    MakeOrder,
    MoveItemUp,
    MoveItemDown,
    Reload,
    CalLeft,
    CalRight,
    CalUp,
    CalDown,
    CalPrevMonth,
    CalNextMonth,
    CalToday,
    CalTime,
    CalConfirm,
    CalCancel,
}

impl Action {
    pub const ALL: &'static [Action] = &[
        Action::Quit,
        Action::Hide,
        Action::MoveUp,
        Action::MoveDown,
        Action::ToggleDone,
        Action::Add,
        Action::EditTitle,
        Action::Delete,
        Action::SetDeadline,
        Action::MakeOrder,
        Action::MoveItemUp,
        Action::MoveItemDown,
        Action::Reload,
        Action::CalLeft,
        Action::CalRight,
        Action::CalUp,
        Action::CalDown,
        Action::CalPrevMonth,
        Action::CalNextMonth,
        Action::CalToday,
        Action::CalTime,
        Action::CalConfirm,
        Action::CalCancel,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Action::Quit => "quit",
            Action::Hide => "hide",
            Action::MoveUp => "move_up",
            Action::MoveDown => "move_down",
            Action::ToggleDone => "toggle_done",
            Action::Add => "add",
            Action::EditTitle => "edit_title",
            Action::Delete => "delete",
            Action::SetDeadline => "set_deadline",
            Action::MakeOrder => "make_order",
            Action::MoveItemUp => "move_item_up",
            Action::MoveItemDown => "move_item_down",
            Action::Reload => "reload",
            Action::CalLeft => "cal_left",
            Action::CalRight => "cal_right",
            Action::CalUp => "cal_up",
            Action::CalDown => "cal_down",
            Action::CalPrevMonth => "cal_prev_month",
            Action::CalNextMonth => "cal_next_month",
            Action::CalToday => "cal_today",
            Action::CalTime => "cal_time",
            Action::CalConfirm => "cal_confirm",
            Action::CalCancel => "cal_cancel",
        }
    }

    /// True for the actions that only apply while the deadline calendar is open.
    pub fn is_calendar(self) -> bool {
        matches!(
            self,
            Action::CalLeft
                | Action::CalRight
                | Action::CalUp
                | Action::CalDown
                | Action::CalPrevMonth
                | Action::CalNextMonth
                | Action::CalToday
                | Action::CalTime
                | Action::CalConfirm
                | Action::CalCancel
        )
    }
}

/// Default bindings, written to the config file on first start.
///
/// A value may hold several comma separated keys for the same action.
const DEFAULT_KEYS: &[(&str, &str)] = &[
    ("quit", "q"),
    ("hide", "Escape"),
    ("move_up", "k,Up"),
    ("move_down", "j,Down"),
    ("toggle_done", "space"),
    ("add", "a"),
    ("edit_title", "e"),
    ("delete", "d"),
    ("set_deadline", "t"),
    ("make_order", "o"),
    ("move_item_up", "ctrl+k"),
    ("move_item_down", "ctrl+j"),
    ("reload", "ctrl+r"),
    ("cal_left", "h,Left"),
    ("cal_right", "l,Right"),
    ("cal_up", "k,Up"),
    ("cal_down", "j,Down"),
    ("cal_prev_month", "[,Page_Up"),
    ("cal_next_month", "],Page_Down"),
    ("cal_today", "t"),
    ("cal_time", "Tab"),
    ("cal_confirm", "Return,KP_Enter"),
    ("cal_cancel", "Escape"),
];

/// `~/.config/next_to_do`, honouring `$XDG_CONFIG_HOME`.
pub fn dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_default()
        .join("next_to_do")
}

pub fn data_path() -> PathBuf {
    dir().join("items.json")
}

pub fn config_path() -> PathBuf {
    dir().join("config.json")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Action name -> comma separated key specs, e.g. `"move_down": "j,Down"`.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
}

impl Config {
    pub fn load(path: &Path) -> Config {
        let mut config: Config = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| match serde_json::from_str(&text) {
                Ok(config) => Some(config),
                Err(err) => {
                    eprintln!("next_to_do: cannot parse {}: {err}", path.display());
                    None
                }
            })
            .unwrap_or_default();
        // Bindings added by a newer version keep their default.
        for (action, keys) in DEFAULT_KEYS {
            config
                .keys
                .entry(action.to_string())
                .or_insert_with(|| keys.to_string());
        }
        config
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
            Err(err) => eprintln!("next_to_do: cannot encode config: {err}"),
        }
    }

    /// Parsed bindings; unparsable entries are reported and skipped.
    pub fn bindings(&self) -> Vec<Binding> {
        let mut bindings = Vec::new();
        for action in Action::ALL {
            let Some(spec) = self.keys.get(action.name()) else {
                continue;
            };
            for part in spec.split(',').map(str::trim).filter(|part| !part.is_empty()) {
                match parse_binding(*action, part) {
                    Some(binding) => bindings.push(binding),
                    None => eprintln!(
                        "next_to_do: ignoring invalid key `{part}` for `{}`",
                        action.name()
                    ),
                }
            }
        }
        bindings
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub action: Action,
    key: KeySpec,
    modifiers: gdk::ModifierType,
}

/// A key symbol: a named GDK key or a single printable character.
///
/// `gdk_keyval_from_name` only knows keysym names (`Return`, `bracketleft`,
/// `space`, ...), so punctuation such as `[` has to be kept as a character and
/// compared through `to_unicode` instead.
#[derive(Debug, Clone, Copy)]
enum KeySpec {
    Key(gdk::Key),
    Char(char),
}

impl KeySpec {
    fn matches(self, key: gdk::Key) -> bool {
        match self {
            KeySpec::Key(expected) => expected == key.to_lower(),
            KeySpec::Char(expected) => {
                key.to_unicode().map(|ch| ch.to_ascii_lowercase()) == Some(expected)
            }
        }
    }
}

impl Binding {
    /// Case-insensitive match on the symbol; Shift and Caps Lock are ignored.
    pub fn matches(&self, key: gdk::Key, state: gdk::ModifierType) -> bool {
        let mask = gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SUPER_MASK;
        self.key.matches(key) && (state & mask) == self.modifiers
    }
}

/// Parse specs such as `j`, `Up`, `space`, `[` or `ctrl+k`.
fn parse_binding(action: Action, spec: &str) -> Option<Binding> {
    let mut parts: Vec<&str> = spec.split('+').map(str::trim).collect();
    let key_name = parts.pop()?;
    if key_name.is_empty() {
        return None;
    }
    let mut modifiers = gdk::ModifierType::empty();
    for part in parts {
        modifiers |= match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => gdk::ModifierType::CONTROL_MASK,
            "alt" => gdk::ModifierType::ALT_MASK,
            "super" | "logo" => gdk::ModifierType::SUPER_MASK,
            _ => return None,
        };
    }
    // Friendly aliases for the names users reach for first.
    let key_name = match key_name.to_ascii_lowercase().as_str() {
        "enter" => "Return",
        "esc" => "Escape",
        _ => key_name,
    };
    let key = match gdk::Key::from_name(key_name) {
        Some(key) => KeySpec::Key(key.to_lower()),
        None => {
            let mut chars = key_name.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => KeySpec::Char(ch.to_ascii_lowercase()),
                _ => return None,
            }
        }
    };
    Some(Binding {
        action,
        key,
        modifiers,
    })
}
