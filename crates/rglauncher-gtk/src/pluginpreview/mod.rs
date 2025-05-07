use crate::constants;
use crate::pluginpreview::application::AppPreview;
#[cfg(feature = "calc")]
use crate::pluginpreview::calculator::CalcPreview;
#[cfg(feature = "clip")]
use crate::pluginpreview::clipboard::ClipPreview;
#[cfg(feature = "mdict")]
use crate::pluginpreview::dictionary::DictPreview;
#[cfg(feature = "wmwin")]
use crate::pluginpreview::windows::WMWindowPreview;
use flume::Receiver;
use gtk::glib::MainContext;
use gtk::pango::WrapMode::WordChar;
use gtk::prelude::{GridExt, WidgetExt};
use gtk::Align::Center;
use gtk::BinLayout;
use rglcore::config::{Config, ParsedConfig};
use rglcore::plugins::{PRWrapper, PluginResult, PluginResultEnum};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

mod application;
#[cfg(feature = "calc")]
mod calculator;
#[cfg(feature = "clip")]
mod clipboard;
#[cfg(feature = "mdict")]
mod dictionary;
#[cfg(feature = "wmwin")]
mod windows;

const DEFAULT_ID: &str = "default";

pub trait PluginPreview {
    type PluginResult: PluginResult;

    fn new() -> Self
    where
        Self: Sized;

    fn get_preview(&self) -> gtk::Widget;

    fn set_preview(&self, plugin_result: &Self::PluginResult);

    fn get_id(&self) -> &str;
}

pub struct PluginPreviewBuilder {
    stack: gtk::Stack,

    app_preview: AppPreview,
    #[cfg(feature = "calc")]
    calc_preview: CalcPreview,
    #[cfg(feature = "clip")]
    clip_preview: ClipPreview,
    #[cfg(feature = "mdict")]
    dict_preview: DictPreview,
    #[cfg(feature = "wmwin")]
    wind_preview: WMWindowPreview,
}

impl PluginPreviewBuilder {
    pub fn new(stack: &gtk::Stack, config: &Config) -> Self {
        let app_preview = AppPreview::new();
        #[cfg(feature = "mdict")]
        let dict_preview = DictPreview::new();
        #[cfg(feature = "mdict")]
        stack.add_named(&dict_preview.get_preview(), Some(dict_preview.get_id()));

        #[cfg(feature = "calc")]
        let calc_preview = CalcPreview::new();
        #[cfg(feature = "calc")]
        stack.add_named(&calc_preview.get_preview(), Some(calc_preview.get_id()));

        #[cfg(feature = "clip")]
        let clip_preview = ClipPreview::new();
        #[cfg(feature = "clip")]
        stack.add_named(&clip_preview.get_preview(), Some(clip_preview.get_id()));

        #[cfg(feature = "wmwin")]
        let wind_preview = WMWindowPreview::new();
        #[cfg(feature = "wmwin")]
        stack.add_named(&wind_preview.get_preview(), Some(wind_preview.get_id()));

        stack.add_named(&app_preview.get_preview(), Some(app_preview.get_id()));

        let default = gtk::Label::builder()
            .label(gtk::glib::GString::from(constants::PROJECT_NAME))
            .vexpand(true)
            .hexpand(true)
            .valign(Center)
            .halign(Center)
            .css_classes(["logo-font", "dim-label"])
            .build();
        stack.add_named(&default, Some(DEFAULT_ID));
        stack.set_visible_child(&default);

        #[cfg(feature = "mdict")]
        dict_preview.add_csses(config.dict.as_ref());

        PluginPreviewBuilder {
            stack: stack.clone(),
            app_preview,
            #[cfg(feature = "calc")]
            calc_preview,
            #[cfg(feature = "clip")]
            clip_preview,
            #[cfg(feature = "mdict")]
            dict_preview,
            #[cfg(feature = "wmwin")]
            wind_preview,
        }
    }

    pub fn set_preview(&self, pr: Option<&PluginResultEnum>) -> Option<()> {
        if let Some(plugin_result) = pr {
            match plugin_result {
                #[cfg(feature = "mdict")]
                PluginResultEnum::MDict(r) => self.dict_preview.set_preview(r),
                PluginResultEnum::Calc(r) => self.calc_preview.set_preview(r),
                PluginResultEnum::App(r) => self.app_preview.set_preview(r),
                PluginResultEnum::Win(r) => self.wind_preview.set_preview(r),
                #[cfg(feature = "clip")]
                PluginResultEnum::Clip(r) => self.clip_preview.set_preview(r),
            };

            self.stack
                .set_visible_child_name(plugin_result.get_type_id());
        } else {
            self.stack.set_visible_child_name(DEFAULT_ID);
        }

        None
    }
}

fn build_pair_line(grid: &gtk::Grid, row: i32, title: &str) -> gtk::Label {
    let left = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .wrap(true)
        .wrap_mode(WordChar)
        .label(title)
        .hexpand(true)
        .build();
    left.add_css_class("dim-label");
    left.add_css_class("font-12");
    let right = gtk::Label::builder()
        .halign(gtk::Align::End)
        .wrap(true)
        .wrap_mode(WordChar)
        .build();
    grid.attach(&left, 0, row, 1, 1);
    grid.attach(&right, 1, row, 1, 1);

    right
}

fn get_seprator() -> gtk::Separator {
    gtk::Separator::builder()
        .hexpand(true)
        .css_classes(["mm-10"])
        .build()
}

pub enum PreviewMsg {
    PluginResult(PRWrapper),
    Clear,
}

#[derive(Clone)]
pub struct Preview {
    preview_rx: Receiver<PreviewMsg>,

    pub preview_window: gtk::Stack,
    config: Arc<ParsedConfig>,
}

impl Preview {
    pub fn new(preview_rx: Receiver<PreviewMsg>, config: Arc<ParsedConfig>) -> Self {
        let preview_window = gtk::Stack::builder()
            .vexpand(true)
            .hexpand(true)
            .css_classes(["preview"])
            .build();

        Preview {
            preview_rx,
            preview_window,
            config,
        }
    }

    pub fn loop_recv(&self) {
        let preview_window = self.preview_window.clone();
        let preview_rx = self.preview_rx.clone();
        let config = self.config.clone();
        MainContext::ref_thread_default().spawn_local(async move {
            let plugin_previews = Rc::new(RefCell::new(PluginPreviewBuilder::new(
                &preview_window,
                &config,
            )));
            loop {
                if let Ok(preview_msg) = preview_rx.recv_async().await {
                    let plugin_preview_builder = plugin_previews.clone();
                    let opt_plugin_result = match preview_msg {
                        PreviewMsg::PluginResult(pr) => Some(pr),
                        PreviewMsg::Clear => None,
                    };
                    gtk::glib::idle_add_local_once(move || {
                        plugin_preview_builder
                            .borrow()
                            .set_preview(opt_plugin_result.as_ref().map(|v| &**v));
                    });
                }
            }
        });
    }
}
