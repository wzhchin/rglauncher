use chin_tools::AResult;
use fuzzy_matcher::skim::SkimMatcherV2;
use mdict::mdx_utils::{self, MDictLookup};

use crate::config::DictConfig;
use crate::plugins::{Plugin, PluginResult};
use crate::userinput::UserInput;
use crate::util::score_utils;

pub const TYPE_ID: &str = "dict";

#[derive(Clone)]
pub enum DictMsg {}

#[derive(Clone)]
pub struct DictResult {
    pub word: String,
    pub html: String,
    pub dict: String,
    id: String,
}

impl PluginResult for DictResult {
    fn icon_name(&self) -> &str {
        "dictionary"
    }

    fn name(&self) -> &str {
        self.word.as_str()
    }

    fn extra(&self) -> Option<&str> {
        Some(self.dict.as_str())
    }

    fn on_enter(&self) {}

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }

    fn get_id(&self) -> &str {
        self.id.as_str()
    }

    fn to_enum(self) -> super::PluginResultEnum {
        super::PluginResultEnum::MDict(self)
    }
}

pub struct DictPlugin {
    mdxes: Vec<mdx_utils::MDictMemIndex>,
    matcher: SkimMatcherV2,
}

impl DictPlugin {
    pub fn new(dict_config: Option<&DictConfig>) -> AResult<Self> {
        let dir = dict_config
            .map(|e| e.dir_path.as_str())
            .context("missing dict config!")?;

        let filepaths = crate::util::fs_utils::walk_dir(
            dir,
            Some(|p: &str| p.to_lowercase().as_str().ends_with("mdx")),
        )?;

        let mdxes = filepaths
            .into_iter()
            .filter_map(|dr| {
                let p = dr.path();

                match mdx_utils::MDictMemIndex::new(p) {
                    Ok(mdx) => Some(mdx),
                    Err(_) => None,
                }
            })
            .collect();

        Ok(DictPlugin {
            mdxes,
            matcher: SkimMatcherV2::default(),
        })
    }

    pub fn seek(&self, word: &str) -> Vec<DictResult> {
        self.mdxes
            .iter()
            .filter_map(|mdx| {
                mdx.lookup_word(word)
                    .map(|explain| DictResult {
                        word: word.to_string(),
                        html: explain,
                        dict: mdx.name.to_string(),
                        id: format!("{}@{}", mdx.name.as_str(), word),
                    })
                    .ok()
            })
            .collect()
    }

    fn recur_seek(&self, word: &str) -> Vec<DictResult> {
        let w = word.trim();
        let seek_res = self.seek(w);

        let mut res: Vec<DictResult> = vec![];
        for item in seek_res {
            if item.html.starts_with("@@@LINK=") {
                let w2 = item.html.replace("\r\n\0", "").replace("@@@LINK=", "");
                let r = self.recur_seek(w2.as_str());
                res.extend(r);
            } else {
                res.push(item);
            }
        }

        res
    }
}

impl Plugin for DictPlugin {
    type R = DictResult;

    type T = DictMsg;

    fn handle_input(&self, user_input: &UserInput) -> AResult<Vec<(DictResult, i32)>> {
        if !user_input.input.is_empty() {
            let res: Vec<(DictResult, i32)> = self
                .recur_seek(user_input.input.as_str())
                .iter()
                .map(|e| (e.to_owned(), score_utils::highest(10)))
                .collect();
            return Ok(res);
        }

        Ok(vec![])
    }

    fn get_type_id(&self) -> &'static str {
        &TYPE_ID
    }
}
