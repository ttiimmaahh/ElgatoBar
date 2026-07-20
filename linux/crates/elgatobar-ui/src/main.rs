use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::mpsc::Sender};

use adw::prelude::*;
use elgatobar_ui::{
    APPLICATION_ID,
    client::{ClientEvent, ClientHandle},
    model::{Command, ConnectionState, Controller, Intent, InventoryState, native_to_kelvin},
};
use gtk::{glib, prelude::*};
use libadwaita as adw;

struct Ui {
    window: adw::ApplicationWindow,
    overlay: adw::ToastOverlay,
    content: gtk::Box,
    controller: RefCell<Controller>,
    commands: Sender<Command>,
    client: RefCell<Option<ClientHandle>>,
    add_window: RefCell<Option<gtk::Window>>,
    slider_commits: RefCell<BTreeMap<String, glib::SourceId>>,
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APPLICATION_ID)
        .build();
    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        build_ui(app);
    });
    app.run()
}

fn build_ui(app: &adw::Application) {
    let client = elgatobar_ui::client::spawn();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("ElgatoBar")
        .default_width(380)
        .default_height(420)
        .width_request(320)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&content));
    window.set_content(Some(&overlay));
    let ui = Rc::new(Ui {
        window,
        overlay,
        content,
        controller: RefCell::new(Controller::loading()),
        commands: client.commands.clone(),
        client: RefCell::new(Some(client)),
        add_window: RefCell::new(None),
        slider_commits: RefCell::new(BTreeMap::new()),
    });
    render(&ui);
    let weak = Rc::downgrade(&ui);
    glib::timeout_add_local(std::time::Duration::from_millis(40), move || {
        let Some(ui) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let mut changed = false;
        loop {
            let event = {
                ui.client
                    .borrow()
                    .as_ref()
                    .and_then(|c| c.events.try_recv().ok())
            };
            let Some(event) = event else { break };
            handle_event(&ui, event);
            changed = true;
        }
        if changed {
            render(&ui);
        }
        glib::ControlFlow::Continue
    });
    ui.window.present();
}

fn send(ui: &Rc<Ui>, intent: Intent) {
    if let Some(command) = ui.controller.borrow_mut().intent(intent) {
        let _ = ui.commands.send(command);
    }
    render(ui);
}

fn queue_slider(ui: &Rc<Ui>, key: String, intent: Intent) {
    if let Some(source) = ui.slider_commits.borrow_mut().remove(&key) {
        source.remove();
    }
    let pending_ui = ui.clone();
    let pending_key = key.clone();
    let source = glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
        pending_ui.slider_commits.borrow_mut().remove(&pending_key);
        send(&pending_ui, intent);
    });
    ui.slider_commits.borrow_mut().insert(key, source);
}

fn handle_event(ui: &Rc<Ui>, event: ClientEvent) {
    match event {
        ClientEvent::Connected(devices) | ClientEvent::Replaced(devices) => {
            let mut c = ui.controller.borrow_mut();
            c.connection(ConnectionState::Available);
            c.replace(devices);
        }
        ClientEvent::Unavailable(error) => ui
            .controller
            .borrow_mut()
            .connection(ConnectionState::Unavailable(error)),
        ClientEvent::Completed {
            id,
            generation,
            results,
        } => {
            let feedback = elgatobar_ui::model::operation_feedback(&results);
            ui.overlay.add_toast(adw::Toast::new(&format!(
                "{} — {}",
                feedback.title, feedback.detail
            )));
            if let Some(ref id) = id
                && let Some(next) = ui.controller.borrow_mut().complete(id, generation)
            {
                let _ = ui.commands.send(next);
            } else if id.is_none() {
                ui.controller.borrow_mut().complete_aggregate(generation);
            }
        }
        ClientEvent::Added { generation } => {
            ui.controller
                .borrow_mut()
                .complete_configuration(generation);
            if let Some(window) = ui.add_window.borrow_mut().take() {
                window.close();
            }
            ui.overlay.add_toast(adw::Toast::new("Light added"))
        }
        ClientEvent::Removed { id, generation } => {
            ui.controller.borrow_mut().complete(&id, generation);
            ui.overlay
                .add_toast(adw::Toast::new("Local configuration removed"));
        }
        ClientEvent::Failed {
            id,
            generation,
            message,
        } => {
            if let Some(ref id) = id
                && let Some(next) = ui.controller.borrow_mut().complete(id, generation)
            {
                let _ = ui.commands.send(next);
            } else if id.is_none() {
                ui.controller.borrow_mut().complete_aggregate(generation);
                ui.controller
                    .borrow_mut()
                    .complete_configuration(generation);
            }
            let feedback = elgatobar_ui::model::error_feedback(&message);
            ui.overlay.add_toast(adw::Toast::new(&format!(
                "{} — {}",
                feedback.title, feedback.detail
            )));
        }
    }
}

