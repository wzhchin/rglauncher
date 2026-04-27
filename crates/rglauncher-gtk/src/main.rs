mod application;
mod arguments;
mod constants;
mod iconcache;
mod inputbar;
mod launcher;
mod pluginpreview;
pub mod resulthandler;
mod sidebar;
mod sidebarrow;
mod window;

use chin_tools::{AResult, EResult};
use clap::Parser;
use rglcore::config::Config;
use rglcore::PluginType;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use tracing::*;

use gtk::prelude::*;

use crate::application::RGLApplication;
use crate::launcher::LauncherMsg;
use flume::Sender;
use std::os::unix::net::{UnixListener, UnixStream};

pub fn daemon(arguments: arguments::Arguments) -> EResult {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_thread_ids(true)
        .with_timer(tracing_subscriber::fmt::time::time())
        .init();

    let plugin_types = arguments.plugin_types().unwrap_or_default();

    let (launcher_tx, launcher_rx) = flume::unbounded();

    let app_msg_tx = launcher_tx.clone();
    std::thread::spawn(move || {
        build_uds(&app_msg_tx).expect("unable to build unix domain socket");
    });

    let mut app = RGLApplication::new();

    let config = Arc::new(Config::read_from_toml_file(arguments.config_file.as_ref())?);
    iconcache::set_config(&config)?;

    let launcher = launcher::Launcher::spawn(app.clone(), config, plugin_types, &launcher_tx, &launcher_rx)?;

    app.set_launcher(launcher);
    app.set_hold();

    let empty_args: Vec<String> = vec![];
    app.run_with_args(&empty_args);

    Ok(())
}

fn parse_socket_msg(msg: &str) -> LauncherMsg {
    if let Some(type_str) = msg.strip_prefix("new_window:") {
        let mut types = Vec::new();
        for part in type_str.split(',') {
            match part.trim() {
                "calc" => types.push(PluginType::Calc),
                "win" => types.push(PluginType::Win),
                "app" => types.push(PluginType::App),
                #[cfg(feature = "clip")]
                "clip" => types.push(PluginType::Clip),
                _ => {}
            }
        }
        LauncherMsg::NewWindow(types)
    } else if msg == "new_window" {
        LauncherMsg::NewWindow(vec![])
    } else {
        LauncherMsg::NewWindow(vec![])
    }
}

fn build_uds(app_msg_tx: &Sender<LauncherMsg>) -> AResult<()> {
    if !Path::new(constants::TMP_DIR).exists() {
        std::fs::create_dir(constants::TMP_DIR)?;
    }

    if Path::new(constants::UNIX_SOCKET_PATH).exists() {
        std::fs::remove_file(constants::UNIX_SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(constants::UNIX_SOCKET_PATH)?;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut response = String::new();
                stream.read_to_string(&mut response)?;
                info!("Got Echo {}", response);

                let msg = parse_socket_msg(&response);
                if matches!(msg, LauncherMsg::NewWindow(_)) {
                    info!("Creating new window.");
                    app_msg_tx.send(msg)?;
                }
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

fn main() -> EResult {
    let arguments = arguments::Arguments::parse();

    let socket_msg = match &arguments.r#type {
        Some(t) => format!("new_window:{}", t),
        None => "new_window".to_string(),
    };

    match UnixStream::connect(constants::UNIX_SOCKET_PATH) {
        Ok(mut stream) => {
            stream.write_all(socket_msg.as_bytes())?;
        }
        Err(_) => {
            daemon(arguments)?;
        }
    }

    Ok(())
}
