pub mod app;
#[cfg(feature = "calc")]
pub mod calc;
#[cfg(feature = "clip")]
pub mod clip;
pub mod history;
#[cfg(feature = "wmwin")]
pub mod win;

use std::ops::Deref;

use app::AppPlugin;
use calc::CalcPlugin;
use chin_tools::{AResult, EResult};
#[cfg(feature = "clip")]
use clip::ClipPlugin;
use history::HistoryItem;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use win::WinPlugin;

#[cfg(feature = "calc")]
use crate::plugins::calc::{CalcReq, CalcResult};
#[cfg(feature = "clip")]
use crate::plugins::clip::{ClipReq, ClipResult};
#[cfg(feature = "wmwin")]
use crate::plugins::win::{WinResult, WindowMsg};
use crate::plugins::app::{AppReq, AppResult};

use crate::userinput::UserInput;

pub trait Plugin: Send + Sync {
    type R: PluginResult;
    type T: Send;

    fn handle_msg(&self, _msg: Self::T) {}

    fn refresh_content(&self) {}

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(Self::R, i32)>>;

    fn get_type_id(&self) -> &'static str;

    fn add_history(&self, item: HistoryItem<Self::R>) -> EResult;

    fn get_history<'a>(&self) -> Vec<HistoryItem<Self::R>>;
}

pub enum PluginEnum {
    App(AppPlugin),
    #[cfg(feature = "calc")]
    Calc(CalcPlugin),
    #[cfg(feature = "clip")]
    Clip(ClipPlugin),
    #[cfg(feature = "wmwin")]
    Win(WinPlugin),
}

macro_rules! pimpl {
    ($self:expr, $($tt:tt)*) => {{
        match $self {
            #[cfg(feature = "calc")]
            PluginEnum::Calc(r) => {r.$($tt)*}
            PluginEnum::App(r) => {r.$($tt)*}
            #[cfg(feature = "wmwin")]
            PluginEnum::Win(r) => {r.$($tt)*}
            #[cfg(feature = "clip")]
            PluginEnum::Clip(r) => {r.$($tt)*}
        }
    }};
}

#[macro_export]
macro_rules! impl_history {
    () => {
        fn add_history(&self, item: HistoryItem<Self::R>) -> chin_tools::EResult {
            CONNECTION.with_borrow(|conn| {
                self.history
                    .add_history(item, HistoryDb::new(conn.as_ref()))
            })
        }

        fn get_history<'a>(&self) -> Vec<HistoryItem<Self::R>> {
            self.history
                .histories
                .load()
                .iter()
                .map(|(_, t)| t.clone())
                .collect()
        }
    };
}

impl Plugin for PluginEnum {
    type R = PluginResultEnum;

    type T = PluginReqEnum;

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(Self::R, i32)>> {
        Ok(pimpl!(
            self,
            handle_input(user_input)?
                .into_iter()
                .map(|(pr, s)| (pr.to_enum(), s))
                .collect()
        ))
    }

    fn get_type_id(&self) -> &'static str {
        pimpl!(self, get_type_id())
    }

    fn add_history(&self, item: HistoryItem<Self::R>) -> EResult {
        match self {
            PluginEnum::App(plugin) => {
                if let PluginResultEnum::App(r) = item.body {
                    let _ =plugin.add_history(HistoryItem {
                        body: r,
                        id: item.id,
                        plugin_type: item.plugin_type,
                        weight: item.weight,
                        update_time: item.update_time,
                    });
                }
            }
            PluginEnum::Calc(plugin) => {
                if let PluginResultEnum::Calc(r) = item.body {
                    let _ = plugin.add_history(HistoryItem {
                        body: r,
                        id: item.id,
                        plugin_type: item.plugin_type,
                        weight: item.weight,
                        update_time: item.update_time,
                    });
                }
            }
            PluginEnum::Win(plugin) => {
                if let PluginResultEnum::Win(r) = item.body {
                    let _ = plugin.add_history(HistoryItem {
                        body: r,
                        id: item.id,
                        plugin_type: item.plugin_type,
                        weight: item.weight,
                        update_time: item.update_time,
                    });
                }
            }
        }

        Ok(())
    }

    fn get_history<'a>(&self) -> Vec<HistoryItem<Self::R>> {
        match self {
            PluginEnum::App(p) => p.get_history().into_iter().map(|e| e.into()).collect(),
            PluginEnum::Calc(p) => p.get_history().into_iter().map(|e| e.into()).collect(),
            PluginEnum::Win(p) => p.get_history().into_iter().map(|e| e.into()).collect(),
        }
    }
}

