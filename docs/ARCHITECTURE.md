# Architecture

## Overview

`next_to_do` is a single Rust binary that renders a small todo panel with GTK4 on
top of a Wayland layer-shell surface. It runs on Niri (and any other compositor
implementing `zwlr_layer_shell_v1`) and exposes a one-shot CLI mode so that
`waybar` can poll today's unfinished item counts.

The binary has two modes:

| Invocation          | Behaviour                                                        |
| ------------------- | ---------------------------------------------------------------- |
| `next_to_do`        | Opens the GUI (the only long-running mode, bundled with the GUI). |
| `next_to_do waybar` | Prints today's counts and exits; no GTK/display access needed.    |

## Module layout

| File           | Responsibility                                                                     |
| -------------- | ---------------------------------------------------------------------------------- |
| `src/main.rs`  | Argument dispatch, the `waybar` output, help text.                                  |
| `src/model.rs` | `Item`, `Kind`, `Data`: persistence, mutation and the display-order algorithm.       |
| `src/config.rs`| Config directory, `Config` (the key map) and `Binding` parsing/matching.            |
| `src/ui.rs`    | GTK4 window, CSS theme, widget tree, keyboard and mouse handling, editing modes.     |

`main.rs` owns no state: everything GUI related lives in `ui.rs`, and everything
that survives a restart lives in `model.rs` and `config.rs`.

## Data model

```rust
struct Item {
    id: u64,
    title: String,
    done: bool,
    kind: Kind,                       // Order | Time
    deadline: Option<DateTime<Local>>,// Time items only
    prev: Option<u64>,                // Order items only
}
```

`Kind::Order` items are order-bounded: they exist independently of the calendar
and are positioned relative to `prev`. `Kind::Time` items are time-bounded: they
carry a `deadline` and are shown once their local deadline day is reached.
`Order` items ignore `deadline`, `Time` items ignore `prev`.

Visibility rules (`Item::is_visible`):

* order-bounded items are always visible (including completed ones, so that they
  can be un-completed);
* time-bounded items are visible from their deadline day onwards, so overdue
  items stay in the list and are marked as overdue; a time-bounded item without a
  deadline stays visible too, instead of silently disappearing.

Completed items are rendered dimmed and struck through but are excluded from the
counts used by `waybar`.

## Display order

`Data::ordered(today)` builds the list of ids shown in the panel:

1. Collect the visible items.
2. Take every visible order-bounded item and group it under its `prev`. Items
   whose `prev` is `None`, points to an invisible item or points to itself are
   *roots*, kept in insertion order.
3. Emit each root followed, depth-first, by everything chained behind it. Because
   roots are emitted first, order-bounded items always sit above time-bounded
   items, and an item without a predecessor appears at the very top.
4. Sort the visible time-bounded items by `(deadline, id)` and emit them. If a
   time-bounded item is the `prev` of an order-bounded item, that follower is
   emitted right after it (this is what makes `prev` work across both kinds).
5. Append anything still unvisited (cycles, dangling `prev`) in insertion order so
   no visible item is ever lost.

Reordering (`move_up` / `move_down`) only rewrites `prev` links: moving up swaps
the item with its predecessor, moving down swaps it with its first follower.
Deleting an item splices its followers onto its own predecessor.

## Persistence

Both files live in `~/.config/next_to_do/` (or `$XDG_CONFIG_HOME/next_to_do/`).

`items.json`:

```json
{
  "next_id": 3,
  "items": [
    { "id": 1, "title": "water the plants", "done": false, "kind": "order" },
    {
      "id": 2,
      "title": "standup",
      "done": false,
      "kind": "time",
      "deadline": "2026-11-06T10:00:00+08:00"
    },
    {
      "id": 3,
      "title": "review PR",
      "done": false,
      "kind": "order",
      "prev": 2
    }
  ]
}
```

`config.json` (written on first start, edit it while the app is closed or press
the reload key afterwards):

```json
{
  "keys": {
    "quit": "q",
    "hide": "Escape",
    "move_up": "k,Up",
    "move_down": "j,Down",
    "toggle_done": "space",
    "add": "a",
    "edit_title": "e",
    "delete": "d",
    "set_deadline": "t",
    "make_order": "o",
    "move_item_up": "ctrl+k",
    "move_item_down": "ctrl+j",
    "reload": "ctrl+r"
  }
}
```

