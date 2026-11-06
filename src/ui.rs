//! The layer-shell window: theme, widget tree, keyboard and mouse handling.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use chrono::{DateTime, Datelike, Days, Local, NaiveDate, TimeZone, Timelike};
use gtk4 as gtk;

use gtk4::gio::prelude::{ApplicationExt as _, ApplicationExtManual as _};
use gtk4::prelude::*;
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

use crate::config::{self, Action, Binding};
use crate::model::{Data, Kind};

/// Dark panel with a green accent, modelled after `docs/sample.png`.
const CSS: &str = r#"
window.next-to-do {
    background-color: transparent;
    font-family: "JetBrains Mono", "Fira Code", "DejaVu Sans Mono", monospace;
}

.panel {
    background-color: rgba(20, 20, 20, 0.97);
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 22px;
    padding: 18px 20px 12px 20px;
}

.header {
    margin-bottom: 10px;
}

.heading {
    color: #a6e3a1;
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 3px;
}

.date {
    color: #8a8a8a;
    font-size: 12px;
    letter-spacing: 1px;
}

.row {
    padding: 7px 10px;
    border-radius: 10px;
    border-left: 3px solid transparent;
}

.row:hover {
    background-color: rgba(255, 255, 255, 0.04);
}

.row.selected {
    background-color: rgba(166, 227, 161, 0.12);
    border-left: 3px solid #a6e3a1;
}

.check {
    background-color: transparent;
    background-image: none;
    border: none;
    box-shadow: none;
    padding: 0;
    min-width: 16px;
    min-height: 16px;
    color: #6f6f6f;
    font-size: 13px;
}

.row.selected .check {
    color: #a6e3a1;
}

.row.done .check {
    color: #4a4a4a;
}

.title {
    color: #eaeaea;
    font-size: 14px;
}

.row.selected .title {
    color: #ffffff;
}

.row.done .title {
    color: #6a6a6a;
    text-decoration-line: line-through;
}

.meta {
    color: #8a8a8a;
    font-size: 12px;
}

.row.done .meta {
    color: #555555;
}

.empty {
    color: #5a5a5a;
    font-size: 13px;
}

.input-bar {
    margin-top: 8px;
    padding: 6px 10px;
    border-radius: 10px;
    background-color: rgba(255, 255, 255, 0.05);
}

.prompt {
    color: #a6e3a1;
    font-weight: 700;
}

.input-bar entry {
    background-color: transparent;
    background-image: none;
    border: none;
    box-shadow: none;
    color: #ffffff;
    font-size: 14px;
}

.status {
    color: #e06c75;
    font-size: 12px;
    margin-top: 4px;
}

.footer {
    margin-top: 10px;
}

.count {
    color: #a6e3a1;
    font-size: 12px;
}

.hint {
    color: #6f6f6f;
    font-size: 11px;
}

.calendar {
    margin-top: 2px;
    margin-bottom: 8px;
    padding: 4px;
    border: 1px solid transparent;
    border-radius: 14px;
}

.calendar.focused {
    border-color: rgba(166, 227, 161, 0.35);
}

.calendar-item {
    color: #8a8a8a;
    font-size: 12px;
}

.month {
    color: #ffffff;
    font-size: 14px;
    font-weight: 700;
    margin-top: 2px;
    margin-bottom: 4px;
}

.weekday {
    color: #a6e3a1;
    font-size: 11px;
    font-weight: 700;
    padding: 2px 0;
}

.day {
    background-color: transparent;
    background-image: none;
    border: none;
    box-shadow: none;
    padding: 1px 0;
    min-height: 26px;
    border-radius: 8px;
    color: #eaeaea;
}

.day-number {
    font-size: 12px;
}

.dot {
    color: #a6e3a1;
    font-size: 8px;
    min-height: 8px;
}

.day.other .dot {
    color: #3f5a3f;
}

