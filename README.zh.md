# Next To Do

一个运行在 Wayland 桌面环境（如 [Niri](https://github.com/YaLTeR/niri)）上的小型悬浮代办事项面板，
使用 Rust、GTK4 与 `gtk4-layer-shell` 编写，并可为
[waybar](https://github.com/Alexays/Waybar) 提供今日未完成事项的计数。

![样式参考](docs/sample.png)

## 特性

- 居中、圆角、始终置顶的面板（layer-shell `overlay`）
- 由顺序框定的 `order-bounded` 事项与由时间框定的 `time-bounded` 事项
- TUI 风格的单键操作，所有按键均可配置
- 鼠标操作与键盘操作一一对应
- `next_to_do waybar` 输出今日的 `order-bounded:time-bounded` 计数
- 数据与配置存放于 `~/.config/next_to_do/`

## 依赖

- 支持 layer-shell 的 Wayland 合成器（Niri、Hyprland、Sway 等）
- GTK4 与 gtk4-layer-shell 开发包：
  - Arch：`gtk4 gtk4-layer-shell`
  - Debian/Ubuntu：`libgtk-4-dev libgtk4-layer-shell-dev`
- 支持 edition 2024 的 Rust（1.85+）

## 构建与安装

```sh
cargo build --release
install -Dm755 target/release/next_to_do ~/.local/bin/next_to_do
```

## 使用

```sh
next_to_do          # 打开面板
next_to_do waybar   # 输出今日的 "order-bounded:time-bounded"
next_to_do help     # 查看用法与文件位置
```

面板只创建一次并常驻；再次运行 `next_to_do` 会切换其显示状态。
`Escape` 隐藏面板，`q` 退出程序。

### 按键

| 按键       | 功能                                  |
| ---------- | ------------------------------------- |
| `j` / `↓`  | 选中下一项                            |
| `k` / `↑`  | 选中上一项                            |
| `space`    | 切换选中项的完成状态                  |
| `a`        | 新增 `order-bounded` 事项             |
| `e`        | 重命名选中项                          |
| `t`        | 打开 deadline 日历（转为 `time-bounded`） |
| `o`        | 取消 deadline（转回 `order-bounded`） |
| `d`        | 删除选中项                            |
| `Ctrl+j`   | 将选中项下移                          |
| `Ctrl+k`   | 将选中项上移                          |
| `Ctrl+r`   | 从磁盘重新载入 `items.json`           |
| `Escape`   | 隐藏面板                              |
| `q`        | 退出                                  |

点击某一行可选中它，点击行首的 `○` 可标记完成。

### deadline 日历

按 `t` 会为选中项打开一个日历，列表区域会被月历取代，被高亮的那一天就是最终保存的日期。

| 按键              | 功能                                 |
| ----------------- | ------------------------------------ |
| `h` / `←`         | 前一天                               |
| `l` / `→`         | 后一天                               |
| `k` / `↑`         | 上一周                               |
| `j` / `↓`         | 下一周                               |
| `[` / `Page_Up`   | 上一个月（日期存在时保持同一天）     |
| `]` / `Page_Down` | 下一个月                             |
| `t`               | 回到今天                             |
| `Tab`             | 改为编辑时间（`HH:MM`）              |
| `Enter`           | 保存日期与时间                       |
| `Escape`          | 取消                                 |

日历中的日期也可以直接点击，鼠标能做到的事都有对应的按键。

以上按键都可以在 `~/.config/next_to_do/config.json` 中修改，例如：

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

## 数据文件

- `~/.config/next_to_do/items.json` — 事项数据
- `~/.config/next_to_do/config.json` — 按键配置

数据模型、排序规则与 waybar 接口的说明见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)。
