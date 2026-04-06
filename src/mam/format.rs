// Copyright (c) 2026 Sandy McArthur, Jr.
// SPDX-License-Identifier: MIT

use serde_json::Value;

use super::types::{BonusEntry, SearchResponse, TorrentDetail, UserDataResponse};
use super::BASE_URL;

pub(crate) fn format_search_response(resp: SearchResponse, query: &str) -> String {
    if resp.data.is_empty() {
        return format!("No results found for \"{query}\".");
    }

    let mut out = format!(
        "Showing {} of {} result(s) for \"{}\"\n",
        resp.total, resp.found, query
    );

    for (i, t) in resp.data.iter().enumerate() {
        out.push_str(&format!("\n{}. {}\n", i + 1, t.title));

        if let Some(cat) = &t.catname {
            out.push_str(&format!("   Category:  {cat}\n"));
        }
        if let Some(size) = &t.size {
            out.push_str(&format!("   Size:      {size}\n"));
        }

        let authors = t
            .author_info
            .as_ref()
            .and_then(|v| v.as_str())
            .map(parse_name_map)
            .unwrap_or_default();
        if !authors.is_empty() {
            out.push_str(&format!("   Authors:   {}\n", authors.join(", ")));
        }

        let narrators = t
            .narrator_info
            .as_ref()
            .and_then(|v| v.as_str())
            .map(parse_name_map)
            .unwrap_or_default();
        if !narrators.is_empty() {
            out.push_str(&format!("   Narrators: {}\n", narrators.join(", ")));
        }

        let series = t
            .series_info
            .as_ref()
            .and_then(|v| v.as_str())
            .map(parse_series_map)
            .unwrap_or_default();
        if !series.is_empty() {
            out.push_str(&format!("   Series:    {}\n", series.join(", ")));
        }

        if let Some(tags) = t.tags.as_ref().and_then(|v| v.as_str()) {
            if !tags.is_empty() {
                out.push_str(&format!("   Tags:      {tags}\n"));
            }
        }

        if t.seeders.is_some() || t.leechers.is_some() {
            out.push_str(&format!(
                "   S/L:       {}/{}\n",
                t.seeders.map_or("-".to_string(), |n| n.to_string()),
                t.leechers.map_or("-".to_string(), |n| n.to_string()),
            ));
        }

        let is_free = t.free.unwrap_or(0) == 1;
        let is_vip = t.vip.unwrap_or(0) == 1;
        if is_free || is_vip {
            let flags: Vec<&str> = [is_free.then_some("Free"), is_vip.then_some("VIP")]
                .into_iter()
                .flatten()
                .collect();
            out.push_str(&format!("   Flags:     {}\n", flags.join(", ")));
        }

        if let Some(added) = &t.added {
            out.push_str(&format!("   Added:     {added}\n"));
        }

        out.push_str(&format!("   ID:        {}\n", t.id));

        if let Some(dl) = &t.dl {
            if !dl.is_empty() {
                out.push_str(&format!(
                    "   DL URL:    {BASE_URL}/tor/download.php/{dl}\n",
                ));
            }
        }
    }

    out
}

pub(crate) fn format_torrent_detail(t: TorrentDetail) -> String {
    let mut out = String::new();

    out.push_str(&format!("Title:       {}\n", t.title));
    out.push_str(&format!("ID:          {}\n", t.id));

    if let Some(cat) = &t.catname {
        out.push_str(&format!("Category:    {cat}\n"));
    }
    if let Some(lang) = &t.lang_code {
        out.push_str(&format!("Language:    {lang}\n"));
    }
    if let Some(size) = &t.size {
        out.push_str(&format!("Size:        {size}\n"));
    }
    if let Some(n) = t.numfiles {
        out.push_str(&format!("Files:       {n}\n"));
    }
    if let Some(ft) = &t.filetype {
        if !ft.is_empty() {
            out.push_str(&format!("File type:   {ft}\n"));
        }
    }

    let authors = t
        .author_info
        .as_ref()
        .and_then(|v| v.as_str())
        .map(parse_name_map)
        .unwrap_or_default();
    if !authors.is_empty() {
        out.push_str(&format!("Authors:     {}\n", authors.join(", ")));
    }

    let narrators = t
        .narrator_info
        .as_ref()
        .and_then(|v| v.as_str())
        .map(parse_name_map)
        .unwrap_or_default();
    if !narrators.is_empty() {
        out.push_str(&format!("Narrators:   {}\n", narrators.join(", ")));
    }

    let series = t
        .series_info
        .as_ref()
        .and_then(|v| v.as_str())
        .map(parse_series_map)
        .unwrap_or_default();
    if !series.is_empty() {
        out.push_str(&format!("Series:      {}\n", series.join(", ")));
    }

    if let Some(tags) = t.tags.as_ref().and_then(|v| v.as_str()) {
        if !tags.is_empty() {
            out.push_str(&format!("Tags:        {tags}\n"));
        }
    }
    if let Some(isbn) = &t.isbn {
        let s = value_as_str(isbn);
        if !s.is_empty() && s != "0" {
            out.push_str(&format!("ISBN:        {s}\n"));
        }
    }

    out.push_str(&format!(
        "Seeders:     {}\nLeechers:    {}\nSnatched:    {}\n",
        t.seeders.unwrap_or(0),
        t.leechers.unwrap_or(0),
        t.times_completed.unwrap_or(0),
    ));

    let is_free = t.free.unwrap_or(0) == 1;
    let is_vip = t.vip.unwrap_or(0) == 1;
    if is_free || is_vip {
        let flags: Vec<&str> = [is_free.then_some("Free"), is_vip.then_some("VIP")]
            .into_iter()
            .flatten()
            .collect();
        out.push_str(&format!("Flags:       {}\n", flags.join(", ")));
    }

    if let Some(added) = &t.added {
        out.push_str(&format!("Added:       {added}\n"));
    }

    if let Some(dl) = &t.dl {
        if !dl.is_empty() {
            out.push_str(&format!(
                "DL URL:      {BASE_URL}/tor/download.php/{dl}\n",
            ));
        }
    }

    if let Some(desc) = &t.description {
        if !desc.is_empty() {
            let md = htmd::convert(desc).unwrap_or_else(|_| desc.clone());
            out.push_str(&format!("\nDescription:\n{md}\n"));
        }
    }
    if let Some(mi) = &t.mediainfo {
        if !mi.is_empty() {
            out.push_str(&format!("\nMedia Info:\n{mi}\n"));
        }
    }

    out
}