fn render(ui: &Rc<Ui>) {
    while let Some(child) = ui.content.first_child() {
        ui.content.remove(&child);
    }
    let model = ui.controller.borrow().model();
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("ElgatoBar", &model.summary);
    header.set_title_widget(Some(&title));
    let add = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Add Light")
        .build();
    add.set_sensitive(
        !ui.controller.borrow().configuration_pending()
            && !matches!(
                model.state,
                InventoryState::Loading | InventoryState::Unavailable
            ),
    );
    {
        let ui = ui.clone();
        add.connect_clicked(move |_| add_dialog(&ui));
    }
    header.pack_start(&add);
    let refresh = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh All")
        .build();
    refresh.set_sensitive(
        !ui.controller.borrow().aggregate_pending()
            && !matches!(
                model.state,
                InventoryState::Loading
                    | InventoryState::Unavailable
                    | InventoryState::Unconfigured
            ),
    );
    {
        let ui = ui.clone();
        refresh.connect_clicked(move |_| send(&ui, Intent::RefreshAll));
    }
    header.pack_end(&refresh);
    ui.content.append(&header);
    if matches!(
        model.state,
        InventoryState::Loading | InventoryState::Unavailable | InventoryState::Unconfigured
    ) {
        let status = adw::StatusPage::builder()
            .title(match model.state {
                InventoryState::Loading => "Connecting…",
                InventoryState::Unavailable => "Daemon Unavailable",
                _ => "No Lights Configured",
            })
            .description(&model.summary)
            .icon_name("dialog-information-symbolic")
            .build();
        if matches!(model.state, InventoryState::Unavailable) {
            let retry = gtk::Button::with_label("Retry");
            retry.add_css_class("suggested-action");
            {
                let ui = ui.clone();
                retry.connect_clicked(move |_| send(&ui, Intent::Retry));
            }
            status.set_child(Some(&retry));
        } else if matches!(model.state, InventoryState::Unconfigured) {
            let button = gtk::Button::with_label("Add Light");
            {
                let ui = ui.clone();
                button.connect_clicked(move |_| add_dialog(&ui));
            }
            status.set_child(Some(&button));
        }
        ui.content.append(&status);
        return;
    }
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    toolbar.set_margin_top(8);
    toolbar.set_margin_bottom(8);
    toolbar.set_margin_start(12);
    toolbar.set_margin_end(12);
    let summary = gtk::Label::new(Some(&model.summary));
    summary.set_hexpand(true);
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    toolbar.append(&summary);
    let toggle_all = gtk::Button::with_label("Toggle All");
    toggle_all.set_sensitive(
        !ui.controller.borrow().aggregate_pending()
            && !model.stale
            && model.rows.iter().any(|r| r.mutations_enabled),
    );
    {
        let ui = ui.clone();
        toggle_all.connect_clicked(move |_| send(&ui, Intent::ToggleAll));
    }
    toolbar.append(&toggle_all);
    ui.content.append(&toolbar);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    list.set_margin_start(12);
    list.set_margin_end(12);
    list.set_margin_bottom(12);
    for row in model.rows {
        list.append(&device_card(ui, row));
    }
    scroll.set_child(Some(&list));
    ui.content.append(&scroll);
}

