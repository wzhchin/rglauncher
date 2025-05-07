mod imp;

use crate::iconcache;
use chin_tools::utils::string_util;
use gtk::glib;
use gtk::prelude::WidgetExt;
use gtk::subclass::prelude::*;

use rglcore::plugins::{PluginResult, PluginResultEnum};

glib::wrapper! {
    pub struct SidebarRow(ObjectSubclass<imp::SidebarRow>)
        @extends gtk::Widget, gtk::Box;
}

impl Default for SidebarRow {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn arrange_sidebar(&self, plugin_result: &PluginResultEnum) {
        let imp = self.imp();

        imp.image
            .set_from_pixbuf(Some(&iconcache::get_pixbuf(plugin_result.icon_name())));

        let name = plugin_result.name();

        imp.title.set_label(if name.len() > 300 {
            string_util::truncate(name, 300)
        } else {
            name
        });

        match plugin_result.extra() {
            Some(desc) => {
                let desc = if desc.len() > 300 {
                    string_util::truncate(desc, 300)
                } else {
                    desc
                };

                imp.extra.set_label(desc)
            }
            None => {
                imp.extra.hide();
            }
        }
    }

    pub fn unbind_all(&self) {
        let imp = self.imp();
        imp.image.clear();
    }
}
