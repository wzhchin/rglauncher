use crate::iconcache;
use crate::pluginpreview::PluginPreview;
use rglcore::plugins::win::WinResult;
use rglcore::plugins::PluginResult;

use gtk::glib::object::Cast;
use gtk::pango::WrapMode::WordChar;
use gtk::prelude::BoxExt;
use gtk::Align::{Center, End};
use gtk::{Image, Orientation, Widget};

pub struct WMWindowPreview {
    preview: gtk::Widget,
    big_pic: gtk::Image,
    title: gtk::Label,
    workspace: gtk::Label,
}

impl PluginPreview for WMWindowPreview {
    type PluginResult = WinResult;

    fn new() -> Self {
        // Content area: icon + title, scrollable if the window is short.
        // pixel_size(256) sets a large min height; ScrolledWindow must not
        // propagate that as the parent's min height or GtkBox measure breaks.
        let content = gtk::Box::builder()
            .vexpand(true)
            .hexpand(true)
            .valign(Center)
            .halign(Center)
            .orientation(Orientation::Vertical)
            .build();

        let big_pic = Image::builder()
            .icon_name("gnome-windows")
            .pixel_size(128)
            .build();
        content.append(&big_pic);

        let title = gtk::Label::builder()
            .css_classes(["font-16"])
            .wrap(true)
            .wrap_mode(WordChar)
            .max_width_chars(40)
            .selectable(true)
            .build();
        content.append(&title);

        let sw = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(false)
            .propagate_natural_width(false)
            .build();
        sw.set_child(Some(&content));

        let sep = super::get_seprator();
        let extra = gtk::Grid::builder()
            .hexpand(true)
            .vexpand(false)
            .valign(End)
            .css_classes(["prev-btm-box"])
            .build();

        let workspace = super::build_pair_line(&extra, 1, "Workspace: ");

        let tb = gtk::Box::builder()
            .vexpand(true)
            .hexpand(true)
            .orientation(Orientation::Vertical)
            .build();

        tb.append(&sw);
        tb.append(&sep);
        tb.append(&extra);

        WMWindowPreview {
            preview: tb.upcast(),
            big_pic,
            title,
            workspace,
        }
    }

    fn get_preview(&self) -> Widget {
        self.preview.clone().upcast()
    }

    fn set_preview(&self, plugin_result: &Self::PluginResult) {
        self.title.set_text(plugin_result.title.as_str());

        self.workspace.set_label(&plugin_result.workspace);
        self.big_pic
            .set_from_pixbuf(Some(&iconcache::get_pixbuf(plugin_result.icon_name())));
    }

    fn get_id(&self) -> &str {
        rglcore::plugins::win::TYPE_ID
    }
}
