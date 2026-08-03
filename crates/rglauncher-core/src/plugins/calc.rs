use crate::dispatcher::CONNECTION;
use crate::impl_history;
use crate::plugins::history::{HistoryDb, HistoryItem};
use crate::plugins::{Plugin, PluginResult};
use crate::userinput::UserInput;

use crate::util::score_utils;
use chin_tools::{AResult, SharedStr};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use tracing::info;

use super::history::HistoryCache;

pub const TYPE_ID: &str = "calc";

#[derive(Clone)]
pub struct CalcReq {}

#[derive(Clone, Deserialize, Serialize)]
pub struct CalcResult {
    pub formula: SharedStr,
    pub result: SharedStr,
}

impl PluginResult for CalcResult {
    fn icon_name(&self) -> &str {
        "calc"
    }

    fn name(&self) -> &str {
        self.formula.as_str()
    }

    fn extra(&self) -> Option<&str> {
        None
    }

    fn on_enter(&self) {}

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    fn get_id(&self) -> Cow<'_, str> {
        let hash = blake3::hash(self.formula.as_bytes()).to_hex();
        Cow::Owned(format!("{}{}", TYPE_ID, hash))
    }

    fn to_enum(self) -> super::PluginResultEnum {
        super::PluginResultEnum::Calc(self)
    }
}

pub struct CalcPlugin {
    history: HistoryCache<CalcResult>
}

impl CalcPlugin {
    pub fn new() -> AResult<Self> {
        info!("Creating Calc Plugin");

        let histories: Vec<HistoryItem<CalcResult>> =
        CONNECTION.with_borrow(|e| HistoryDb::new(e.as_ref()).fetch_histories(TYPE_ID))?;

        Ok(CalcPlugin {
            history: HistoryCache::new(histories)
        })
    }
}

impl Plugin for CalcPlugin {
    type R = CalcResult;

    type T = CalcReq;

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(CalcResult, i32)>> {
        let mut result = Vec::new();

        if user_input.input.is_empty() {
            // Surface recent calculations when the field is empty.
            for item in self.get_history() {
                let boost = (item.weight * 20.0) as i64;
                result.push((item.body, score_utils::high(boost)));
            }
            return Ok(result);
        }

        match meval::eval_str(user_input.input.as_str()) {
            Ok(res) => {
                result.push((
                    CalcResult {
                        formula: user_input.input.clone(),
                        result: res.to_string().into(),
                    },
                    score_utils::highest(0),
                ));
            }
            Err(_) => {}
        }

        Ok(result)
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    impl_history!();
}
