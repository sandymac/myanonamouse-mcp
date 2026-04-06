// Copyright (c) 2026 Sandy McArthur, Jr.
// SPDX-License-Identifier: MIT

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub(crate) struct SearchResponse {
    pub(crate) data: Vec<TorrentResult>,
    pub(crate) total: u64,
    pub(crate) found: u64,
}

#[derive(Deserialize)]
pub(crate) struct TorrentResult {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) catname: Option<String>,
    pub(crate) size: Option<String>,
    pub(crate) author_info: Option<Value>,
    pub(crate) narrator_info: Option<Value>,
    pub(crate) series_info: Option<Value>,
    pub(crate) tags: Option<Value>,
    pub(crate) seeders: Option<u64>,
    pub(crate) leechers: Option<u64>,
    pub(crate) free: Option<u64>,
    pub(crate) vip: Option<u64>,
    pub(crate) added: Option<String>,
    pub(crate) dl: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct TorrentDetail {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) catname: Option<String>,
    pub(crate) lang_code: Option<String>,
    pub(crate) size: Option<String>,
    pub(crate) numfiles: Option<u64>,
    pub(crate) filetype: Option<String>,
    pub(crate) author_info: Option<Value>,
    pub(crate) narrator_info: Option<Value>,
    pub(crate) series_info: Option<Value>,
    pub(crate) tags: Option<Value>,
    pub(crate) description: Option<String>,
    pub(crate) isbn: Option<Value>, // API returns integer or string
    pub(crate) mediainfo: Option<String>,
    pub(crate) seeders: Option<u64>,
    pub(crate) leechers: Option<u64>,
    pub(crate) times_completed: Option<u64>,
    pub(crate) free: Option<u64>,
    pub(crate) vip: Option<u64>,
    pub(crate) added: Option<String>,
    pub(crate) dl: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct UserDataResponse {
    pub(crate) username: Option<String>,
    pub(crate) uid: Option<u64>,
    pub(crate) classname: Option<String>,
    pub(crate) downloaded: Option<String>,
    pub(crate) uploaded: Option<String>,
    pub(crate) ratio: Option<f64>,
    pub(crate) seedbonus: Option<u64>,
    pub(crate) wedges: Option<u64>,
    pub(crate) country_name: Option<String>,
    pub(crate) notifs: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct BonusEntry {
    pub(crate) timestamp: f64,
    pub(crate) amount: Value,
    #[serde(rename = "type")]
    pub(crate) bonus_type: String,
    pub(crate) tid: Option<Value>,
    pub(crate) title: Option<String>,
    pub(crate) other_userid: Option<Value>,
    pub(crate) other_name: Option<String>,
}