pub(crate) fn format_user_data(resp: UserDataResponse) -> String {
    let mut out = String::new();

    if let Some(name) = &resp.username {
        out.push_str(&format!("Username:    {name}\n"));
    }
    if let Some(uid) = resp.uid {
        out.push_str(&format!("User ID:     {uid}\n"));
    }
    if let Some(class) = &resp.classname {
        out.push_str(&format!("Class:       {class}\n"));
    }
    if let Some(country) = &resp.country_name {
        out.push_str(&format!("Country:     {country}\n"));
    }
    if let Some(up) = &resp.uploaded {
        out.push_str(&format!("Uploaded:    {up}\n"));
    }
    if let Some(down) = &resp.downloaded {
        out.push_str(&format!("Downloaded:  {down}\n"));
    }
    if let Some(ratio) = resp.ratio {
        out.push_str(&format!("Ratio:       {ratio:.2}\n"));
    }
    if let Some(bonus) = resp.seedbonus {
        out.push_str(&format!("Seed bonus:  {bonus}\n"));
    }
    if let Some(wedges) = resp.wedges {
        out.push_str(&format!("Wedges:      {wedges}\n"));
    }

    if let Some(notifs) = &resp.notifs {
        let count = match notifs {
            Value::Array(arr) => arr.len(),
            Value::Object(map) => map.len(),
            _ => 0,
        };
        if count > 0 {
            out.push_str(&format!("Notifications: {count} unread\n"));
        }
    }

    if out.is_empty() {
        out.push_str("(no data returned)");
    }

    out
}

pub(crate) fn format_bonus_history(entries: Vec<BonusEntry>) -> String {
    if entries.is_empty() {
        return "No bonus history found.".to_string();
    }

    let mut out = format!("{} transaction(s):\n", entries.len());

    for entry in &entries {
        let secs = entry.timestamp as i64;
        let ts = chrono::DateTime::from_timestamp(secs, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| secs.to_string());
        let amount = value_as_i64(&entry.amount);
        let sign = if amount >= 0 { "+" } else { "" };

        out.push_str(&format!(
            "\n  [{ts}] {sign}{amount} — {btype}",
            btype = entry.bonus_type,
        ));

        match (&entry.title, &entry.tid) {
            (Some(title), _) if !title.is_empty() => {
                out.push_str(&format!("\n    Torrent: {title}"));
            }
            (_, Some(tid)) => {
                out.push_str(&format!("\n    Torrent ID: {}", value_as_str(tid)));
            }
            _ => {}
        }
        match (&entry.other_name, &entry.other_userid) {
            (Some(name), _) if !name.is_empty() => {
                out.push_str(&format!("\n    User: {name}"));
            }
            (_, Some(uid)) => {
                out.push_str(&format!("\n    User ID: {}", value_as_str(uid)));
            }
            _ => {}
        }
        out.push('\n');
    }

    out
}