.day.selected .dot {
    color: #141414;
}

.day:hover {
    background-color: rgba(255, 255, 255, 0.06);
}

.day.other {
    color: #5a5a5a;
}

.day.today {
    color: #a6e3a1;
    font-weight: 700;
}

.day.selected {
    background-color: #a6e3a1;
    color: #141414;
    font-weight: 700;
}
"#;

/// Which pane owns the keys while nothing is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    List,
    Calendar,
}

/// What the entry line or the calendar is currently editing.
#[derive(Debug, Clone, Copy)]
enum Editing {
    NewItem,
    Title(u64),
    /// The deadline day comes from the calendar, the entry only holds `HH:MM`.
    Deadline { id: u64, time_focus: bool },
}

impl Editing {
    fn placeholder(self) -> &'static str {
        match self {
            Editing::NewItem => "new item, Enter to save, Escape to cancel",
            Editing::Title(_) => "new title, Enter to save, Escape to cancel",
            Editing::Deadline { .. } => "time `HH:MM`, Tab to adjust the date",
        }
    }
}

struct App {
    data: Data,
    config: config::Config,
    bindings: Vec<Binding>,
    selected: Option<u64>,
    /// Day whose items are listed and highlighted in the calendar.
    view_date: NaiveDate,
    focus: Focus,
    editing: Option<Editing>,
    status: Option<String>,
}

impl App {
    fn save(&self) {
        self.data.save(&config::data_path());
    }
}

struct Ui {
    window: gtk::Window,
    rows: gtk::Box,
    calendar: gtk::Box,
    calendar_item: gtk::Label,
    month_label: gtk::Label,
    calendar_days: gtk::Box,
    header_heading: gtk::Label,
    header_date: gtk::Label,
    footer_count: gtk::Label,
    hint: gtk::Label,
    input_bar: gtk::Box,
    entry: gtk::Entry,
    status: gtk::Label,
}

/// A row snapshot, so that the list can be rebuilt without borrowing `App`.
struct Row {
    id: u64,
    title: String,
    meta: String,
    done: bool,
    selected: bool,
}

pub fn run() {
    let application = gtk::Application::builder()
        .application_id("io.github.next_to_do")
        .build();
    application.connect_activate(activate);
    let _ = application.run();
}

fn activate(application: &gtk::Application) {
    // A second launch only toggles the window of the running instance.
    let windows = application.windows();
    if let Some(window) = windows.first() {
        if window.is_visible() {
            window.set_visible(false);
        } else {
            window.present();
        }
        return;
    }
    build(application);
}

fn build(application: &gtk::Application) {
    install_css();

    let config_path = config::config_path();
    let config = config::Config::load(&config_path);
    if !config_path.exists() {
        config.save(&config_path);
    }
    let data = Data::load(&config::data_path());
    let bindings = config.bindings();

    let ui = Rc::new(build_ui(application));
    let app = Rc::new(RefCell::new(App {
        data,
        config,
        bindings,
        selected: None,
        view_date: Local::now().date_naive(),
        focus: Focus::List,
        editing: None,
        status: None,
    }));

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (key_app, key_ui) = (app.clone(), ui.clone());
    keys.connect_key_pressed(move |_, key, _, state| on_key(&key_app, &key_ui, key, state));
    ui.window.add_controller(keys);

    let (entry_app, entry_ui) = (app.clone(), ui.clone());
    ui.entry.connect_activate(move |entry| {
        let text = entry.text().to_string();
        submit(&entry_app, &entry_ui, &text);
    });

    refresh(&app, &ui);
    ui.window.present();
}

