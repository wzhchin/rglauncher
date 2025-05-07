use crate::application::RGLApplication;
use crate::constants;
use crate::inputbar::{InputBar, InputMessage};
use crate::launcher::LauncherMsg;
use crate::pluginpreview::Preview;
use crate::resulthandler::ResultHolder;
use crate::sidebar::SidebarMsg;
use flume::{Receiver, Sender};
use gtk::glib::{clone, MainContext};
use gtk::{glib, BinLayout};
use gtk::prelude::{EntryBufferExtManual, LayoutManagerExt};
use gtk::prelude::{BoxExt, EntryExt, GtkWindowExt, WidgetExt};
use gtk::{gdk, ApplicationWindow};
use rglcore::config::ParsedConfig;
use rglcore::dispatcher::DispatchMsg;
use rglcore::ResultMsg;
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
    ) -> Self {
        let (sidebar_tx, sidebar_rx) = flume::unbounded();
        let (preview_tx, preview_rx) = flume::unbounded();
        let (window_tx, window_rx) = flume::unbounded();

        let result_tx = ResultHolder::start(launcher_tx, dispatch_tx, &sidebar_tx, &preview_tx);

        let window = ApplicationWindow::builder()
            .default_width(810)
            .default_height(520)
            .application(app)
            .resizable(false)
            .title(constants::PROJECT_NAME)
            .decorated(false)
            .css_classes(["rgwindow"])
            .build();

        let main_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .build();

        window.set_child(Some(&main_box));

        let inputbar = InputBar::new(&result_tx, &window_tx);
        let left_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();

        main_box.append(&left_box);
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
        main_box.append(&preview.preview_window.clone());

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
    ) {
        let window = Self::new(app, arguments, dispatch_tx, launcher_tx);

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
