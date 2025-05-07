use arc_swap::ArcSwap;
use chin_tools::{AResult, EResult};
use fragile::Fragile;
use gtk::gio;
use gtk::gio::MemoryInputStream;

use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib::Bytes;
use lazy_static::lazy_static;
use rglcore::config::ParsedConfig;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fs::read_to_string;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

lazy_static! {
    static ref ICON_PATHS: ArcSwap<Vec<PathBuf>> = ArcSwap::new(Arc::new(Vec::new()));
    static ref ICON_MAP: ArcSwap<HashMap<smol_str::SmolStr, Option<Arc<Fragile<Pixbuf>>>>> =
        ArcSwap::new(Arc::new(HashMap::new()));
    static ref ALIAS_MAP: ArcSwap<HashMap<smol_str::SmolStr, smol_str::SmolStr>> =
        ArcSwap::new(Arc::new(HashMap::new()));
    static ref LOGO: Arc<Fragile<Pixbuf>> = Arc::new(Fragile::new(
        load_from_svg(include_str!("../../../data/logo.svg")).unwrap()
    ));
}

fn load_from_svg(svg_data: &str) -> AResult<Pixbuf> {
    let image_stream = MemoryInputStream::from_bytes(&Bytes::from(svg_data.as_bytes()));

    Ok(Pixbuf::from_stream_at_scale(
        &image_stream,
        256,
        256,
        true,
        None::<&gio::Cancellable>,
    )?)
}

pub fn set_config(config: &ParsedConfig) -> EResult {
    tracing::info!("set icon config: {:?}", config.icon);
    if let Some(icon_config) = &config.icon {
        let paths: Result<Vec<PathBuf>, Infallible> = icon_config
            .paths
            .iter()
            .map(|e| PathBuf::from_str(e))
            .collect();

        ICON_PATHS.store(Arc::new(paths?));

        let mut alias = HashMap::new();
        icon_config.alias.iter().for_each(|(icon_name, aliases)| {
            for ele in aliases {
                alias.insert(ele.to_lowercase().into(), icon_name.to_lowercase().into());
            }
        });

        ALIAS_MAP.store(Arc::new(alias));
    }

    Ok(())
}

pub fn get_pixbuf(name: &str) -> Pixbuf {
    if let Some(icon) = get_pixbuf_inner(name) {
        icon.get().clone()
    } else {
        LOGO.get().clone()
    }
}

fn get_pixbuf_inner(name: &str) -> Option<Arc<Fragile<Pixbuf>>> {
    let name = name.to_lowercase();
    if let Some(icon) = ICON_MAP.load().get(name.as_str()) {
        icon.clone()
    } else if let Some(mapped) = ALIAS_MAP.load().get(name.as_str()) {
        return get_pixbuf_inner(&mapped);
    } else {
        let icon = ICON_PATHS
            .load()
            .iter()
            .filter_map(|e| {
                let svg_path = e.join(format!("{}.svg", name));
                if svg_path.exists() {
                    read_to_string(svg_path)
                        .ok()
                        .and_then(|e| load_from_svg(&e).ok())
                } else {
                    None
                }
            })
            .nth(0);

        let icon = icon.clone().map(|e| Arc::new(Fragile::from(e)));
        let mut icon_map: HashMap<SmolStr, Option<Arc<Fragile<Pixbuf>>>> = ICON_MAP
            .load()
            .iter()
            .map(|(key, icon)| (key.clone(), icon.clone()))
            .collect();
        icon_map.insert(name.to_lowercase().into(), icon.clone());
        ICON_MAP.store(Arc::new(icon_map));

        return icon;
    }
}
