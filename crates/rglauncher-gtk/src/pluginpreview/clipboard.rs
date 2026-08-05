use crate::pluginpreview::PluginPreview;
use chrono::Local;
use gtk::glib::object::Cast;
use gtk::pango::WrapMode::WordChar as PangoWordChar;
use gtk::prelude::{BoxExt, TextBufferExt};
use gtk::Align::{End, Fill, Start};
use gtk::WrapMode::WordChar;
use gtk::{Orientation, Stack, StackTransitionType, TextBuffer, TextView, Widget};

use rglcore::plugins::clip::ClipResult;

const STACK_PAGE_TEXT: &str = "text";
const STACK_PAGE_PICTURE: &str = "picture";

pub struct ClipPreview {
    preview: gtk::Widget,
    text_buffer: gtk::TextBuffer,
    stack: Stack,
    picture: gtk::Picture,
    content_type: gtk::Label,
    count: gtk::Label,
    time: gtk::Label,
}

impl PluginPreview for ClipPreview {
    type PluginResult = ClipResult;

    fn new() -> Self {
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

        // TextView is scrollable; one ScrolledWindow is enough. Do not nest
        // another ScrolledWindow around this stack — that breaks height measure
        // (min height can exceed natural height on GtkBox parents).
        let text_window = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(false)
            .propagate_natural_width(false)
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
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(false)
            .propagate_natural_width(false)
            .build();
        picture_window.set_child(Some(&picture));

        let stack = Stack::builder()
            .hexpand(true)
            .vexpand(true)
            .vhomogeneous(false)
            .hhomogeneous(false)
            .transition_type(StackTransitionType::Crossfade)
            .build();
        stack.add_titled(&text_window, Some(STACK_PAGE_TEXT), "Text");
        stack.add_titled(&picture_window, Some(STACK_PAGE_PICTURE), "Picture");

        let sep = super::get_seprator();
        let extra = gtk::Grid::builder()
            .hexpand(true)
            .vexpand(false)
            .valign(End)
            .css_classes(["prev-btm-box"])
            .build();

        let content_type = super::build_pair_line(&extra, 1, "Type: ");
        let count = super::build_pair_line(&extra, 2, "Count: ");
        let time = super::build_pair_line(&extra, 3, "Time: ");

        // Content stack + fixed footer. Footer stays outside the scroll area.
        let tb = gtk::Box::builder()
            .vexpand(true)
            .hexpand(true)
            .orientation(Orientation::Vertical)
            .build();

        tb.append(&stack);
        tb.append(&sep);
        tb.append(&extra);

        ClipPreview {
            preview: tb.upcast(),
            text_buffer,
            stack,
            picture,
            content_type,
            count,
            time,
        }
    }

    fn get_preview(&self) -> Widget {
        self.preview.clone().upcast()
    }

    fn set_preview(&self, plugin_result: &Self::PluginResult) {
        self.content_type.set_label(&plugin_result.content_type);
        self.count.set_label(&plugin_result.count.to_string());
        let local_time = plugin_result.update_time.with_timezone(&Local);
        self.time
            .set_label(&local_time.format("%Y-%m-%d %H:%M:%S").to_string());

        if plugin_result.is_image {
            let file = gtk::gio::File::for_path(&plugin_result.content);
            self.picture.set_file(Some(&file));
            self.stack.set_visible_child_name(STACK_PAGE_PICTURE);
        } else {
            self.text_buffer.set_text(plugin_result.content.as_str());
            self.stack.set_visible_child_name(STACK_PAGE_TEXT);
        }
    }

    fn get_id(&self) -> &str {
        rglcore::plugins::clip::TYPE_ID
    }
}
