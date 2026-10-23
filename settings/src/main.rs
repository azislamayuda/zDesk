//! On-demand preferences window. The desktop widget never links GTK or Adwaita.

mod changelog;
mod store;

use adw::gtk;
use adw::prelude::*;
use gtk::{Orientation, PolicyType};
use libadwaita as adw;
use std::cell::RefCell;
use std::ffi::OsStr;
use std::rc::Rc;
use store::Store;

enum Editor {
    Float(adw::SpinRow),
    Integer(adw::SpinRow),
    Text(adw::EntryRow),
}

#[derive(PartialEq)]
enum FieldValue {
    Float(f64),
    Integer(i64),
    Text(String),
}

struct Field {
    section: &'static str,
    key: &'static str,
    editor: Editor,
    initial: FieldValue,
}

impl Field {
    fn current_value(&self) -> FieldValue {
        match &self.editor {
            Editor::Float(row) => FieldValue::Float(row.value()),
            Editor::Integer(row) => FieldValue::Integer(row.value().round() as i64),
            Editor::Text(row) => FieldValue::Text(row.text().to_string()),
        }
    }
}

fn main() {
    // The on-demand editor needs no GPU renderer; Cairo uses less memory here.
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "cairo");
    }
    let application = adw::Application::builder()
        .application_id("id.lamayuda.zDesk.Settings")
        .build();
    let about_action = adw::gio::SimpleAction::new("about", None);
    let about_app = application.clone();
    about_action.connect_activate(move |_, _| {
        if let Some(window) = about_app.active_window() {
            let dialog = adw::AboutDialog::builder()
                .application_name("zDesk")
                .application_icon("preferences-system")
                .developer_name("Azis Lamayuda")
                .version(env!("CARGO_PKG_VERSION"))
                .comments("Created by Azis Lamayuda")
                .build();
            dialog.present(Some(&window));
        }
    });
    application.add_action(&about_action);
    let changelog_action = adw::gio::SimpleAction::new("changelog", None);
    let changelog_app = application.clone();
    changelog_action.connect_activate(move |_, _| {
        if let Some(window) = changelog_app.active_window() {
            changelog::show(&window);
        }
    });
    application.add_action(&changelog_action);
    application.connect_activate(build_window);
    application.run();
}

fn build_window(app: &adw::Application) {
    let store = match Store::open() {
        Ok(store) => store,
        Err(error) => {
            show_open_error(app, &error);
            return;
        }
    };

    let controls = match build_fields(&store) {
        Ok(controls) => controls,
        Err(error) => {
            show_open_error(app, &error);
            return;
        }
    };
    let (groups, fields) = controls;

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new("zDesk", "Display Settings")));
    let menu = adw::gio::Menu::new();
    menu.append(Some("About zDesk"), Some("app.about"));
    menu.append(Some("Changelog"), Some("app.changelog"));
    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_tooltip_text(Some("Application menu"));
    menu_button.set_menu_model(Some(&menu));
    header.pack_end(&menu_button);

    let save_button = gtk::Button::with_label("Save & Apply");
    save_button.add_css_class("suggested-action");

    let column = gtk::Box::new(Orientation::Vertical, 24);
    column.set_margin_top(24);
    column.set_margin_bottom(24);
    column.set_margin_start(24);
    column.set_margin_end(24);
    for group in groups {
        column.append(&group);
    }

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(PolicyType::Never, PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&column));

    let status = gtk::Label::new(Some(
        "Settings apply to every monitor. Save to apply your changes.",
    ));
    status.set_wrap(true);
    status.set_xalign(0.0);
    status.set_hexpand(true);
    status.add_css_class("dim-label");
    let footer = gtk::Box::new(Orientation::Horizontal, 16);
    footer.set_margin_start(24);
    footer.set_margin_end(24);
    footer.set_margin_top(12);
    footer.set_margin_bottom(18);
    footer.append(&status);
    footer.append(&save_button);

    let shell = gtk::Box::new(Orientation::Vertical, 0);
    shell.append(&header);
    shell.append(&scroll);
    shell.append(&footer);
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&shell));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(640)
        .default_height(740)
        .content(&overlay)
        .build();

    let store = Rc::new(RefCell::new(store));
    let fields = Rc::new(RefCell::new(fields));
    save_button.connect_clicked({
        let save_button = save_button.clone();
        let status = status.clone();
        let overlay = overlay.clone();
        move |_| {
            save_button.set_sensitive(false);
            let result = (|| -> Result<bool, String> {
                let mut store = store.borrow_mut();
                store.reset_draft()?;
                let mut changed = false;
                for field in fields.borrow().iter() {
                    let value = field.current_value();
                    if value == field.initial {
                        continue;
                    }
                    changed = true;
                    match value {
                        FieldValue::Float(value) => {
                            store.set_f64(field.section, field.key, value)?
                        }
                        FieldValue::Integer(value) => {
                            store.set_i64(field.section, field.key, value)?
                        }
                        FieldValue::Text(value) => {
                            store.set_text(field.section, field.key, &value)?
                        }
                    }
                }
                if changed {
                    store.save()?;
                }
                Ok(changed)
            })();

            match result {
                Ok(true) => {
                    for field in fields.borrow_mut().iter_mut() {
                        field.initial = field.current_value();
                    }
                    status.set_text("Configuration saved. Checking whether zDesk is running…");
                    apply_when_running(save_button.clone(), status.clone(), overlay.clone());
                }
                Ok(false) => {
                    status.set_text("No new edits. Checking whether zDesk needs to reload…");
                    apply_when_running(save_button.clone(), status.clone(), overlay.clone());
                }
                Err(error) => {
                    status.set_text(&format!("Not saved: {error}"));
                    overlay.add_toast(adw::Toast::new("Check the setting values"));
                    save_button.set_sensitive(true);
                }
            }
        }
    });

    window.present();
}