Both files are written with `serde_json`. Unknown or missing actions fall back to
the default binding, so adding actions in a later version does not invalidate an
existing file.

## Key bindings

Every action is described by `config::Action` and has a default. A value is a
comma separated list of alternative keys (`"j,Down"`). A key spec is
`[modifier+]*key`, where the modifiers are `ctrl`, `alt` and `super`, and the key
is any name understood by `gdk_keyval_from_name` (`j`, `space`, `Up`, `Escape`,
`F1`, ...).

Matching (`Binding::matches`) compares the lower-cased key symbol and only the
`ctrl`/`alt`/`super` bits, so `Shift` and `Caps Lock` do not change a binding and
`k` matches both `k` and `K`.

## GUI

* The window is a plain `gtk::Window` (undecorated, non-resizable) turned into a
  layer surface with the `gtk4-layer-shell` bindings:
  * `Layer::Overlay` — always above normal windows;
  * namespace `next-to-do`;
  * `KeyboardMode::Exclusive` — the panel receives key events without the
    compositor forwarding them elsewhere, which is why plain TUI-style keys work;
  * no anchors — a layer surface without anchors is centred by the compositor,
    which is what gives the "centred popup" look on Niri.
* If the compositor does not support layer shell, the window is shown as a normal
  toplevel instead of aborting.
* The window is created once and later toggled with `set_visible`. A second
  `next_to_do` invocation activates the running instance (GApplication single
  instance behaviour) and toggles the panel; `q` quits for real.

Widget tree:

```
Window (transparent, class `next-to-do`)
└── Box.panel (dark rounded background, 420px wide)
    ├── Box.header   "TODAY" + date
    ├── ScrolledWindow (max content height 460)
    │   └── Box.rows  one Box.row per item: check button, title, meta
    ├── Box.input-bar (hidden unless editing): ">" + Entry
    ├── Label.status  inline error, e.g. a malformed deadline
    └── Box.footer    open/total counter + key legend
```

The theme is a single CSS string installed for the default display at
`STYLE_PROVIDER_PRIORITY_APPLICATION`: near-black panel (`rgba(20,20,20,0.97)`),
22px rounded corners, mint green accent (`#a6e3a1`), white titles and grey
metadata, monospace font, matching `docs/sample.png`.

## Interaction

There are two modes:

* **normal** — keys are matched against the bindings; the selected row is
  highlighted.
* **input** — the entry line is open for a new title, a rename or a deadline
  (`YYYY-MM-DD HH:MM`, `YYYY-MM-DD HH:MM:SS` or `YYYY-MM-DD`, the latter meaning
  23:59). `Enter` submits, `Escape` cancels. While the entry is open every key
  except `Escape` is forwarded to it.

Mouse and keyboard are equivalent:

| Mouse                          | Key                       |
| ------------------------------ | ------------------------- |
| click a row                    | `j`/`k`, `Down`/`Up`      |
| click the check button         | `space`                   |

The key controller is attached to the window in the *capture* phase, so it sees
events before the focused widget and can decide whether the entry belongs to the
input mode or not.

Setting a deadline turns the item into a time-bounded one; `make_order` turns it
back and appends it at the end of the order-bounded chain.

## Waybar integration

`next_to_do waybar` prints `format!("{}:{}", order_bounded, time_bounded)` for
today's **unfinished** items, for example `2:1`. It only reads `items.json`, so it
is cheap enough to poll.

```jsonc
// ~/.config/waybar/config.jsonc
"custom/nexttodo": {
    "exec": "next_to_do waybar",
    "interval": 30,
    "format": " {}",
    "tooltip": false
}
```

The panel itself can be bound in Niri:

```kdl
binds {
    Mod+Shift+T { spawn "next_to_do"; }
}
```

## Build

The build needs the GTK4 and gtk4-layer-shell development packages
(`libgtk-4-dev`, `libgtk4-layer-shell-dev` on Debian/Ubuntu, `gtk4`,
`gtk4-layer-shell` on Arch) plus a Rust toolchain with edition 2024 support.

```sh
cargo build --release
```
