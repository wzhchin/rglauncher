use crate::pluginpreview::PluginPreview;
use gtk::glib::object::Cast;
use gtk::prelude::{GridExt, TextBufferExt};
use gtk::WrapMode::WordChar;
use rglcore::plugins::calc::CalcResult;

pub struct CalcPreview {
    root: gtk::Widget,
    formula_buffer: gtk::TextBuffer,
    result_buffer: gtk::TextBuffer,
}

impl PluginPreview for CalcPreview {
    type PluginResult = CalcResult;
    fn new() -> Self {
        let preview = gtk::Grid::builder().vexpand(true).hexpand(true).build();

        let formula_buffer = gtk::TextBuffer::builder().build();
        let formula_area = gtk::TextView::builder()
            .hexpand(true)
            .vexpand(true)
            .wrap_mode(WordChar)
            .margin_start(15)
            .margin_end(15)
            .margin_top(15)
            .margin_bottom(15)
            .buffer(&formula_buffer)
            .css_classes(["raw-box", "font-24"])
            .build();
        let formula_scroll = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(false)
            .propagate_natural_width(false)
            .child(&formula_area)
            .build();
        preview.attach(&formula_scroll, 0, 0, 1, 1);

        let sep = gtk::Separator::builder().hexpand(true).build();
        preview.attach(&sep, 0, 1, 1, 1);

        let result_buffer = gtk::TextBuffer::builder().build();
        let result_area = gtk::TextView::builder()
            .hexpand(true)
            .vexpand(true)
            .wrap_mode(WordChar)
            .margin_start(15)
            .margin_end(15)
            .margin_top(15)
            .margin_bottom(15)
            .css_classes(["raw-box", "font-24"])
            .buffer(&result_buffer)
            .build();
        let result_scroll = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(false)
            .propagate_natural_width(false)
            .child(&result_area)
            .build();
        preview.attach(&result_scroll, 0, 2, 1, 1);

        // Keep TextView content out of the stack's min-height path.
        let root = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(false)
            .propagate_natural_width(false)
            .child(&preview)
            .build();

        CalcPreview {
            root: root.upcast(),
            formula_buffer,
            result_buffer,
        }
    }

    fn get_preview(&self) -> gtk::Widget {
        self.root.clone()
    }

    fn set_preview(&self, plugin_result: &Self::PluginResult) {
        self.formula_buffer.set_text(plugin_result.formula.as_str());
        self.result_buffer.set_text(plugin_result.result.as_str());
    }

    fn get_id(&self) -> &str {
        rglcore::plugins::calc::TYPE_ID
    }
}