fn build_ui(application: &gtk::Application) -> Ui {
    let window = gtk::Window::builder()
        .application(application)
        .title("Next To Do")
        .decorated(false)
        .resizable(false)
        .build();
    window.add_css_class("next-to-do");

    if gtk4_layer_shell::is_supported() {
        // Overlay keeps the panel above every normal window. No anchors are set,
        // so the compositor centres the surface on the current output.
        window.init_layer_shell();
        window.set_namespace(Some("next-to-do"));
        window.set_layer(Layer::Overlay);
        window.set_keyboard_mode(KeyboardMode::Exclusive);
    }

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class("panel");
    panel.set_size_request(420, -1);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("header");
    let heading = gtk::Label::new(Some("TODAY"));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    heading.set_hexpand(true);
    let header_date = gtk::Label::new(None);
    header_date.add_css_class("date");
    header.append(&heading);
    header.append(&header_date);

    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rows.add_css_class("rows");
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(280);
    scroller.set_child(Some(&rows));

    let calendar = gtk::Box::new(gtk::Orientation::Vertical, 2);
    calendar.add_css_class("calendar");
    calendar.set_visible(false);
    let calendar_item = gtk::Label::new(None);
    calendar_item.add_css_class("calendar-item");
    calendar_item.set_max_width_chars(46);
    calendar_item.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let month_label = gtk::Label::new(None);
    month_label.add_css_class("month");
    let weekdays = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    weekdays.set_homogeneous(true);
    for name in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"] {
        let weekday = gtk::Label::new(Some(name));
        weekday.add_css_class("weekday");
        weekdays.append(&weekday);
    }
    let calendar_days = gtk::Box::new(gtk::Orientation::Vertical, 2);
    calendar.append(&calendar_item);
    calendar.append(&month_label);
    calendar.append(&weekdays);
    calendar.append(&calendar_days);

    let input_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    input_bar.add_css_class("input-bar");
    let prompt = gtk::Label::new(Some(">"));
    prompt.add_css_class("prompt");
    let entry = gtk::Entry::new();
    entry.add_css_class("entry");
    entry.set_hexpand(true);
    input_bar.append(&prompt);
    input_bar.append(&entry);
    input_bar.set_visible(false);

    let status = gtk::Label::new(None);
    status.add_css_class("status");
    status.set_xalign(0.0);
    status.set_visible(false);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.add_css_class("footer");
    let footer_count = gtk::Label::new(None);
    footer_count.add_css_class("count");
    footer_count.set_xalign(0.0);
    footer_count.set_hexpand(true);
    let hint = gtk::Label::new(None);
    hint.add_css_class("hint");
    hint.set_xalign(1.0);
    hint.set_wrap(true);
    hint.set_max_width_chars(52);
    hint.set_justify(gtk::Justification::Right);
    footer.append(&footer_count);
    footer.append(&hint);

    panel.append(&header);
    panel.append(&calendar);
    panel.append(&scroller);
    panel.append(&input_bar);
    panel.append(&status);
    panel.append(&footer);
    window.set_child(Some(&panel));

    Ui {
        window,
        rows,
        calendar,
        calendar_item,
        month_label,
        calendar_days,
        header_heading: heading,
        header_date,
        footer_count,
        hint,
        input_bar,
        entry,
        status,
    }
}

const MAIN_HINTS: &[(&str, Action)] = &[
    ("move", Action::MoveDown),
    ("toggle", Action::ToggleDone),
    ("add", Action::Add),
    ("edit", Action::EditTitle),
    ("time", Action::SetDeadline),
    ("delete", Action::Delete),
    ("calendar", Action::CalTime),
    ("quit", Action::Quit),
];

const CALENDAR_HINTS: &[(&str, Action)] = &[
    ("day", Action::CalRight),
    ("week", Action::CalDown),
    ("month", Action::CalNextMonth),
    ("today", Action::CalToday),
    ("list", Action::CalTime),
    ("hide", Action::CalCancel),
];

const TIME_HINTS: &[(&str, Action)] = &[
    ("save", Action::CalConfirm),
    ("cancel", Action::CalCancel),
];

