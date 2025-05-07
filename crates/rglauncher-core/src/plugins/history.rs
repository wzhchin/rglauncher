use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use chin_tools::{AResult, AnyhowContext, EResult, SharedStr};
use chrono::{NaiveDateTime, Utc};
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};
use tracing::info;

use super::{PluginResult, PluginResultEnum};

#[derive(Clone)]
pub struct HistoryItem<V: Clone> {
    pub id: SharedStr,
    pub plugin_type: String,
    pub body: V,
    pub weight: f64,
    pub update_time: NaiveDateTime,
}

#[derive(Default)]
pub struct HistoryCache<V: Clone> {
    pub histories: ArcSwap<HashMap<SharedStr, HistoryItem<V>>>,
}

impl<V: Clone + Serialize> HistoryCache<V> {
    pub fn new(histories: Vec<HistoryItem<V>>) -> Self {
        info!("history: {}", histories.len());
        Self {
            histories: ArcSwap::new(Arc::new(
                histories.into_iter().map(|e| (e.id.clone(), e)).collect(),
            )),
        }
    }

    pub fn add_history<'a>(&self, item: HistoryItem<V>, ho: HistoryDb<'a>) -> EResult {
        let mut histories: HashMap<SharedStr, HistoryItem<V>> = self
            .histories
            .load()
            .as_ref()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let oi = histories.get(&item.id);

        let item = HistoryItem {
            weight: if let Some(i) = oi {
                i.weight
                    + 2_f64.powf(
                        Utc::now()
                            .naive_utc()
                            .signed_duration_since(i.update_time)
                            .num_days()
                            .abs() as f64
                            * (-1.),
                    )
            } else {
                item.weight
            },
            ..item
        };
        let _ = ho.update_or_insert(&item);
        histories.insert(item.id.clone(), item);

        self.histories.store(Arc::new(histories));

        Ok(())
    }

    pub fn remove_unvalid<'a, F>(&self, retain: F, ho: HistoryDb<'a>) -> EResult
    where
        F: Fn(&SharedStr, &HistoryItem<V>) -> bool,
    {
        let mut to_remove = vec![];
        let mut retains = HashMap::new();
        self.histories.load().as_ref().iter().for_each(|e| {
            if retain(e.0, e.1) {
                retains.insert(e.0.clone(), e.1.clone());
            } else {
                to_remove.push(e.0.clone());
            }
        });

        let _ = ho.invalid_items(to_remove.as_slice());
        info!("remove: {:?} {:?}", to_remove, retains.len());
        self.histories.store(Arc::new(retains));

        Ok(())
    }
}

pub struct HistoryDb<'a> {
    conn: Option<&'a Connection>,
}

impl<'a> HistoryDb<'a> {
    pub fn new(conn: Option<&'a Connection>) -> Self {
        HistoryDb { conn }
    }

    pub fn get_id(pr: &PluginResultEnum) -> SharedStr {
        return (pr.get_type_id().to_owned() + pr.get_id()).into();
    }

    pub fn fetch_histories<V: Clone + DeserializeOwned>(
        &self,
        ptype: &str,
    ) -> AResult<Vec<HistoryItem<V>>> {
        let mut stmt = self.conn
            .context("conn is none")?.prepare(
                "select id, plugin_type, body_json, weight, update_time from result_history where plugin_type = ? and valid = 1 order by update_time desc limit 100",
            )?;

        let result = stmt
            .query_map(params![ptype], |row| {
                let id: String = row.get("id")?;
                let plugin_type: String = row.get("plugin_type")?;
                let body_json: String = row.get("body_json")?;
                let weight: f64 = row.get("weight")?;
                let update_time = row.get("update_time")?;

                let body: V = serde_json::from_str(&body_json)
                    .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;

                Ok(HistoryItem {
                    plugin_type,
                    id: id.into(),
                    body,
                    weight,
                    update_time,
                })
            })?
            .collect::<rusqlite::Result<Vec<HistoryItem<V>>>>()?;

        Ok(result.into_iter().rev().collect())
    }

    pub fn invalid_items<T: AsRef<str>>(&self, ids: &[T]) -> EResult {
        if let Some(conn) = self.conn.as_ref() {
            let phs = (0..ids.len()).map(|_| "?").collect::<Vec<_>>().join(",");
            let mut stmt = conn.prepare(
                format!("update result_history set valid = 0 where id in ({})", phs).as_str(),
            )?;

            let ids: Vec<&str> = ids.iter().map(|e| e.as_ref()).collect();
            let ids: Vec<&dyn rusqlite::ToSql> =
                ids.iter().map(|e| e as &dyn rusqlite::ToSql).collect();
            stmt.execute::<&[&dyn rusqlite::ToSql]>(ids.as_slice())?;
        }

        Ok(())
    }

    pub fn update_or_insert<V: Clone + Serialize>(&self, result: &HistoryItem<V>) -> AResult<()> {
        if let Some(conn) = self.conn.as_ref() {
            let mut stmt = conn.prepare(
                "insert or replace into result_history \
            (id, plugin_type, body_json, weight, update_time) values (?, ?, ?, ?, ?)",
            )?;

            let body_json = serde_json::to_string(&result.body)?;

            stmt.insert(params![
                &result.id.as_str(),
                &result.plugin_type,
                &body_json,
                &result.weight,
                &result.update_time
            ])?;
        }

        Ok(())
    }

    pub fn try_create_table(&self) -> AResult<()> {
        self.conn
            .context("conn is none")?
            .prepare(
                "CREATE TABLE IF NOT EXISTS result_history (
id TEXT,
plugin_type TEXT,
body_json TEXT,
update_time TIMESTAMP,
weight REAL,
valid int(1) default 1,
PRIMARY KEY (id)
)",
            )?
            .execute([])?;

        Ok(())
    }
}