fn build_fields(store: &Store) -> Result<(Vec<adw::PreferencesGroup>, Vec<Field>), String> {
    let mut groups = Vec::new();
    let mut fields = Vec::new();

    let main_width = store.get_i64("window", "width")?;
    let main_height = store.get_i64("window", "height")?;
    let position = group(
        "Main position",
        "Pixel offset from the center of each monitor.",
    );
    integer(
        &position,
        &mut fields,
        store,
        "window",
        "offset_x",
        "Move right / left",
        -2000.0,
        2000.0,
        1.0,
    )?;
    integer(
        &position,
        &mut fields,
        store,
        "window",
        "offset_y",
        "Move down / up",
        -2000.0,
        2000.0,
        1.0,
    )?;
    groups.push(position);

    let clock = group("Clock", "The center stays fixed when the font size changes. Large text may extend beyond the widget area.");
    number(
        &clock,
        &mut fields,
        store,
        "clock",
        "font_size",
        "Clock font size",
        4.0,
        256.0,
        0.5,
        2,
    )?;
    integer(
        &clock,
        &mut fields,
        store,
        "clock",
        "center_x",
        "Horizontal position",
        0.0,
        (main_width - 1) as f64,
        1.0,
    )?;
    integer(
        &clock,
        &mut fields,
        store,
        "clock",
        "center_y",
        "Vertical position",
        0.0,
        (main_height - 1) as f64,
        1.0,
    )?;
    text(
        &clock,
        &mut fields,
        store,
        "clock",
        "color",
        "Clock color (#RRGGBB or #RRGGBBAA)",
    )?;
    groups.push(clock);

    if store.get_f64("weekday", "label_font_size").is_ok() {
        let weekday = group("Weekday", "Day labels above the clock.");
        number(
            &weekday,
            &mut fields,
            store,
            "weekday",
            "label_font_size",
            "Day label font size",
            4.0,
            256.0,
            0.5,
            2,
        )?;
        text(
            &weekday,
            &mut fields,
            store,
            "weekday",
            "label_color",
            "Label color (#RRGGBB or #RRGGBBAA)",
        )?;
        groups.push(weekday);
    }

    if store.get_f64("greeting", "font_size").is_ok() {
        let greeting = group(
            "Greeting",
            "Text below the clock, positioned within the main widget area.",
        );
        number(
            &greeting,
            &mut fields,
            store,
            "greeting",
            "font_size",
            "Greeting font size",
            4.0,
            256.0,
            0.5,
            2,
        )?;
        integer(
            &greeting,
            &mut fields,
            store,
            "greeting",
            "center_x",
            "Horizontal position",
            0.0,
            (main_width - 1) as f64,
            1.0,
        )?;
        integer(
            &greeting,
            &mut fields,
            store,
            "greeting",
            "center_y",
            "Vertical position",
            0.0,
            (main_height - 1) as f64,
            1.0,
        )?;
        text(
            &greeting,
            &mut fields,
            store,
            "greeting",
            "name",
            "Name in greeting",
        )?;
        text(
            &greeting,
            &mut fields,
            store,
            "greeting",
            "color",
            "Text color (#RRGGBB or #RRGGBBAA)",
        )?;
        text(
            &greeting,
            &mut fields,
            store,
            "greeting",
            "name_color",
            "Name color (#RRGGBB or #RRGGBBAA)",
        )?;
        groups.push(greeting);
    }

    if store.get_i64("rings_window", "width").is_ok() {
        let ring_width = store.get_i64("rings_window", "width")?;
        let position = group(
            "Ring position",
            "Offset from the bottom edge and center of each monitor.",
        );
        integer(
            &position,
            &mut fields,
            store,
            "rings_window",
            "offset_x",
            "Move right / left",
            -2000.0,
            2000.0,
            1.0,
        )?;
        integer(
            &position,
            &mut fields,
            store,
            "rings_window",
            "offset_y",
            "Move down / up",
            -2000.0,
            2000.0,
            1.0,
        )?;
        groups.push(position);

        let rings = group(
            "Rings and sensors",
            "CPU, temperature, RAM, and refresh interval.",
        );
        number(
            &rings,
            &mut fields,
            store,
            "rings",
            "label_font_size",
            "Label font size",
            4.0,
            256.0,
            0.5,
            2,
        )?;
        number(
            &rings,
            &mut fields,
            store,
            "rings",
            "value_font_size",
            "Value font size",
            4.0,
            256.0,
            0.5,
            2,
        )?;
        integer(
            &rings,
            &mut fields,
            store,
            "rings",
            "refresh_ms",
            "Sensor refresh (ms)",
            250.0,
            60_000.0,
            250.0,
        )?;
        text(
            &rings,
            &mut fields,
            store,
            "rings",
            "label_color",
            "Label color (#RRGGBB or #RRGGBBAA)",
        )?;
        text(
            &rings,
            &mut fields,
            store,
            "rings",
            "value_color",
            "Value color (#RRGGBB or #RRGGBBAA)",
        )?;
        groups.push(rings);

        let radius = store.get_f64("rings", "radius")?;
        let thickness = store.get_f64("rings", "thickness")?;
        let margin = (radius + thickness / 2.0).ceil();
        let spacing = group(
            "Ring spacing",
            "Horizontal center of each ring within the ring widget area.",
        );
        for (section, label) in [
            ("rings.cpu", "CPU"),
            ("rings.temperature", "Temperature"),
            ("rings.ram", "RAM"),
        ] {
            integer(
                &spacing,
                &mut fields,
                store,
                section,
                "center_x",
                label,
                margin,
                ring_width as f64 - margin - 1.0,
                1.0,
            )?;
        }
        groups.push(spacing);
    }

    Ok((groups, fields))
}

