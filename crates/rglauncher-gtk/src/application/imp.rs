use crate::launcher::Launcher;
use gtk::gio::ApplicationHoldGuard;
use gtk::gdk::Display;
use gtk::subclass::prelude::*;
use gtk::{glib, style_context_add_provider_for_display, CssProvider};
use std::cell::OnceCell;
use tracing::info;

use std::borrow::Borrow;
#[derive(Default)]
pub struct RGLApplication {
    pub(crate) launcher: OnceCell<Launcher>,
    pub background_hold: OnceCell<ApplicationHoldGuard>,
}

#[glib::object_subclass]
impl ObjectSubclass for RGLApplication {
    const NAME: &'static str = "ExApplication";
    type Type = super::RGLApplication;
    type ParentType = gtk::Application;
}

impl ObjectImpl for RGLApplication {}
impl ApplicationImpl for RGLApplication {
    fn activate(&self) {
        info!("Activating");
        self.parent_activate();
        if let Some(launcher) = self.launcher.borrow().get() {
            launcher.new_window();
        }
    }

    fn startup(&self) {
        self.parent_startup();
        self.load_css();
    }
}
impl GtkApplicationImpl for RGLApplication {}

impl RGLApplication {
    fn load_css(&self) {
        let provider = CssProvider::new();
        let layout_css = include_str!("../styles/layout.css");
        let light_css = include_str!("../styles/light.css");
        let dark_css = include_str!("../styles/dark.css");

        let dark_mode = match self.launcher.borrow().get() {
            Some(launcher) => launcher
                .config
                .ui
                .as_ref()
                .map_or(false, |e| e.dark_mode.is_some_and(|m| m)),
            None => false,
        };

        let css = format!(
            "{}\n{}",
            if dark_mode { dark_css } else { light_css },
            layout_css
        );

        provider.load_from_data(&css);

        style_context_add_provider_for_display(
            &Display::default().expect("Could not connect to a display."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