/// Legend built from the configured keys of `hints`.
fn hint_text(config: &config::Config, hints: &[(&str, Action)]) -> String {
    hints
        .iter()
        .map(|(label, action)| {
            let key = config
                .keys
                .get(action.name())
                .and_then(|spec| spec.split(',').next())
                .unwrap_or("?");
            format!("{key} {label}")
        })
        .collect::<Vec<_>>()
        .join("  ·  ")
}

#[allow(deprecated)]
fn install_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// Rebuild the whole list, the calendar and the labels from the current state.
fn refresh(app: &Rc<RefCell<App>>, ui: &Rc<Ui>) {
    let today = Local::now().date_naive();

    let (rows, footer, hint, editing, status, calendar_title, view_date, focus, marked) = {
        let mut app = app.borrow_mut();
        let view_date = app.view_date;
        let order = app.data.ordered(view_date);
        if app.selected.map_or(true, |id| !order.contains(&id)) {
            app.selected = order.first().copied();
        }
        let selected = app.selected;
        let editing = app.editing;
        let focus = app.focus;

        // A plain loop: closures returning references into the borrowed state
        // would not compile.
        let mut rows = Vec::with_capacity(order.len());
        for id in &order {
            if let Some(item) = app.data.get(*id) {
                rows.push(Row {
                    id: item.id,
                    title: item.title.clone(),
                    meta: meta_text(item.deadline.as_ref()),
                    done: item.done,
                    selected: selected == Some(item.id),
                });
            }
        }

        let (open_order, open_time) = app.data.counts(view_date);
        let footer = format!(
            "{} open  ·  {} items",
            open_order + open_time,
            order.len()
        );
        let calendar_title = match editing {
            Some(Editing::Deadline { id, .. }) => app.data.get(id).map(|item| item.title.clone()),
            _ => None,
        };

        // Days that still have unfinished time-bounded items get a marker.
        let marked: HashSet<NaiveDate> = app
            .data
            .items
            .iter()
            .filter(|item| item.kind == Kind::Time && !item.done)
            .filter_map(|item| {
                item.deadline
                    .as_ref()
                    .map(|deadline| deadline.date_naive())
            })
            .collect();

        let hints = match editing {
            Some(Editing::Deadline {
                time_focus: false, ..
            }) => CALENDAR_HINTS,
            Some(_) => TIME_HINTS,
            None if focus == Focus::Calendar => CALENDAR_HINTS,
            None => MAIN_HINTS,
        };
        let hint = hint_text(&app.config, hints);

        (
            rows,
            footer,
            hint,
            editing,
            app.status.clone(),
            calendar_title,
            view_date,
            focus,
            marked,
        )
    };

    while let Some(child) = ui.rows.first_child() {
        ui.rows.remove(&child);
    }
    if rows.is_empty() {
        let empty = gtk::Label::new(Some("nothing on this day"));
        empty.add_css_class("empty");
        empty.set_margin_top(14);
        empty.set_margin_bottom(14);
        ui.rows.append(&empty);
    } else {
        for row in rows {
            ui.rows.append(&make_row(app, ui, row));
        }
    }

    // The calendar is only on screen while it owns the keys, or while a deadline
    // is being edited on it.
    let calendar_open =
        matches!(editing, Some(Editing::Deadline { .. })) || focus == Focus::Calendar;
    ui.calendar.set_visible(calendar_open);
    if calendar_open {
        let calendar_focused = match editing {
            Some(Editing::Deadline {
                time_focus: false, ..
            }) => true,
            Some(_) => false,
            None => focus == Focus::Calendar,
        };
        if calendar_focused {
            ui.calendar.add_css_class("focused");
        } else {
            ui.calendar.remove_css_class("focused");
        }

        ui.calendar_item.set_visible(calendar_title.is_some());
        ui.calendar_item
            .set_text(calendar_title.as_deref().unwrap_or_default());
        ui.month_label.set_text(&view_date.format("%B %Y").to_string());
        build_calendar_days(app, ui, view_date, today, &marked);
    }

    let heading = if view_date == today {
        "TODAY".to_string()
    } else {
        view_date.format("%A").to_string().to_uppercase()
    };
    ui.header_heading.set_text(&heading);
    ui.header_date
        .set_text(&view_date.format("%d %b").to_string().to_uppercase());
    ui.footer_count.set_text(&footer);
    ui.hint.set_text(&hint);
    ui.status.set_text(status.as_deref().unwrap_or_default());
    ui.status.set_visible(status.is_some());
    ui.input_bar.set_visible(editing.is_some());
    match editing {
        Some(editing) => {
            ui.entry.set_placeholder_text(Some(editing.placeholder()));
            if !matches!(
                editing,
                Editing::Deadline {
                    time_focus: false,
                    ..
                }
            ) {
                ui.entry.grab_focus();
            }
        }
        None => ui.entry.set_text(""),
    }
}

