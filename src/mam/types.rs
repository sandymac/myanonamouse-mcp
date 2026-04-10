// Copyright (c) 2026 Sandy McArthur, Jr.
// SPDX-License-Identifier: MIT

use serde::Deserialize;
use serde_json::Value;

/// Request parameters for the MAM search API (`/tor/js/loadSearchJSONbasic.php`).
/// All fields are optional except `text`; omitted fields use MAM defaults.
#[derive(Debug, Default, Clone)]
pub(crate) struct SearchRequest {
    // -- tor object fields --
    /// Search query text. Empty string searches everything.
    pub(crate) text: String,
    /// Fields to search in (title, author, narrator, series, description, tags, filenames, fileTypes).
    /// None = default (title + author + narrator + series).
    pub(crate) srch_in: Option<Vec<String>>,
    /// Torrent filter: all, active, inactive, fl, fl-VIP, VIP, nVIP, nMeta.
    pub(crate) search_type: Option<String>,
    /// Main category IDs (13=AudioBooks, 14=E-Books, 15=Musicology, 16=Radio).
    pub(crate) main_cat: Vec<u32>,
    /// Subcategory IDs.
    pub(crate) cat: Vec<u32>,
    /// Language IDs.
    pub(crate) browse_lang: Vec<u32>,
    /// Sort order (e.g. "dateDesc", "snatchedDesc", "default").
    pub(crate) sort_type: Option<String>,
    /// Pagination offset.
    pub(crate) start_number: u32,
    /// Start date filter (unix timestamp string or YYYY-MM-DD).
    pub(crate) start_date: Option<String>,
    /// End date filter (unix timestamp string or YYYY-MM-DD).
    pub(crate) end_date: Option<String>,
    /// Minimum seeders.
    pub(crate) min_seeders: Option<i32>,
    /// browseFlagsHideVsShow value. Defaults to "0".
    pub(crate) browse_flags_hide_vs_show: Option<String>,
    // -- top-level fields --
    /// Results per page (5-1000).
    pub(crate) perpage: Option<u32>,
    /// Include download hash in results. Defaults to true.
    pub(crate) dl_link: Option<bool>,
    /// Include full description in results.
    pub(crate) description: Option<bool>,
    /// Include ISBN in results.
    pub(crate) isbn: Option<bool>,
    /// Include media info in results.
    pub(crate) media_info: Option<bool>,
    /// Limit to user's snatched torrents.
    pub(crate) my_snatched: Option<bool>,
    /// Include thumbnail URLs.
    pub(crate) thumbnail: Option<bool>,
    /// Include bookmark status.
    pub(crate) bookmarks: Option<bool>,
}

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