pub enum PluginReqEnum {
    App(AppReq),
    #[cfg(feature = "calc")]
    Calc(CalcReq),
    #[cfg(feature = "clip")]
    Clip(ClipReq),
    #[cfg(feature = "wmwin")]
    Win(WindowMsg),
}

pub trait PluginResult: Send + Sync + Clone + DeserializeOwned + Serialize {
    fn icon_name(&self) -> &str;

    fn name(&self) -> &str;

    fn extra(&self) -> Option<&str>;

    fn on_enter(&self);

    fn get_type_id(&self) -> &'static str;

    fn get_id(&self) -> &str;

    fn to_enum(self) -> PluginResultEnum;
}

#[derive(Clone, Deserialize, Serialize)]
pub enum PluginResultEnum {
    App(AppResult),
    #[cfg(feature = "calc")]
    Calc(CalcResult),
    #[cfg(feature = "wmwin")]
    Win(WinResult),
    #[cfg(feature = "clip")]
    Clip(ClipResult),
}

macro_rules! plugin_box {
    ($inner:tt, $one:tt) => {
        impl Into<PluginResultEnum> for $inner {
            fn into(self) -> PluginResultEnum {
                PluginResultEnum::$one(self)
            }
        }

        impl Into<HistoryItem<PluginResultEnum>> for HistoryItem<$inner> {
            fn into(self) -> HistoryItem<PluginResultEnum> {
                HistoryItem {
                    body: self.body.into(),
                    id: self.id,
                    plugin_type: self.plugin_type,
                    weight: self.weight,
                    update_time: self.update_time,
                }
            }
        }
    };
}

plugin_box!(AppResult, App);
plugin_box!(CalcResult, Calc);
plugin_box!(WinResult, Win);

#[derive(Clone)]
pub struct PRWrapper {
    pub body: PluginResultEnum,
    pub score: i32,
}

impl Deref for PRWrapper {
    type Target = PluginResultEnum;

    fn deref(&self) -> &Self::Target {
        &self.body
    }
}

impl<T> Into<PRWrapper> for (T, i32)
where
    T: PluginResult,
{
    fn into(self) -> PRWrapper {
        PRWrapper {
            body: self.0.to_enum(),
            score: self.1,
        }
    }
}

macro_rules! primpl {
    ($self:expr, $method:ident) => {
        match $self {
            PluginResultEnum::App(r) => r.$method(),
            #[cfg(feature = "calc")]
            PluginResultEnum::Calc(r) => r.$method(),
            #[cfg(feature = "wmwin")]
            PluginResultEnum::Win(r) => r.$method(),
            #[cfg(feature = "clip")]
            PluginResultEnum::Clip(r) => r.$method(),
        }
    };
}

impl PluginResult for PluginResultEnum {
    fn icon_name(&self) -> &str {
        primpl!(self, icon_name)
    }

    fn name(&self) -> &str {
        primpl!(self, name)
    }

    fn extra(&self) -> Option<&str> {
        primpl!(self, extra)
    }

    fn on_enter(&self) {
        primpl!(self, on_enter)
    }

    fn get_type_id(&self) -> &'static str {
        primpl!(self, get_type_id)
    }

    fn get_id(&self) -> &str {
        primpl!(self, get_id)
    }

    fn to_enum(self) -> PluginResultEnum {
        primpl!(self, to_enum)
    }
}