/// First day of the month `date` belongs to.
fn month_start(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date)
}

/// Fill the calendar grid with six weeks starting on the Monday of the week of
/// the first day of the month. `marked` holds the days that still have
/// unfinished time-bounded items.
fn build_calendar_days(
    app: &Rc<RefCell<App>>,
    ui: &Rc<Ui>,
    cursor: NaiveDate,
    today: NaiveDate,
    marked: &HashSet<NaiveDate>,
) {
    while let Some(child) = ui.calendar_days.first_child() {
        ui.calendar_days.remove(&child);
    }

    let month = month_start(cursor);
    let offset = month.weekday().num_days_from_monday() as u64;
    let start = month.checked_sub_days(Days::new(offset)).unwrap_or(month);

    for week in 0..6u64 {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        row.set_homogeneous(true);
        for weekday in 0..7u64 {
            let date = start
                .checked_add_days(Days::new(week * 7 + weekday))
                .unwrap_or(start);
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let number = gtk::Label::new(Some(&date.day().to_string()));
            number.add_css_class("day-number");
            let dot = gtk::Label::new(Some(if marked.contains(&date) { "•" } else { "" }));
            dot.add_css_class("dot");
            cell.append(&number);
            cell.append(&dot);

            let button = gtk::Button::new();
            button.set_child(Some(&cell));
            button.add_css_class("day");
            if date.month() != cursor.month() {
                button.add_css_class("other");
            }
            if date == today {
                button.add_css_class("today");
            }
            if date == cursor {
                button.add_css_class("selected");
            }
            let (app, ui) = (app.clone(), ui.clone());
            button.connect_clicked(move |_| {
                app.borrow_mut().view_date = date;
                defer_refresh(&app, &ui);
            });
            row.append(&button);
        }
        ui.calendar_days.append(&row);
    }
}

fn make_row(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, row: Row) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    container.add_css_class("row");
    if row.selected {
        container.add_css_class("selected");
    }
    if row.done {
        container.add_css_class("done");
    }

    let check = gtk::Button::with_label(if row.done { "●" } else { "○" });
    check.add_css_class("check");
    check.set_has_frame(false);
    check.set_valign(gtk::Align::Center);
    {
        let (app, ui, id) = (app.clone(), ui.clone(), row.id);
        check.connect_clicked(move |_| {
            {
                let mut app = app.borrow_mut();
                if let Some(item) = app.data.get_mut(id) {
                    item.done = !item.done;
                }
                app.save();
            }
            defer_refresh(&app, &ui);
        });
    }

    let title = gtk::Label::new(Some(row.title.as_str()));
    title.add_css_class("title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(40);

    let meta = gtk::Label::new(Some(row.meta.as_str()));
    meta.add_css_class("meta");

    container.append(&check);
    container.append(&title);
    container.append(&meta);

    let click = gtk::GestureClick::new();
    {
        let (app, ui, id) = (app.clone(), ui.clone(), row.id);
        click.connect_pressed(move |_, _, _, _| {
            app.borrow_mut().selected = Some(id);
            defer_refresh(&app, &ui);
        });
    }
    container.add_controller(click);
    container
}

