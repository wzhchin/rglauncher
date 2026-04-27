use crate::constants;
use clap::Parser;
use rglcore::PluginType;

fn parse_plugin_types(s: &str) -> Result<Vec<PluginType>, String> {
    let mut types = Vec::new();
    for part in s.split(',') {
        match part.trim() {
            "calc" => types.push(PluginType::Calc),
            "win" => types.push(PluginType::Win),
            "app" => types.push(PluginType::App),
            #[cfg(feature = "clip")]
            "clip" => types.push(PluginType::Clip),
            other => return Err(format!("unknown plugin type: {}", other)),
        }
    }
    Ok(types)
}

#[derive(Parser, Default, Debug, Clone)]
#[command(author = constants::PROJECT_AUTHOR, version = constants::PROJECT_VERSION, about = constants::PROJECT_DESCRIPTION)]
pub struct Arguments {
    #[clap(long, help = "The file path of config file.")]
    pub config_file: Option<String>,

    #[clap(short, long, help = "Plugin types, comma-separated: win,app,calc,clip")]
    pub r#type: Option<String>,
}

impl Arguments {
    pub fn plugin_types(&self) -> Option<Vec<PluginType>> {
        self.r#type.as_ref().map(|s| parse_plugin_types(s).unwrap())
    }
}
