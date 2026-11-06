# Next To Do

English | [中文文档](README.zh.md)

A small, always-on-top todo panel for Wayland compositors such as
[Niri](https://github.com/YaLTeR/niri), written in Rust with GTK4 and
`gtk4-layer-shell`. It pairs with [waybar](https://github.com/Alexays/Waybar) by
printing today's unfinished item counts.
The style is inspired by [waycal](https://github.com/ForrestKnight/waycal).

![demo](docs/demo.png)

## Features

- Centred, rounded, always-on-top panel (layer-shell `overlay`)
- Order-bounded items (positioned after a predecessor) and time-bounded items
  (positioned by a deadline)
- A built-in month calendar to browse any day; days that still have open items
  carry a dot
- TUI-style single-key controls, every binding configurable
- Mouse and keyboard cover the same actions
- `next_to_do waybar` prints `order-bounded:time-bounded` for today
- State and configuration under `~/.config/next_to_do/`

## Requirements

- A Wayland compositor with layer-shell support (Niri, Hyprland, Sway, ...)
- GTK4 and gtk4-layer-shell development packages:
  - Arch: `gtk4 gtk4-layer-shell`
  - Debian/Ubuntu: `libgtk-4-dev libgtk4-layer-shell-dev`
- Rust with edition 2024 support (1.85+)

## Build and install

```sh
cargo build --release
install -Dm755 target/release/next_to_do ~/.local/bin/next_to_do
```

## Usage

```sh
next_to_do          # open the panel
next_to_do waybar   # print "order-bounded:time-bounded" for today
next_to_do help     # usage and file locations
```

The panel is created once and kept alive; running `next_to_do` again toggles it.
`q` quits; `Escape` leaves the calendar (and hides the panel from the list).

### Keys

The panel has two panes: the calendar on top and the item list below. The calendar
is hidden by default; `Tab` opens it and moves the keyboard to it (green outline),
and `Tab`, `Enter` or `Escape` close it again. Each pane only reacts to its own
keys, so `j`/`k` mean "next item" in the list and "next week" in the calendar.

| Key        | Action                                         |
| ---------- | ---------------------------------------------- |
| `j` / `↓`  | select next item                               |
| `k` / `↑`  | select previous item                           |
| `space`    | toggle selected item done / not done           |
| `a`        | add an order-bounded item                      |
| `e`        | rename the selected item                       |
| `t`        | set the deadline on the viewed day (time-bound)|
| `o`        | drop the deadline (back to order-bounded)      |
| `d`        | delete the selected item                       |
| `Ctrl+j`   | move the selected item down                    |
| `Ctrl+k`   | move the selected item up                      |
| `Ctrl+r`   | reload `items.json` from disk                  |
| `Tab`      | switch to the calendar                         |
| `Escape`   | hide the panel                                 |
| `q`        | quit                                           |

Clicking a row selects it, clicking its `○` marks it done.

### Calendar

`Tab` opens the calendar; it always shows the day whose items are listed, so
another day's items are reached by moving the highlighted day. Days that still
have unfinished time-bounded items show a dot.

| Key               | Action                                   |
| ----------------- | ---------------------------------------- |
| `h` / `←`         | previous day                             |
| `l` / `→`         | next day                                 |
| `k` / `↑`         | previous week                            |
| `j` / `↓`         | next week                                |
| `[` / `Page_Up`   | previous month (same day when it exists) |
| `]` / `Page_Down` | next month                               |
| `t`               | jump to today                            |
| `Enter` / `Tab`   | close the calendar                        |
| `Escape`          | close the calendar                        |

`t` on an item keeps the day the calendar shows: the deadline edit only asks for
the time (`HH:MM`) and the calendar stays navigable, so the day can still be
changed before `Enter` confirms. Days can be clicked as well.

All of these keys can be changed in `~/.config/next_to_do/config.json`, e.g.

```json
{
  "keys": {
    "move_down": "j,Down,n",
    "toggle_done": "space,Enter",
    "quit": "q,Ctrl+q",
    "cal_next_month": "],Page_Down,n",
    "cal_confirm": "Return,Enter"
  }
}
```

### Waybar

```jsonc
"custom/nexttodo": {
    "exec": "next_to_do waybar",
    "interval": 30,
    "format": " {}",
    "tooltip": false
}
```

### Niri

```kdl
binds {
    Mod+Shift+T { spawn "next_to_do"; }
}
```

## Data

- `~/.config/next_to_do/items.json` — the items
- `~/.config/next_to_do/config.json` — the key bindings

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the data model, the ordering
rules and the waybar output.
