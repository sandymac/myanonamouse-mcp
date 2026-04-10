// Copyright (c) 2026 Sandy McArthur, Jr.
// SPDX-License-Identifier: MIT

use serde::Deserialize;

use super::{enrich_error, BASE_URL};
use super::format::{
    format_bonus_history, format_search_response, format_torrent_detail, format_user_data,
};
use super::types::{BonusEntry, SearchRequest, SearchResponse, TorrentDetail, UserDataResponse};

pub(crate) async fn do_search(
    client: &reqwest::Client,
    request: &SearchRequest,
) -> Result<SearchResponse, String> {
    let srch_in_val = if let Some(ref fields) = request.srch_in {
        serde_json::json!(fields)
    } else {
        serde_json::json!(["title", "author", "narrator", "series"])
    };

    let mut tor = serde_json::json!({
        "text": request.text,
        "srchIn": srch_in_val,
        "searchType": request.search_type.as_deref().unwrap_or("all"),
        "searchIn": "torrents",
        "browseFlagsHideVsShow": request.browse_flags_hide_vs_show.as_deref().unwrap_or("0"),
        "startNumber": request.start_number,
    });
    if !request.main_cat.is_empty() {
        tor["main_cat"] = serde_json::json!(request.main_cat);
    }
    if !request.cat.is_empty() {
        tor["cat"] = serde_json::json!(request.cat);
    }
    // Omit browse_lang when empty — sending [] breaks the MAM search engine
    if !request.browse_lang.is_empty() {
        tor["browse_lang"] = serde_json::json!(request.browse_lang);
    }
    if let Some(ref sort) = request.sort_type {
        tor["sortType"] = serde_json::json!(sort);
    }
    if let Some(ref d) = request.start_date {
        tor["startDate"] = serde_json::json!(d);
    }
    if let Some(ref d) = request.end_date {
        tor["endDate"] = serde_json::json!(d);
    }
    if let Some(min) = request.min_seeders {
        tor["minSeeders"] = serde_json::json!(min);
    }

    let mut body = serde_json::json!({
        "tor": tor,
        "perpage": request.perpage.unwrap_or(25),
    });
    if request.dl_link.unwrap_or(true) {
        body["dlLink"] = serde_json::json!("true");
    }
    if request.description.unwrap_or(false) {
        body["description"] = serde_json::json!("");
    }
    if request.isbn.unwrap_or(false) {
        body["isbn"] = serde_json::json!("");
    }
    if request.media_info.unwrap_or(false) {
        body["mediaInfo"] = serde_json::json!("");
    }
    if request.my_snatched.unwrap_or(false) {
        body["my_snatched"] = serde_json::json!("");
    }
    if let Some(thumb) = request.thumbnail {
        body["thumbnail"] = serde_json::json!(if thumb { "true" } else { "false" });
    }
    if request.bookmarks.unwrap_or(false) {
        body["bookmarks"] = serde_json::json!("true");
    }

    let resp = client
        .post(format!("{BASE_URL}/tor/js/loadSearchJSONbasic.php"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(enrich_error(status.as_u16(), &text));
    }

    let text = resp.text().await.map_err(|e| format!("Failed to read search response: {e}"))?;

    // MAM returns {"error":"Nothing returned, out of 0"} for empty result sets
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        if v.get("data").is_none() {
            if let Some(msg) = v.get("error").and_then(|e| e.as_str()) {
                if msg.contains("Nothing returned") {
                    return Ok(SearchResponse { data: vec![], total: 0, found: 0 });
                }
                return Err(format!("Search error: {msg}"));
            }
        }
    }

    serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse search response: {e}\nBody: {text}"))
}

pub(crate) async fn get_user_data(
    client: &reqwest::Client,
    user_id: Option<u64>,
    include_notifications: bool,
) -> Result<String, String> {
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(id) = user_id {
        query.push(("id", id.to_string()));
    }
    if include_notifications {
        query.push(("notif", "true".to_string()));
    }

    let resp = client
        .get(format!("{BASE_URL}/jsonLoad.php"))
        .query(&query)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(enrich_error(status.as_u16(), &text));
    }

    let parsed: UserDataResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse user data response: {e}"))?;

    Ok(format_user_data(parsed))
}

