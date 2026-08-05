use crate::application::RGLApplication;
use crate::constants;
use crate::inputbar::{InputBar, InputMessage};
use crate::launcher::LauncherMsg;
use crate::pluginpreview::Preview;
use crate::resulthandler::ResultHolder;
use crate::sidebar::SidebarMsg;
use flume::{Receiver, Sender};
use gtk::glib::{clone, MainContext};
use gtk::prelude::{BoxExt, EntryBufferExtManual, EntryExt, GridExt, GtkWindowExt, WidgetExt};
use gtk::{gdk, glib, ApplicationWindow};
use rglcore::config::ParsedConfig;
use rglcore::dispatcher::DispatchMsg;
use rglcore::{PluginType, ResultMsg};
use std::sync::Arc;

pub enum WindowMsg {
    Close,
}

#[derive(Clone)]
pub struct RGWindow {
    window: ApplicationWindow,
    input_bar: InputBar,

    pub window_tx: Sender<WindowMsg>,
    window_rx: Receiver<WindowMsg>,

    sidebar_tx: Sender<SidebarMsg>,
    result_tx: Sender<ResultMsg>,
}

impl RGWindow {
    pub fn new(
        app: &RGLApplication,
        config: Arc<ParsedConfig>,
        dispatch_tx: &flume::Sender<DispatchMsg>,
        launcher_tx: &Sender<LauncherMsg>,
        plugin_types: Vec<PluginType>,
    ) -> Self {
        let (sidebar_tx, sidebar_rx) = flume::unbounded();
        let (preview_tx, preview_rx) = flume::unbounded();
        let (window_tx, window_rx) = flume::unbounded();

        let result_tx = ResultHolder::start(launcher_tx, dispatch_tx, &sidebar_tx, &preview_tx, plugin_types);

        let window = ApplicationWindow::builder()
            .default_width(810)
            .default_height(520)
            .application(app)
            .resizable(false)
            .title(constants::PROJECT_NAME)
            .decorated(false)
            .css_classes(["rgwindow"])
            .build();

        // Horizontal GtkBox height measure uses unconstrained (for_size=-1) mins
        // but allocated-width naturals. A child with a large unconstrained min
        // (e.g. 256px icons) and a small natural at half-width yields
        // min > natural. Use Grid so both columns share one height request.
        let main_grid = gtk::Grid::builder()
            .column_homogeneous(false)
            .row_homogeneous(true)
            .hexpand(true)
            .vexpand(true)
            .build();

        window.set_child(Some(&main_grid));

        let inputbar = InputBar::new(&result_tx, &window_tx);
        let left_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .vexpand(true)
            .css_classes(["left-pane"])
            .build();

        left_box.append(&inputbar.entry);

        let mut sidebar = crate::sidebar::Sidebar::new(
            &result_tx,
            &sidebar_tx,
            &sidebar_rx,
            &window_tx,
            &inputbar.input_tx,
        );
        let sidebar_window = &sidebar.scrolled_window;
        let sidebar_tx = sidebar.sidebar_tx.clone();
        left_box.append(sidebar_window);

        sidebar.loop_recv();

        let preview = Preview::new(preview_rx, config);

        main_grid.attach(&left_box, 0, 0, 1, 1);
        main_grid.attach(&preview.preview_window, 1, 0, 1, 1);

        preview.loop_recv();

        window.present();

        Self {
            window,
            input_bar: inputbar,

            window_tx,
            window_rx,

            result_tx,

            sidebar_tx,
        }
    }

    fn receive_messages(&self) {
        let window = self.window.clone();
        let window_rx = self.window_rx.clone();
        MainContext::ref_thread_default().spawn_local(async move {
            loop {
                match window_rx.recv_async().await {
                    Ok(WindowMsg::Close) => {
                        RGWindow::close_window(&window.clone());
                        break;
                    }
                    _ => {}
                }
            }
        });
    }

    fn setup_keybindings(&self) {
        let controller = gtk::EventControllerKey::new();
        let sidebar_tx = self.sidebar_tx.clone();
        let result_tx = self.result_tx.clone();
        let entry = self.input_bar.entry.clone();
        let inputbar_tx = self.input_bar.input_tx.clone();
        let window_tx = self.window_tx.clone();
        let window = &self.window;

        controller.connect_key_pressed(clone!(
            #[strong]
            entry,
            move |_, key, _keycode, _| {
                match key {
                    gdk::Key::Up => {
                        sidebar_tx.send(SidebarMsg::PreviousItem).unwrap();
                        glib::Propagation::Stop
                    }
                    gdk::Key::Down => {
                        sidebar_tx.send(SidebarMsg::NextItem).unwrap();
                        glib::Propagation::Stop
                    }
                    gdk::Key::Escape => {
                        window_tx
                            .send(WindowMsg::Close)
                            .expect("unable to close window");
                        glib::Propagation::Stop
                    }
                    gdk::Key::Return => {
                        result_tx
                            .send(ResultMsg::SelectSomething)
                            .expect("select something");
                        inputbar_tx
                            .send(InputMessage::Clear)
                            .expect("unable to clear");
                        window_tx
                            .send(WindowMsg::Close)
                            .expect("unable to close window");
                        glib::Propagation::Proceed
                    }
                    _ => {
                        if !(key.is_lower() && key.is_upper()) {
                            if let Some(key_name) = key.name() {
                                let buffer = entry.buffer();

                                let content = buffer.text();
                                buffer.insert_text((content.len()) as u16, key_name);
                            }
                        }

                        glib::Propagation::Proceed
                    }
                }
            }
        ));
        window.add_controller(controller);
    }

    pub fn setup_one(
        app: &RGLApplication,
        arguments: Arc<ParsedConfig>,
        dispatch_tx: &flume::Sender<DispatchMsg>,
        launcher_tx: &Sender<LauncherMsg>,
        plugin_types: Vec<PluginType>,
    ) {
        let window = Self::new(app, arguments, dispatch_tx, launcher_tx, plugin_types);

        window.setup_keybindings();
        window.receive_messages();
    }

    pub fn close_window(window: &ApplicationWindow) {
        let window = window.clone();

        glib::idle_add_local_once(move || {
            window.destroy();
        });
    }
}