pub(crate) fn format_categories() -> String {
    let mut out = String::new();
    out.push_str("MAIN CATEGORIES  (use with search_torrents main_cat=[])\n");
    out.push_str("  13  AudioBooks\n");
    out.push_str("  14  E-Books\n");
    out.push_str("  15  Musicology\n");
    out.push_str("  16  Radio\n");

    out.push_str("\nAUDIOBOOK SUBCATEGORIES  (use with search_torrents cat=[])\n");
    for (name, id) in [
        ("Action/Adventure", 39u32), ("Art", 49), ("Biographical", 50), ("Business", 83),
        ("Computer/Internet", 51), ("Crafts", 97), ("Crime/Thriller", 40), ("Fantasy", 41),
        ("Food", 106), ("General Fiction", 42), ("General Non-Fiction", 52),
        ("Historical Fiction", 98), ("History", 54), ("Home/Garden", 55), ("Horror", 43),
        ("Humor", 99), ("Instructional", 84), ("Juvenile", 44), ("Language", 56),
        ("Literary Classics", 45), ("Math/Science/Tech", 57), ("Medical", 85),
        ("Mystery", 87), ("Nature", 119), ("Philosophy", 88), ("Pol/Soc/Relig", 58),
        ("Recreation", 59), ("Romance", 46), ("Science Fiction", 47), ("Self-Help", 53),
        ("Travel/Adventure", 89), ("True Crime", 100), ("Urban Fantasy", 108),
        ("Western", 48), ("Young Adult", 111),
    ] {
        out.push_str(&format!("  {id:>4}  {name}\n"));
    }

    out.push_str("\nEBOOK SUBCATEGORIES  (use with search_torrents cat=[])\n");
    for (name, id) in [
        ("Action/Adventure", 60u32), ("Art", 71), ("Biographical", 72), ("Business", 90),
        ("Comics/Graphic Novels", 61), ("Computer/Internet", 73), ("Crafts", 101),
        ("Crime/Thriller", 62), ("Fantasy", 63), ("Food", 107), ("General Fiction", 64),
        ("General Non-Fiction", 74), ("Historical Fiction", 102), ("History", 76),
        ("Home/Garden", 77), ("Horror", 65), ("Humor", 103), ("Illusion/Magic", 115),
        ("Instructional", 91), ("Juvenile", 66), ("Language", 78), ("Literary Classics", 67),
        ("Magazines/Newspapers", 79), ("Math/Science/Tech", 80), ("Medical", 92),
        ("Mixed Collections", 118), ("Mystery", 94), ("Nature", 120), ("Philosophy", 95),
        ("Pol/Soc/Relig", 81), ("Recreation", 82), ("Romance", 68), ("Science Fiction", 69),
        ("Self-Help", 75), ("Travel/Adventure", 96), ("True Crime", 104),
        ("Urban Fantasy", 109), ("Western", 70), ("Young Adult", 112),
    ] {
        out.push_str(&format!("  {id:>4}  {name}\n"));
    }

    out.push_str("\nMUSICOLOGY SUBCATEGORIES  (use with search_torrents cat=[])\n");
    for (name, id) in [
        ("Art", 49u32), ("Guitar/Bass Tabs", 19), ("Individual Sheet", 20),
        ("Individual Sheet MP3", 24), ("Instructional Media", 22),
        ("Lick Library LTP/Jam", 113), ("Lick Library Techniques", 114),
        ("Music Complete Editions", 17), ("Music Book", 26), ("Music Book MP3", 27),
        ("Sheet Collection", 30), ("Sheet Collection MP3", 31),
        ("Instructional Book with Video", 126),
    ] {
        out.push_str(&format!("  {id:>4}  {name}\n"));
    }

    out.push_str("\nRADIO SUBCATEGORIES  (use with search_torrents cat=[])\n");
    for (name, id) in [
        ("Comedy", 127u32), ("Factual/Documentary", 128), ("Drama", 130), ("Reading", 132),
    ] {
        out.push_str(&format!("  {id:>4}  {name}\n"));
    }

    out
}

pub(crate) fn value_as_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn value_as_i64(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// Parse a JSON-encoded map of `{ "id": "name" }` into a sorted list of names.
pub(crate) fn parse_name_map(json_str: &str) -> Vec<String> {
    let Ok(map) = serde_json::from_str::<serde_json::Map<String, Value>>(json_str) else {
        return vec![];
    };
    let mut names: Vec<String> = map
        .values()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    names.sort();
    names
}

/// Parse a JSON-encoded map of `{ "id": ["series name", "pos_str", pos_float] }` into display strings.
/// pos_float of -1.0 means no position specified.
pub(crate) fn parse_series_map(json_str: &str) -> Vec<String> {
    let Ok(map) = serde_json::from_str::<serde_json::Map<String, Value>>(json_str) else {
        return vec![];
    };
    let mut series: Vec<String> = map
        .values()
        .filter_map(|v| {
            let arr = v.as_array()?;
            let name = arr.first()?.as_str()?;
            let pos_str = arr.get(1).and_then(|p| p.as_str()).unwrap_or("");
            let pos_float = arr.get(2).and_then(|p| p.as_f64()).unwrap_or(-1.0);
            let pos = if !pos_str.is_empty() {
                pos_str.to_string()
            } else if pos_float >= 0.0 {
                pos_float.to_string()
            } else {
                String::new()
            };
            if pos.is_empty() {
                Some(name.to_string())
            } else {
                Some(format!("{name} [{pos}]"))
            }
        })
        .collect();
    series.sort();
    series
}