pub(crate) async fn get_user_bonus_history(
    client: &reqwest::Client,
    bonus_types: Option<Vec<String>>,
    other_user_id: Option<u64>,
) -> Result<String, String> {
    let mut query: Vec<(&str, String)> = Vec::new();
    if let Some(types) = &bonus_types {
        query.push(("type", types.join(",")));
    }
    if let Some(uid) = other_user_id {
        query.push(("other_userid", uid.to_string()));
    }

    let resp = client
        .get(format!("{BASE_URL}/json/userBonusHistory.php"))
        .query(&query)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(enrich_error(status.as_u16(), &text));
    }

    let entries: Vec<BonusEntry> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse bonus history response: {e}"))?;

    Ok(format_bonus_history(entries))
}

pub(crate) async fn get_torrent_details(
    client: &reqwest::Client,
    id: Option<u64>,
    hash: Option<String>,
) -> Result<String, String> {
    let mut tor = serde_json::json!({
        "searchType": "all",
        "searchIn": "torrents",
        "startNumber": "0",
        "perpage": 1,
    });
    if let Some(id) = id {
        tor["id"] = serde_json::json!(id);
    }
    if let Some(hash) = &hash {
        tor["hash"] = serde_json::json!(hash);
    }

    let body = serde_json::json!({
        "tor": tor,
        "dlLink": "true",
        "description": "",
        "isbn": "",
        "mediaInfo": "",
    });

    let resp = client
        .post(format!("{BASE_URL}/tor/js/loadSearchJSONbasic.php"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(enrich_error(status.as_u16(), &text));
    }

    let body = resp.text().await.map_err(|e| format!("Failed to read response: {e}"))?;

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
        if v.get("data").is_none() {
            if let Some(msg) = v.get("error").and_then(|e| e.as_str()) {
                if msg.contains("Nothing returned") {
                    return Ok("No torrent found.".to_string());
                }
                return Err(format!("Lookup error: {msg}"));
            }
        }
    }

    #[derive(Deserialize)]
    struct DetailResponse {
        data: Vec<TorrentDetail>,
    }
    let parsed: DetailResponse = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse response: {e}\nBody: {body}"))?;

    match parsed.data.into_iter().next() {
        None => Ok("No torrent found.".to_string()),
        Some(t) => Ok(format_torrent_detail(t)),
    }
}

pub(crate) async fn get_top_10(
    client: &reqwest::Client,
    main_cat: Vec<u32>,
    cat: Vec<u32>,
    period: Option<&str>,
) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp();
    let (start_date, end_date, period_label) = match period {
        Some("day") => (Some((now - 86400).to_string()), Some(now.to_string()), "past day"),
        Some("week") => (Some((now - 7 * 86400).to_string()), Some(now.to_string()), "past week"),
        Some("month") => (Some((now - 30 * 86400).to_string()), Some(now.to_string()), "past month"),
        Some("year") => (Some((now - 365 * 86400).to_string()), Some(now.to_string()), "past year"),
        _ => (None, None, "all time"),
    };

    let cat = if cat.is_empty() && main_cat.is_empty() { vec![0] } else { cat };

    let request = SearchRequest {
        main_cat,
        cat,
        sort_type: Some("snatchedDesc".into()),
        start_date,
        end_date,
        perpage: Some(10),
        dl_link: Some(true),
        bookmarks: Some(true),
        ..Default::default()
    };

    let label = format!("Top 10 ({period_label})");
    let resp = do_search(client, &request).await?;
    Ok(format_search_response(resp, &label))
}

pub(crate) async fn update_seedbox_ip(client: &reqwest::Client) -> Result<String, String> {
    let resp = client
        .get(format!("{BASE_URL}/json/dynamicSeedbox.php"))
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(enrich_error(status.as_u16(), &text));
    }

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
        let success = v.get("Success").and_then(|s| s.as_bool()).unwrap_or(false);
        let msg = v
            .get("msg")
            .and_then(|m| m.as_str())
            .unwrap_or("(no message)");
        let ip = v.get("ip").and_then(|i| i.as_str()).unwrap_or("");
        let asn = v.get("ASN").and_then(|a| a.as_str()).unwrap_or_default();

        if !success {
            return Err(format!(
                "{msg}\n[Hint: This endpoint is rate-limited to once per hour.]"
            ));
        }

        let mut out = msg.to_string();
        if !ip.is_empty() {
            out.push_str(&format!("\nRegistered IP: {ip}"));
        }
        if !asn.is_empty() {
            out.push_str(&format!("\nASN: {asn}"));
        }
        return Ok(out);
    }

    Ok(text)
}