/// Rebuild the list once the current event finished; removing the widget that
/// emitted the signal from inside its own handler is best avoided.
fn defer_refresh(app: &Rc<RefCell<App>>, ui: &Rc<Ui>) {
    let (app, ui) = (app.clone(), ui.clone());
    gtk::glib::idle_add_local_once(move || refresh(&app, &ui));
}

fn meta_text(deadline: Option<&DateTime<Local>>) -> String {
    deadline
        .map(|deadline| deadline.format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Parse the `HH:MM` time typed into the deadline entry.
fn parse_time(text: &str) -> Option<(u32, u32)> {
    let (hour, minute) = text.trim().split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;
    (hour < 24 && minute < 60).then_some((hour, minute))
}

/// First binding of the requested mode (normal or calendar) that matches `key`.
fn find_action(
    app: &Rc<RefCell<App>>,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
    calendar: bool,
) -> Option<Action> {
    let app = app.borrow();
    app.bindings
        .iter()
        .find(|binding| binding.action.is_calendar() == calendar && binding.matches(key, state))
        .map(|binding| binding.action)
}

fn on_key(
    app: &Rc<RefCell<App>>,
    ui: &Rc<Ui>,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> gtk::glib::Propagation {
    let editing = app.borrow().editing;

    // While a deadline is being edited the calendar keeps the keyboard, so the
    // day can be adjusted before the time is confirmed.
    if matches!(editing, Some(Editing::Deadline { time_focus: false, .. })) {
        match find_action(app, key, state, true) {
            Some(Action::CalCancel) => cancel_edit(app, ui),
            Some(Action::CalConfirm) => {
                let text = ui.entry.text().to_string();
                submit(app, ui, &text);
            }
            Some(Action::CalTime) => focus_entry(app, ui, true),
            Some(action) => calendar_action(app, ui, action),
            None if key == gtk::gdk::Key::Escape => cancel_edit(app, ui),
            None => {}
        }
        return gtk::glib::Propagation::Stop;
    }

    // Text entry: Escape cancels, the calendar key hands the keyboard back.
    if editing.is_some() {
        if key == gtk::gdk::Key::Escape {
            cancel_edit(app, ui);
            return gtk::glib::Propagation::Stop;
        }
        if matches!(editing, Some(Editing::Deadline { time_focus: true, .. }))
            && find_action(app, key, state, true) == Some(Action::CalTime)
        {
            focus_entry(app, ui, false);
            return gtk::glib::Propagation::Stop;
        }
        return gtk::glib::Propagation::Proceed;
    }

    // Nothing is being edited: the focused pane decides what a key means.
    if app.borrow().focus == Focus::Calendar {
        match find_action(app, key, state, true) {
            Some(Action::CalConfirm | Action::CalCancel | Action::CalTime) => {
                set_focus(app, ui, Focus::List)
            }
            Some(action) => calendar_action(app, ui, action),
            // Quitting and hiding stay available from every pane.
            None => match find_action(app, key, state, false) {
                Some(action @ (Action::Quit | Action::Hide)) => run_action(app, ui, action),
                _ if key == gtk::gdk::Key::Escape => set_focus(app, ui, Focus::List),
                _ => {}
            },
        }
        return gtk::glib::Propagation::Stop;
    }

    match find_action(app, key, state, false) {
        Some(action) => {
            run_action(app, ui, action);
            gtk::glib::Propagation::Stop
        }
        None => {
            if find_action(app, key, state, true) == Some(Action::CalTime) {
                set_focus(app, ui, Focus::Calendar);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        }
    }
}

fn set_focus(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, focus: Focus) {
    app.borrow_mut().focus = focus;
    refresh(app, ui);
}

/// Give the `HH:MM` entry the keyboard, or hand it back to the calendar.
fn focus_entry(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, entry: bool) {
    if let Some(Editing::Deadline { time_focus, .. }) = app.borrow_mut().editing.as_mut() {
        *time_focus = entry;
    }
    refresh(app, ui);
}

/// Move the viewed day by one of the `cal_*` movement actions.
fn calendar_action(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, action: Action) {
    let today = Local::now().date_naive();
    {
        let mut app = app.borrow_mut();
        let date = app.view_date;
        let moved = match action {
            Action::CalLeft => date.checked_sub_days(Days::new(1)),
            Action::CalRight => date.checked_add_days(Days::new(1)),
            Action::CalUp => date.checked_sub_days(Days::new(7)),
            Action::CalDown => date.checked_add_days(Days::new(7)),
            Action::CalPrevMonth => shift_month(date, -1),
            Action::CalNextMonth => shift_month(date, 1),
            Action::CalToday => Some(today),
            _ => None,
        };
        if let Some(moved) = moved {
            app.view_date = moved;
        }
    }
    refresh(app, ui);
}

/// The same day in the neighbouring month, falling back to its first day.
fn shift_month(date: NaiveDate, delta: i32) -> Option<NaiveDate> {
    let index = date.year() * 12 + date.month0() as i32 + delta;
    let year = index.div_euclid(12);
    let month = index.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(year, month, date.day())
        .or_else(|| NaiveDate::from_ymd_opt(year, month, 1))
}

fn run_action(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, action: Action) {
    match action {
        Action::Quit => {
            app.borrow().save();
            if let Some(application) = ui.window.application() {
                application.quit();
            }
        }
        Action::Hide => {
            app.borrow_mut().editing = None;
            ui.window.set_visible(false);
        }
        Action::MoveUp => move_selection(app, ui, -1),
        Action::MoveDown => move_selection(app, ui, 1),
        Action::ToggleDone => toggle_selected(app, ui),
        Action::Add => begin_edit(app, ui, Editing::NewItem, ""),
        Action::EditTitle => {
            let selected = app.borrow().selected;
            if let Some(id) = selected {
                let title = app
                    .borrow()
                    .data
                    .get(id)
                    .map(|item| item.title.clone())
                    .unwrap_or_default();
                begin_edit(app, ui, Editing::Title(id), &title);
            }
        }
        Action::Delete => delete_selected(app, ui),
        Action::SetDeadline => {
            let selected = app.borrow().selected;
            if let Some(id) = selected {
                let (hour, minute) = {
                    let app = app.borrow();
                    match app.data.get(id).and_then(|item| item.deadline.as_ref()) {
                        Some(deadline) => (deadline.hour(), deadline.minute()),
                        None => (23, 59),
                    }
                };
                begin_edit(
                    app,
                    ui,
                    Editing::Deadline {
                        id,
                        time_focus: false,
                    },
                    &format!("{hour:02}:{minute:02}"),
                );
            }
        }
        Action::MakeOrder => {
            let selected = app.borrow().selected;
            if let Some(id) = selected {
                {
                    let mut app = app.borrow_mut();
                    app.data.make_order(id);
                    app.save();
                }
                refresh(app, ui);
            }
        }
        Action::MoveItemUp => reorder_selected(app, ui, true),
        Action::MoveItemDown => reorder_selected(app, ui, false),
        Action::Reload => {
            {
                let mut app = app.borrow_mut();
                app.data = Data::load(&config::data_path());
                app.status = None;
            }
            refresh(app, ui);
        }
        // The `cal_*` actions are routed by `on_key` while the calendar is open
        // and never reach this function.
        _ => {}
    }
}

fn move_selection(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, step: isize) {
    let today = Local::now().date_naive();
    let (order, current) = {
        let app = app.borrow();
        (app.data.ordered(today), app.selected)
    };
    if order.is_empty() {
        return;
    }
    let index = current.and_then(|id| order.iter().position(|&item| item == id));
    let next = match (index, step) {
        (Some(index), step) if step < 0 => index.saturating_sub(1),
        (Some(index), _) => (index + 1).min(order.len() - 1),
        (None, _) => 0,
    };
    app.borrow_mut().selected = Some(order[next]);
    refresh(app, ui);
}

fn toggle_selected(app: &Rc<RefCell<App>>, ui: &Rc<Ui>) {
    let selected = app.borrow().selected;
    let Some(id) = selected else {
        return;
    };
    {
        let mut app = app.borrow_mut();
        if let Some(item) = app.data.get_mut(id) {
            item.done = !item.done;
        }
        app.save();
    }
    refresh(app, ui);
}

fn delete_selected(app: &Rc<RefCell<App>>, ui: &Rc<Ui>) {
    let selected = app.borrow().selected;
    let Some(id) = selected else {
        return;
    };
    {
        let mut app = app.borrow_mut();
        app.data.remove(id);
        app.selected = None;
        app.save();
    }
    refresh(app, ui);
}

fn reorder_selected(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, up: bool) {
    let selected = app.borrow().selected;
    let Some(id) = selected else {
        return;
    };
    {
        let mut app = app.borrow_mut();
        if up {
            app.data.move_up(id);
        } else {
            app.data.move_down(id);
        }
        app.save();
    }
    refresh(app, ui);
}

fn begin_edit(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, editing: Editing, prefill: &str) {
    {
        let mut app = app.borrow_mut();
        app.editing = Some(editing);
        app.status = None;
    }
    ui.entry.set_text(prefill);
    refresh(app, ui);
    // The calendar keeps the keyboard unless the time field is asked for.
    if !matches!(editing, Editing::Deadline { time_focus: false, .. }) {
        ui.entry.grab_focus();
        ui.entry.select_region(0, -1);
    }
}

fn cancel_edit(app: &Rc<RefCell<App>>, ui: &Rc<Ui>) {
    {
        let mut app = app.borrow_mut();
        app.editing = None;
        app.status = None;
    }
    refresh(app, ui);
}

fn submit(app: &Rc<RefCell<App>>, ui: &Rc<Ui>, text: &str) {
    let editing = app.borrow_mut().editing.take();
    let Some(editing) = editing else {
        return;
    };
    let view_date = app.borrow().view_date;

    let mut selected = None;
    let mut error = None;
    match editing {
        Editing::NewItem => {
            let title = text.trim();
            if !title.is_empty() {
                let id = app.borrow_mut().data.add(title, Kind::Order);
                selected = Some(id);
            }
        }
        Editing::Title(id) => {
            let title = text.trim();
            if !title.is_empty() {
                if let Some(item) = app.borrow_mut().data.get_mut(id) {
                    item.title = title.to_string();
                }
            }
            selected = Some(id);
        }
        Editing::Deadline { id, .. } => {
            let deadline = parse_time(text)
                .and_then(|(hour, minute)| view_date.and_hms_opt(hour, minute, 0))
                .and_then(|naive| Local.from_local_datetime(&naive).single());
            match deadline {
                Some(deadline) => {
                    app.borrow_mut().data.set_deadline(id, deadline);
                    selected = Some(id);
                }
                None => {
                    // Keep the editor open so the time can be corrected.
                    let mut restored = editing;
                    if let Editing::Deadline { time_focus, .. } = &mut restored {
                        *time_focus = true;
                    }
                    app.borrow_mut().editing = Some(restored);
                    error = Some("expected a time like `09:30`".to_string());
                }
            }
        }
    }

    {
        let mut app = app.borrow_mut();
        if let Some(id) = selected {
            app.selected = Some(id);
        }
        app.status = error;
        app.save();
    }
    refresh(app, ui);
}
