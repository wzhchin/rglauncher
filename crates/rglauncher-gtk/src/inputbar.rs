use flume::Sender;
use gtk::prelude::EditableExt;
use rglcore::userinput::{Signal, UserInput};
use rglcore::ResultMsg;

use crate::window::WindowMsg;
use gtk::glib::{ControlFlow, MainContext};
use gtk::prelude::EntryExt;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum InputMessage {
    TextChanged(String),
    TextAppend(String),
    Clear,
    Focus,
}

#[derive(Clone)]
pub struct InputBar {
    pub entry: gtk::Entry,
    pub input_tx: Sender<InputMessage>,
}

impl InputBar {
    pub fn new(result_tx: &Sender<ResultMsg>, window_tx: &Sender<WindowMsg>) -> Self {
        let (input_tx, input_rx) = flume::unbounded();

        let entry = gtk::Entry::builder()
            .placeholder_text("Input Anything...")
            .css_classes(["inputbar"])
            .has_frame(false)
            .build();

        let signal = Signal::new();
        {
            let result_tx = result_tx.clone();
            let signal = signal.clone();
            entry.connect_changed(move |e| {
                let text = e.text().to_string();
                result_tx
                    .send(ResultMsg::UserInput(UserInput::new(&text, &signal)))
                    .expect("Unable to send user input");
            });
        }
        result_tx
            .send(ResultMsg::UserInput(UserInput::new("", &signal)))
            .expect("Unable to send init message");

        {
            let result_tx = result_tx.clone();
            let window_tx = window_tx.clone();
            entry.connect_activate(move |_e| {
                result_tx
                    .send(ResultMsg::SelectSomething)
                    .expect("TODO: panic message");
                window_tx
                    .send(WindowMsg::Close)
                    .expect("unable to close window");
            });
        }

        {
            let entry = entry.clone();
            MainContext::ref_thread_default().spawn_local(async move {
                match input_rx.recv_async().await {
                    Ok(input_msg) => match input_msg {
                        InputMessage::TextChanged(input) => {
                            entry.set_text(input.as_str());
                        }
                        InputMessage::Clear => {
                            entry.set_text("");
                        }
                        InputMessage::TextAppend(cs) => {
                            let mut pos: i32 = entry.text_length() as i32;
                            entry.insert_text(cs.as_str(), &mut pos);
                        }
                        InputMessage::Focus => {
                            entry.grab_focus_without_selecting();
                        }
                    },
                    Err(_) => {}
                }

                ControlFlow::Continue
            });
        }

        InputBar { entry, input_tx }
    }
}
