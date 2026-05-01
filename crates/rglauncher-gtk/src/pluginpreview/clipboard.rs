use crate::pluginpreview::PluginPreview;
use gtk::glib::object::Cast;
use gtk::pango::WrapMode::WordChar as PangoWordChar;
use gtk::prelude::{BoxExt, TextBufferExt, WidgetExt};
use gtk::Align::{End, Fill, Start};
use gtk::WrapMode::WordChar;
use gtk::{Orientation, TextBuffer, TextView, Widget};

use rglcore::plugins::clip::ClipResult;

pub struct ClipPreview {
    preview: gtk::Widget,
    title: gtk::Label,
    text_buffer: gtk::TextBuffer,
    text_window: gtk::ScrolledWindow,
    picture_window: gtk::ScrolledWindow,
    picture: gtk::Picture,
    content_type: gtk::Label,
    count: gtk::Label,
}

impl PluginPreview for ClipPreview {
    type PluginResult = ClipResult;

    fn new() -> Self {
        let r#box = gtk::Box::builder()
            .vexpand(true)
            .hexpand(true)
            .valign(Fill)
            .halign(Fill)
            .orientation(Orientation::Vertical)
            .build();

        let title = gtk::Label::builder()
            .css_classes(["font-16"])
            .wrap(true)
            .wrap_mode(PangoWordChar)
            .selectable(true)
            .halign(Start)
            .margin_start(10)
            .margin_top(10)
            .build();
        r#box.append(&title);

        let text_buffer = TextBuffer::builder().build();
        let text_view = TextView::builder()
            .hexpand(true)
            .wrap_mode(WordChar)
            .css_classes(["raw-box"])
            .buffer(&text_buffer)
            .vexpand(true)
            .focusable(false)
            .valign(Start)
            .halign(Fill)
            .margin_start(10)
            .margin_end(10)
            .margin_top(10)
            .build();

        let text_window = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .build();
        text_window.set_child(Some(&text_view));

        let picture = gtk::Picture::builder()
            .keep_aspect_ratio(true)
            .can_shrink(true)
            .hexpand(true)
            .vexpand(true)
            .halign(Fill)
            .valign(Fill)
            .margin_start(10)
            .margin_end(10)
            .margin_top(10)
            .build();

        let picture_window = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .visible(false)
            .build();
        picture_window.set_child(Some(&picture));

        r#box.append(&text_window);
        r#box.append(&picture_window);

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
            title,
            text_buffer,
            text_window,
            picture_window,
            picture,
            content_type,
            count,
        }
    }

    fn get_preview(&self) -> Widget {
        self.preview.clone().upcast()
    }

    fn set_preview(&self, plugin_result: &Self::PluginResult) {
        self.content_type
            .set_label(&plugin_result.content_type);
        self.count.set_label(&plugin_result.count.to_string());

        if plugin_result.is_image {
            let file = gtk::gio::File::for_path(&plugin_result.content);
            self.picture.set_file(Some(&file));
            self.picture_window.set_visible(true);
            self.text_window.set_visible(false);
            self.title.set_visible(false);
        } else {
            self.text_buffer.set_text(plugin_result.content.as_str());
            self.text_window.set_visible(true);
            self.picture_window.set_visible(false);
            self.title.set_visible(true);
            self.title.set_text(plugin_result.display_name.as_str());
        }
    }

    fn get_id(&self) -> &str {
        rglcore::plugins::clip::TYPE_ID
    }
}