fn device_card(ui: &Rc<Ui>, row: elgatobar_ui::model::DeviceRow) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("card");
    card.set_margin_top(4);
    card.set_margin_bottom(4);
    card.set_margin_start(4);
    card.set_margin_end(4);
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    let name = gtk::Label::new(Some(&row.name));
    name.add_css_class("heading");
    name.set_xalign(0.0);
    let meta = gtk::Label::new(Some(&format!("{} · {}", row.endpoint, row.status)));
    meta.add_css_class("dim-label");
    meta.set_xalign(0.0);
    meta.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    meta.set_tooltip_text(Some(&row.detail));
    labels.append(&name);
    labels.append(&meta);
    heading.append(&labels);
    let power = gtk::Switch::builder()
        .active(row.state.as_ref().is_some_and(|s| s.power == "On"))
        .sensitive(row.mutations_enabled && !row.pending)
        .tooltip_text("Toggle this light")
        .build();
    {
        let ui = ui.clone();
        let id = row.id.clone();
        power.connect_state_set(move |_, _| {
            send(&ui, Intent::Toggle(id.clone()));
            glib::Propagation::Stop
        });
    }
    heading.append(&power);
    card.append(&heading);
    if let Some(state) = row.state {
        let b = gtk::Scale::with_range(gtk::Orientation::Horizontal, 3.0, 100.0, 1.0);
        b.set_value(
            state
                .brightness
                .trim_end_matches('%')
                .parse::<f64>()
                .unwrap_or(3.0),
        );
        b.set_sensitive(row.mutations_enabled);
        b.set_tooltip_text(Some("Brightness, 3 to 100 percent"));
        {
            let ui = ui.clone();
            let id = row.id.clone();
            b.connect_change_value(move |_, _, value| {
                queue_slider(
                    &ui,
                    format!("brightness:{id}"),
                    Intent::Brightness(id.clone(), value.round() as u8),
                );
                glib::Propagation::Proceed
            });
        }
        card.append(&labeled("Brightness", &state.brightness, &b));
        let native = ui
            .controller
            .borrow()
            .snapshot(&row.id)
            .map_or(143, |s| s.temperature);
        let t = gtk::Scale::with_range(gtk::Orientation::Horizontal, 143.0, 344.0, 1.0);
        t.set_value(f64::from(native));
        t.set_inverted(true);
        t.set_sensitive(row.mutations_enabled);
        t.set_tooltip_text(Some(&format!(
            "{}; warmer to cooler",
            state.native_temperature
        )));
        {
            let ui = ui.clone();
            let id = row.id.clone();
            t.connect_change_value(move |_, _, value| {
                queue_slider(
                    &ui,
                    format!("temperature:{id}"),
                    Intent::Temperature(id.clone(), value.round() as u16),
                );
                glib::Propagation::Proceed
            });
        }
        card.append(&labeled(
            "Temperature",
            &format!("{} K", native_to_kelvin(native)),
            &t,
        ));
    } else {
        let unknown = gtk::Label::new(Some("No state received yet"));
        unknown.add_css_class("dim-label");
        unknown.set_xalign(0.0);
        card.append(&unknown);
    }
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let identify = gtk::Button::with_label("Identify");
    identify.set_sensitive(row.mutations_enabled && !row.pending);
    {
        let ui = ui.clone();
        let id = row.id.clone();
        identify.connect_clicked(move |_| send(&ui, Intent::Identify(id.clone())));
    }
    actions.append(&identify);
    let remove = gtk::Button::with_label("Remove…");
    remove.set_sensitive(!ui.controller.borrow().model().stale && !row.pending);
    remove.add_css_class("destructive-action");
    {
        let ui = ui.clone();
        let id = row.id.clone();
        remove.connect_clicked(move |_| remove_dialog(&ui, &id));
    }
    actions.append(&remove);
    card.append(&actions);
    card.upcast()
}

fn labeled(label: &str, value: &str, scale: &gtk::Scale) -> gtk::Widget {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let l = gtk::Label::new(Some(label));
    l.set_hexpand(true);
    l.set_xalign(0.0);
    top.append(&l);
    top.append(&gtk::Label::new(Some(value)));
    box_.append(&top);
    box_.append(scale);
    box_.upcast()
}

fn add_dialog(ui: &Rc<Ui>) {
    if let Some(window) = ui.add_window.borrow().as_ref() {
        window.present();
        return;
    }
    let window = gtk::Window::builder()
        .title("Add Light")
        .transient_for(&ui.window)
        .modal(true)
        .default_width(360)
        .resizable(false)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    let description = gtk::Label::new(Some(
        "Enter a hostname, IP address, or endpoint. The daemon validates the light before saving it.",
    ));
    description.set_wrap(true);
    description.set_xalign(0.0);
    content.append(&description);
    let entry = gtk::Entry::builder()
        .placeholder_text("key-light.local")
        .activates_default(true)
        .build();
    content.append(&entry);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    let add = gtk::Button::with_label("Add");
    add.add_css_class("suggested-action");
    actions.append(&cancel);
    actions.append(&add);
    content.append(&actions);
    window.set_child(Some(&content));
    window.set_default_widget(Some(&add));
    {
        let window = window.clone();
        cancel.connect_clicked(move |_| window.hide());
    }
    {
        let ui = ui.clone();
        let entry = entry.clone();
        add.connect_clicked(move |_| {
            let text = entry.text().to_string();
            if !text.trim().is_empty() {
                send(&ui, Intent::Add(text));
            }
        });
    }
    {
        let ui = ui.clone();
        entry.connect_activate(move |entry| {
            let text = entry.text().to_string();
            if !text.trim().is_empty() {
                send(&ui, Intent::Add(text));
            }
        });
    }
    *ui.add_window.borrow_mut() = Some(window.clone());
    window.present();
}

fn remove_dialog(ui: &Rc<Ui>, id: &str) {
    let dialog = adw::AlertDialog::new(
        Some("Remove this light?"),
        Some("Only local configuration will be deleted. The physical light will not be changed."),
    );
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("remove", "Remove");
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    dialog.set_close_response("cancel");
    dialog.present(Some(&ui.window));
    let ui = ui.clone();
    let id = id.to_string();
    dialog.connect_response(None, move |_, response| {
        if response == "remove" {
            send(&ui, Intent::Remove(id.clone()));
        }
    });
}
