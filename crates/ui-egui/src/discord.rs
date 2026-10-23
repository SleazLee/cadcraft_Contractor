//! Rich Presence preferences only. Local Discord IPC belongs to the desktop host, never the
//! engine or drawing. `ui.discord-presence` is shared by the menu and control channel.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{CadApp, theme::Tokens};

/// Public Discord application identifier supplied by this fork's owner. Never a token/secret.
pub const APPLICATION_ID: &str = "1562973280875315221";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub application_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true, application_id: APPLICATION_ID.into() }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        let id = &self.application_id;
        if (!id.is_empty() || self.enabled)
            && (!(17..=20).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_digit()) || !id.parse::<u64>().is_ok_and(|n| n > 0))
        {
            return Err("Enter the numeric Discord Application ID (17–20 digits).".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    #[default]
    Unavailable,
    Off,
    Waiting,
    Connecting,
    Connected,
    Rejected,
    Error,
}

impl Status {
    pub fn message(self) -> &'static str {
        match self {
            Self::Unavailable => "Discord presence is unavailable in this build.",
            Self::Off => "Discord presence is off.",
            Self::Waiting => "Waiting for the Discord desktop app…",
            Self::Connecting => "Connecting to Discord…",
            Self::Connected => "Sharing CADCraft activity on Discord.",
            Self::Rejected => "Discord rejected the activity. Check your Application ID.",
            Self::Error => "Discord presence could not start.",
        }
    }
}

#[derive(Default)]
pub struct State {
    pub settings: Settings,
    pub status: Status,
    pub available: bool,
    draft: Option<Settings>,
    error: Option<String>,
}

impl State {
    pub fn load_settings(&mut self, value: &Value) -> Result<(), String> {
        let settings: Settings = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        settings.validate()?;
        self.settings = settings;
        Ok(())
    }
}

pub fn route(app: &mut CadApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != "ui.discord-presence" {
        return None;
    }
    if params.is_null() {
        app.discord.draft = Some(app.discord.settings.clone());
        app.discord.error = None;
        app.ui.dialog = Some("discord-presence".into());
    } else {
        if !params.is_object() {
            return Some(Err("Expected {settings: {enabled, applicationId}} or {} to read status".into()));
        }
        if let Some(settings) = params.get("settings")
            && let Err(error) = app.discord.load_settings(settings)
        {
            return Some(Err(error));
        }
    }
    Some(Ok(json!({"settings": app.discord.settings, "status": app.discord.status, "available": app.discord.available})))
}

pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let mut draft = app.discord.draft.take().unwrap_or_else(|| app.discord.settings.clone());
    let mut apply = false;
    let mut cancel = false;
    egui::Window::new(crate::tl!("Discord Rich Presence"))
        .id(egui::Id::new("discord-presence"))
        .open(open)
        .resizable(false)
        .collapsible(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            ui.label(crate::i18n::t(app.discord.status.message()));
            ui.separator();
            ui.add_enabled_ui(app.discord.available, |ui| {
                ui.checkbox(&mut draft.enabled, crate::tl!("Share that CADCraft is running"));
                ui.label(crate::tl!("Discord Application ID"));
                ui.add(egui::TextEdit::singleline(&mut draft.application_id).char_limit(20).desired_width(260.0));
                ui.label(crate::tl!("Create a Discord application named CADCraft, then paste its Application ID."));
                ui.hyperlink_to(crate::tl!("Open Discord Developer Portal"), "https://discord.com/developers/applications");
                ui.label(crate::tl!("Shown on Discord: Designing in CADCraft · Drafting and modeling"));
                ui.label(
                    egui::RichText::new(crate::tl!("Drawing names, paths and model contents stay private.")).small().color(Tokens::get().text_dim),
                );
            });
            if let Some(error) = &app.discord.error {
                ui.label(crate::i18n::t(error));
            }
            ui.separator();
            ui.horizontal(|ui| {
                apply = ui.add_enabled(app.discord.available, egui::Button::new(crate::tl!("Apply"))).clicked();
                cancel = ui.button(crate::tl!("Close")).clicked();
            });
        });
    if apply {
        match app.run("ui.discord-presence", json!({"settings": draft})) {
            Ok(_) => app.discord.error = None,
            Err(error) => app.discord.error = Some(error),
        }
    }
    app.discord.draft = Some(draft);
    if cancel {
        *open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_engine::Session;

    #[test]
    fn presence_preferences_round_trip_without_touching_the_drawing() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        let drawing = app.session.doc().unwrap().clone();
        let settings = json!({"enabled":true,"applicationId":"123456789012345678"});
        let result = app.run("ui.discord-presence", json!({"settings":settings})).unwrap();
        assert_eq!(result["settings"], settings);
        assert!(app.ui.dialog.is_none());
        assert_eq!(app.session.doc().unwrap(), &drawing);
        let mut other = CadApp::new(Session::new(), crate::Services::default());
        other.load_prefs(&app.prefs_json());
        assert_eq!(other.discord.settings, app.discord.settings);
        assert_eq!(other.discord.status, Status::Unavailable);
        app.run("ui.discord-presence", json!({"settings":{"enabled":false,"applicationId":"123456789012345678"}})).unwrap();
        other.load_prefs(&app.prefs_json());
        assert!(!other.discord.settings.enabled, "the off switch survives restart");
        app.run("ui.discord-presence", Value::Null).unwrap();
        assert_eq!(app.ui.dialog.as_deref(), Some("discord-presence"));
    }

    #[test]
    fn hostile_ids_are_atomic_and_disabled_defaults_need_no_id() {
        let mut app = CadApp::new(Session::new(), crate::Services::default());
        assert!(app.discord.settings.validate().is_ok());
        assert!(Settings { enabled: false, application_id: String::new() }.validate().is_ok());
        for id in ["", "0", "abc", "1234567890123456", "18446744073709551616", "１２３４５６７８９０１２３４５６７８", "../discord-ipc-0"]
        {
            assert!(app.run("ui.discord-presence", json!({"settings":{"enabled":true,"applicationId":id}})).is_err(), "{id}");
            assert_eq!(app.discord.settings, Settings::default());
        }
        assert!(app.run("ui.discord-presence", json!({"settings":{"enabled":"yes"}})).is_err());
        assert!(app.run("ui.discord-presence", json!({"settings":{"clientSecret":"never accepted"}})).is_err());
        assert!(app.run("ui.discord-presence", json!({})).is_ok());
        assert!(app.ui.dialog.is_none());
    }
}
