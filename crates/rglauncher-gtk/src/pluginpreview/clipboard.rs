use crate::iconcache;
use crate::pluginpreview::PluginPreview;
use rglcore::plugins::clip::ClipResult;
use rglcore::plugins::PluginResult;

use gtk::glib::object::Cast;
use gtk::pango::WrapMode::WordChar;
use gtk::prelude::BoxExt;
use gtk::Align::{Center, End};
use gtk::{Image, Orientation, Widget};

pub struct ClipPreview {
    preview: gtk::Widget,
    big_pic: gtk::Image,
    title: gtk::Label,
    content_type: gtk::Label,
    count: gtk::Label,
}

impl PluginPreview for ClipPreview {
    type PluginResult = ClipResult;

    fn new() -> Self {
        let r#box = gtk::Box::builder()
            .vexpand(true)
            .hexpand(true)
            .valign(Center)
            .halign(Center)
            .orientation(Orientation::Vertical)
            .build();

        let big_pic = Image::builder()
            .icon_name("clipboard")
            .pixel_size(256)
            .vexpand(true)
            .build();

        r#box.append(&big_pic);

        let title = gtk::Label::builder()
            .css_classes(["font-16"])
            .wrap(true)
            .wrap_mode(WordChar)
            .selectable(true)
            .build();
        r#box.append(&title);

        let sep = super::get_seprator();
        let extra = gtk::Grid::builder()
            .hexpand(true)
            .vexpand(false)
            .valign(End)
            .css_classes(["prev-btm-box"])
            .build();

        let content_type = super::build_pair_line(&extra, 1, "Type: ");
        let count = super::build_pair_line(&extra, 2, "Count: ");

        let sw = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();
        sw.set_child(Some(&r#box));

        let tb = gtk::Box::builder()
            .vexpand(true)
            .hexpand(true)
            .orientation(Orientation::Vertical)
            .build();

        tb.append(&sw);
        tb.append(&sep);
        tb.append(&extra);

        ClipPreview {
            preview: tb.upcast(),
            big_pic,
            title,
            content_type,
            count,
        }
    }

    fn get_preview(&self) -> Widget {
        self.preview.clone().upcast()
    }

    fn set_preview(&self, plugin_result: &Self::PluginResult) {
        self.title.set_text(plugin_result.display_name.as_str());
        self.content_type
            .set_label(&plugin_result.content_type);
        self.count.set_label(&plugin_result.count.to_string());
        self.big_pic
            .set_from_pixbuf(Some(&iconcache::get_pixbuf(plugin_result.icon_name())));
    }

    fn get_id(&self) -> &str {
        rglcore::plugins::clip::TYPE_ID
    }
}