fn group(title: &str, description: &str) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title(title);
    group.set_description(Some(description));
    group
}

#[allow(clippy::too_many_arguments)]
fn number(
    group: &adw::PreferencesGroup,
    fields: &mut Vec<Field>,
    store: &Store,
    section: &'static str,
    key: &'static str,
    title: &str,
    minimum: f64,
    maximum: f64,
    step: f64,
    digits: u32,
) -> Result<(), String> {
    let value = store.get_f64(section, key)?;
    let row = adw::SpinRow::with_range(minimum.min(value), maximum.max(value), step);
    row.set_title(title);
    row.set_numeric(true);
    row.set_digits(digits);
    row.set_value(value);
    group.add(&row);
    let initial = FieldValue::Float(row.value());
    fields.push(Field {
        section,
        key,
        editor: Editor::Float(row),
        initial,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn integer(
    group: &adw::PreferencesGroup,
    fields: &mut Vec<Field>,
    store: &Store,
    section: &'static str,
    key: &'static str,
    title: &str,
    minimum: f64,
    maximum: f64,
    step: f64,
) -> Result<(), String> {
    let value = store.get_i64(section, key)? as f64;
    let row = adw::SpinRow::with_range(minimum.min(value), maximum.max(value), step);
    row.set_title(title);
    row.set_numeric(true);
    row.set_value(value);
    group.add(&row);
    let initial = FieldValue::Integer(row.value().round() as i64);
    fields.push(Field {
        section,
        key,
        editor: Editor::Integer(row),
        initial,
    });
    Ok(())
}

fn text(
    group: &adw::PreferencesGroup,
    fields: &mut Vec<Field>,
    store: &Store,
    section: &'static str,
    key: &'static str,
    title: &str,
) -> Result<(), String> {
    let row = adw::EntryRow::new();
    row.set_title(title);
    row.set_text(&store.get_text(section, key)?);
    group.add(&row);
    let initial = FieldValue::Text(row.text().to_string());
    fields.push(Field {
        section,
        key,
        editor: Editor::Text(row),
        initial,
    });
    Ok(())
}

fn show_open_error(app: &adw::Application, error: &str) {
    let text = gtk::Label::new(Some(&format!("Settings could not be opened:\n{error}")));
    text.set_wrap(true);
    text.set_selectable(true);
    text.set_margin_top(32);
    text.set_margin_bottom(32);
    text.set_margin_start(32);
    text.set_margin_end(32);
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("zDesk — Settings")
        .default_width(480)
        .default_height(220)
        .content(&text)
        .build();
    window.present();
}

fn apply_when_running(button: gtk::Button, status: gtk::Label, overlay: adw::ToastOverlay) {
    adw::glib::spawn_future_local(async move {
        match run_systemctl(&["--user", "is-active", "zdesk.service"]).await {
            Ok((_, state)) if state == "activating" => {
                status.set_text("Saved. The widget is starting. Wait until it is active, then press Save & Apply again.");
                overlay.add_toast(adw::Toast::new("Settings saved"));
            }
            Ok((false, state))
                if matches!(
                    state.as_str(),
                    "inactive" | "failed" | "deactivating" | "unknown"
                ) =>
            {
                status.set_text(
                    "Saved. zDesk is not running; the settings will apply on its next start.",
                );
                overlay.add_toast(adw::Toast::new("Settings saved"));
            }
            Ok((true, state)) if state == "active" => {
                status.set_text("Restarting zDesk…");
                match run_systemctl(&["--user", "try-restart", "zdesk.service"]).await {
                    Ok((true, _)) => {
                        adw::glib::timeout_future_seconds(2).await;
                        match run_systemctl(&["--user", "is-active", "zdesk.service"]).await {
                            Ok((true, state)) if state == "active" => {
                                status.set_text("Saved. zDesk is running with the new settings.");
                                overlay.add_toast(adw::Toast::new("Changes applied"));
                            }
                            Ok((_, state)) | Err(state) => {
                                status.set_text(&format!(
                                    "Configuration saved, but zDesk is not active: {state}"
                                ));
                                overlay.add_toast(adw::Toast::new("Check zDesk status"));
                            }
                        }
                    }
                    Ok((false, reason)) | Err(reason) => {
                        status.set_text(&format!(
                            "Configuration saved, but the widget did not reload: {reason}"
                        ));
                        overlay.add_toast(adw::Toast::new("Try applying again"));
                    }
                }
            }
            Ok((_, reason)) | Err(reason) => {
                status.set_text(&format!(
                    "Configuration saved. Could not check the service status: {reason}"
                ));
                overlay.add_toast(adw::Toast::new("Settings saved"));
            }
        }
        button.set_sensitive(true);
    });
}

async fn run_systemctl(args: &[&str]) -> Result<(bool, String), String> {
    let arguments: Vec<&OsStr> = std::iter::once(OsStr::new("systemctl"))
        .chain(args.iter().map(|argument| OsStr::new(argument)))
        .collect();
    let process = adw::gio::Subprocess::newv(
        &arguments,
        adw::gio::SubprocessFlags::STDOUT_PIPE | adw::gio::SubprocessFlags::STDERR_PIPE,
    )
    .map_err(|error| error.to_string())?;
    let (stdout, stderr) = process
        .communicate_utf8_future(None)
        .await
        .map_err(|error| error.to_string())?;
    let error = stderr.map(|value| value.to_string()).unwrap_or_default();
    let output = stdout.map(|value| value.to_string()).unwrap_or_default();
    let message = if error.trim().is_empty() {
        output
    } else {
        error
    };
    Ok((process.is_successful(), message.trim().to_owned()))
}
