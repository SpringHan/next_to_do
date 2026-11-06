//! The layer-shell window: theme, widget tree, keyboard and mouse handling.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
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

.row.overdue .meta {
    color: #e06c75;
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
"#;

/// What the entry line is currently editing.
#[derive(Debug, Clone, Copy)]
enum Editing {
    NewItem,
    Title(u64),
    Deadline(u64),
}

impl Editing {
    fn placeholder(self) -> &'static str {
        match self {
            Editing::NewItem => "new item, Enter to save, Escape to cancel",
            Editing::Title(_) => "new title, Enter to save, Escape to cancel",
            Editing::Deadline(_) => "deadline `YYYY-MM-DD HH:MM`, Enter to save",
        }
    }
}

struct App {
    data: Data,
    bindings: Vec<Binding>,
    selected: Option<u64>,
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
    header_date: gtk::Label,
    footer_count: gtk::Label,
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
    overdue: bool,
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

    let ui = Rc::new(build_ui(application, &config));
    let app = Rc::new(RefCell::new(App {
        data,
        bindings,
        selected: None,
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

fn build_ui(application: &gtk::Application, config: &config::Config) -> Ui {
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
    scroller.set_max_content_height(460);
    scroller.set_child(Some(&rows));

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
    let hint = gtk::Label::new(Some(hint_text(config).as_str()));
    hint.add_css_class("hint");
    hint.set_xalign(1.0);
    footer.append(&footer_count);
    footer.append(&hint);

    panel.append(&header);
    panel.append(&scroller);
    panel.append(&input_bar);
    panel.append(&status);
    panel.append(&footer);
    window.set_child(Some(&panel));

    Ui {
        window,
        rows,
        header_date,
        footer_count,
        input_bar,
        entry,
        status,
    }
}

/// Footer legend built from the configured keys.
fn hint_text(config: &config::Config) -> String {
    const HINTS: &[(&str, Action)] = &[
        ("move", Action::MoveDown),
        ("toggle", Action::ToggleDone),
        ("add", Action::Add),
        ("edit", Action::EditTitle),
        ("time", Action::SetDeadline),
        ("delete", Action::Delete),
        ("quit", Action::Quit),
    ];
    HINTS
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

/// Rebuild the whole list and the labels from the current state.
fn refresh(app: &Rc<RefCell<App>>, ui: &Rc<Ui>) {
    let today = Local::now().date_naive();

    let (rows, footer, date, editing, status) = {
        let mut app = app.borrow_mut();
        let order = app.data.ordered(today);
        if app.selected.map_or(true, |id| !order.contains(&id)) {
            app.selected = order.first().copied();
        }
        let selected = app.selected;

        // A plain loop: closures returning references into the borrowed state
        // would not compile.
        let mut rows = Vec::with_capacity(order.len());
        for id in &order {
            if let Some(item) = app.data.get(*id) {
                rows.push(Row {
                    id: item.id,
                    title: item.title.clone(),
                    meta: meta_text(item.deadline.as_ref(), today),
                    done: item.done,
                    overdue: item.is_overdue(today),
                    selected: selected == Some(item.id),
                });
            }
        }

        let (open_order, open_time) = app.data.counts(today);
        let footer = format!(
            "{} open  ·  {} today",
            open_order + open_time,
            order.len()
        );
        let date = Local::now().format("%a %d %b").to_string().to_uppercase();
        (rows, footer, date, app.editing, app.status.clone())
    };

    while let Some(child) = ui.rows.first_child() {
        ui.rows.remove(&child);
    }
    if rows.is_empty() {
        let empty = gtk::Label::new(Some("nothing left for today"));
        empty.add_css_class("empty");
        empty.set_margin_top(14);
        empty.set_margin_bottom(14);
        ui.rows.append(&empty);
    } else {
        for row in rows {
            ui.rows.append(&make_row(app, ui, row));
        }
    }

    ui.header_date.set_text(&date);
    ui.footer_count.set_text(&footer);
    ui.status.set_text(status.as_deref().unwrap_or_default());
    ui.status.set_visible(status.is_some());
    ui.input_bar.set_visible(editing.is_some());
    match editing {
        Some(editing) => {
            ui.entry.set_placeholder_text(Some(editing.placeholder()));
            ui.entry.grab_focus();
        }
        None => ui.entry.set_text(""),
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
    if row.overdue {
        container.add_css_class("overdue");
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

fn meta_text(deadline: Option<&DateTime<Local>>, today: NaiveDate) -> String {
    match deadline {
        None => String::new(),
        Some(deadline) if deadline.date_naive() == today => deadline.format("%H:%M").to_string(),
        Some(deadline) => format!(
            "{} {}",
            deadline.format("%d %b"),
            deadline.format("%H:%M")
        ),
    }
}

fn parse_deadline(text: &str) -> Option<DateTime<Local>> {
    let text = text.trim();
    let naive = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| {
            NaiveDate::parse_from_str(text, "%Y-%m-%d")
                .map(|date| date.and_hms_opt(23, 59, 0).expect("23:59 is a valid time"))
        })
        .ok()?;
    Local.from_local_datetime(&naive).single()
}

fn on_key(
    app: &Rc<RefCell<App>>,
    ui: &Rc<Ui>,
    key: gtk::gdk::Key,
    state: gtk::gdk::ModifierType,
) -> gtk::glib::Propagation {
    // While the entry is open every key except Escape belongs to it.
    if app.borrow().editing.is_some() {
        if key == gtk::gdk::Key::Escape {
            cancel_edit(app, ui);
            return gtk::glib::Propagation::Stop;
        }
        return gtk::glib::Propagation::Proceed;
    }

    let action = {
        let app = app.borrow();
        app.bindings
            .iter()
            .find(|binding| binding.matches(key, state))
            .map(|binding| binding.action)
    };
    match action {
        Some(action) => {
            run_action(app, ui, action);
            gtk::glib::Propagation::Stop
        }
        None => gtk::glib::Propagation::Proceed,
    }
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
                let prefill = app
                    .borrow()
                    .data
                    .get(id)
                    .and_then(|item| item.deadline.as_ref())
                    .map(|deadline| deadline.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| {
                        format!("{} 23:59", Local::now().format("%Y-%m-%d"))
                    });
                begin_edit(app, ui, Editing::Deadline(id), &prefill);
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
    ui.entry.grab_focus();
    ui.entry.select_region(0, -1);
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
        Editing::Deadline(id) => match parse_deadline(text) {
            Some(deadline) => {
                app.borrow_mut().data.set_deadline(id, deadline);
                selected = Some(id);
            }
            None => {
                // Keep the line open so the text can be corrected.
                app.borrow_mut().editing = Some(Editing::Deadline(id));
                error = Some("expected `YYYY-MM-DD HH:MM`".to_string());
            }
        },
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
