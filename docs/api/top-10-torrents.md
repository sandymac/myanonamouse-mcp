# Top 10 Torrents API

> **Undocumented API** -- reverse-engineered from observing the Top 10 page served at `https://www.myanonamouse.net/stats/top10Tor.php`.
> Not listed on the official API endpoint page (`/api/list.php`).
> Presumably subject to change without notice, but the underlying search and stats endpoints it relies on are stable and well-established.

The Top 10 feature shows the most-snatched (most-downloaded) torrents on
MyAnonamouse, with optional category and time period filtering. It operates in
two modes: **Live** (current data via the standard search endpoint) and
**Historical** (archived snapshots via a dedicated endpoint).

---

## Live Mode

Live mode reuses the standard torrent search endpoint. The Top 10 is simply a
search with no query text, sorted by snatch count descending, limited to 10
results.

### Endpoint

```
GET or POST /tor/js/loadSearchJSONbasic.php
```

Accepts input as `application/json`, `application/x-www-form-urlencoded`,
`multipart/form-data`, or query string parameters.

### Parameters

All parameters are the same as the standard torrent search API. The Top 10
page sets them as follows:

| Parameter | Value | Notes |
|---|---|---|
| `tor[sortType]` | `snatchedDesc` | Sort by times snatched, descending |
| `perpage` | `10` | Return 10 results |
| `tor[text]` | `""` | Empty string -- no text search |
| `tor[searchType]` | `all` | Include all torrent types |
| `tor[searchIn]` | `torrents` | Search torrents (not requests) |
| `tor[startNumber]` | `0` | No pagination offset |
| `tor[browseFlagsHideVsShow]` | `0` | Default flag visibility |
| `dlLink` | `true` | Include download link hash in response |
| `bookmarks` | `true` | Include bookmark status in response |

#### Category filtering

| Parameter | Type | Notes |
|---|---|---|
| `tor[cat][]` | integer (repeatable) | Subcategory IDs. Use `0` for all. Multiple values are OR-combined. |
| `tor[main_cat][]` | integer (repeatable) | Parent category IDs: 13=AudioBooks, 14=E-Books, 15=Musicology, 16=Radio |

When no category filter is applied, the page sends `tor[cat][]=0`.

When specific subcategories are selected, each selected ID is sent as a
separate `tor[cat][]=ID` parameter. See the
[Search Form HTML fragment](../../api-docs/Search-Form-HTML-fragment.html)
for the full subcategory ID table.

#### Time period filtering

| Parameter | Type | Notes |
|---|---|---|
| `tor[startDate]` | unix timestamp or empty string | Start of time range (inclusive) |
| `tor[endDate]` | unix timestamp or empty string | End of time range (exclusive) |

The page computes timestamps relative to the current time:

| Period | startDate | endDate |
|---|---|---|
| All Time | `""` | `""` |
| Past Year | `now - 365 * 86400` | `now` |
| Past Month | `now - 30 * 86400` | `now` |
| Past Week | `now - 7 * 86400` | `now` |
| Past Day | `now - 86400` | `now` |

Where `now` is `Math.round(new Date() / 1000)` (current unix timestamp in
seconds).

### Example request (JSON POST, all time, all categories)

```json
{
    "tor": {
        "text": "",
        "srchIn": ["title", "description", "tags", "author", "narrator", "series", "fileTypes", "filenames"],
        "searchType": "all",
        "searchIn": "torrents",
        "cat": [0],
        "browseFlagsHideVsShow": "0",
        "startDate": "",
        "endDate": "",
        "sortType": "snatchedDesc",
        "startNumber": 0
    },
    "dlLink": "true",
    "bookmarks": "true",
    "perpage": 10
}
```

### Example request (GET, past month, all categories)

```
/tor/js/loadSearchJSONbasic.php?dlLink=true&bookmarks=true&perpage=10
  &tor%5Btext%5D=
  &tor%5BsrchIn%5D%5Btitle%5D=true
  &tor%5BsrchIn%5D%5Bdescription%5D=true
  &tor%5BsrchIn%5D%5Btags%5D=true
  &tor%5BsrchIn%5D%5Bauthor%5D=true
  &tor%5BsrchIn%5D%5Bnarrator%5D=true
  &tor%5BsrchIn%5D%5Bseries%5D=true
  &tor%5BsrchIn%5D%5BfileTypes%5D=true
  &tor%5BsrchIn%5D%5Bfilenames%5D=true
  &tor%5BsearchType%5D=all
  &tor%5BsearchIn%5D=torrents
  &tor%5Bcat%5D%5B%5D=0
  &tor%5BbrowseFlagsHideVsShow%5D=0
  &tor%5BstartDate%5D=1741478401
  &tor%5BendDate%5D=1744070401
  &tor%5BsortType%5D=snatchedDesc
  &tor%5BstartNumber%5D=0
```

### Response

Standard torrent search JSON response. Key fields per torrent:

| Field | Type | Description |
|---|---|---|
| `id` | int | Torrent ID (viewable at `/t/{id}`) |
| `title` | string | Torrent title |
| `times_completed` | int | Number of times snatched (the ranking metric) |
| `catname` | string | Category name (e.g. "Audiobooks - Science Fiction") |
| `size` | string | File size |
| `seeders` | int | Current seeders |
| `leechers` | int | Current leechers |
| `author_info` | string | JSON object of `{id: name}` pairs |
| `narrator_info` | string | JSON object of `{id: name}` pairs |
| `series_info` | string | JSON object of `{id: [name, position]}` pairs |
| `tags` | string | Space-separated tag list |
| `free` | int | 1 if freeleech, 0 otherwise |
| `vip` | int | 1 if VIP, 0 otherwise |
| `added` | string | UTC datetime when uploaded |
| `dl` | string | User-specific download hash (prepend `/tor/download.php/`) |

Wrapper fields:

| Field | Type | Description |
|---|---|---|
| `data` | array | Array of torrent objects |
| `total` | int | Number of results loaded |
| `found` | int | Total matching results |

Empty result sets return `{"error": "Nothing returned, out of 0"}` instead of
the normal structure.

---

## Historical Mode

When viewing Top 10 for a specific past year, month, or week, the page uses a
dedicated endpoint that serves pre-computed snapshots.

### Endpoint

```
GET /stats/top10TorJSON.php
```

### Parameters

| Parameter | Type | Required | Description |
|---|---|---|---|
| `dlLink` | boolean | No | Include download link hash |
| `bookmarks` | boolean | No | Include bookmark status |
| `year` | int | Yes | Four-digit year (e.g. `2024`) |
| `month` | int | No | Month number 1--12. Omit for full year. |
| `week` | int | No | ISO week number. Omit for full month/year. |
| `categories` | string | No | Comma-separated subcategory IDs (e.g. `41,63`). Omit for all. |

### Example requests

```
# Full year 2024
/stats/top10TorJSON.php?dlLink=true&bookmarks=true&year=2024

# June 2024
/stats/top10TorJSON.php?dlLink=true&bookmarks=true&year=2024&month=6

# Week 35 of 2024
/stats/top10TorJSON.php?dlLink=true&bookmarks=true&year=2024&week=35

# Year 2024, only Science Fiction audiobooks and ebooks
/stats/top10TorJSON.php?dlLink=true&bookmarks=true&year=2024&categories=47,69
```

### Response

Same format as the standard torrent search response (see Live Mode above).

### Special case: cross-year weeks

When January includes a week that started in the previous December (e.g. week
53), the JavaScript adjusts by decrementing the year:

```javascript
if (l.month === 1 && l.week > 45) {
    l.year--;
}
```

---

## Available Time Periods

The page fetches available historical periods from a separate endpoint to
populate the year/month/week dropdown menus.

### Endpoint

```
GET /stats/js/top10TorAvailable.php
```

(Also available via `GET https://cdn.myanonamouse.net/stats/js/top10TorAvailable.php`)

### Response

A JSON object keyed by year. Each year contains:
- `"all"`: boolean -- whether an aggregate "all" view is available for that year
- Month keys `"0"` through `"11"` (0 = January, 11 = December), each containing:
  - `"all"`: boolean -- whether an aggregate "all" view is available for that month
  - Week number keys (e.g. `"1"`, `"35"`), each an array of
    `[start_timestamp, end_timestamp, year]`

### Example (abbreviated)

```json
{
    "2024": {
        "all": true,
        "0": {
            "all": true,
            "01": [1704067201, 1704067201, 2024],
            "02": [1704672002, 1704672002, 2024],
            "03": [1705276801, 1705276801, 2024],
            "04": [1705881602, 1705881602, 2024],
            "05": [1706486402, 1706486402, 2024]
        },
        "1": {
            "all": true,
            "05": [1706486402, 1706486402, 2024],
            "06": [1707091201, 1707091201, 2024],
            "07": [1707696001, 1707696001, 2024],
            "08": [1708300802, 1708300802, 2024],
            "09": [1708905602, 1708905602, 2024]
        }
    },
    "2025": {
        "all": true,
        "0": {
            "all": true,
            "1": [1735516801, 1736035201, 2025],
            "2": [1736121601, 1736640001, 2025]
        }
    }
}
```

Note: Weeks can span month boundaries, so a week may appear under two
consecutive months. The timestamps define the exact start/end of each week
window.

---

## Client-Side Behavior

The Top 10 page stores user preferences in `localStorage`:

| Key | Default | Description |
|---|---|---|
| `lastHistorical` | `{year: 0, month: 0, week: 0}` | Selected time period. `year: 0` means live mode; `month` values 0--4 map to All Time, Past Year, Past Month, Past Week, Past Day in live mode. |
| `top10Categories` | `{All: true, subcategories: []}` | Selected category filter. Parent category names as keys with boolean values; `subcategories` is a sparse array keyed by subcategory ID. |
| `top10FilterVisible` | `false` | Whether the category filter panel is expanded |

The page URL is updated via `pushState` to reflect the current filter state,
allowing direct linking to a specific view (e.g.
`/stats/top10Tor.php?lastHistorical={"year":2024,"month":6,"week":0}`).

---

## Source

Discovered 2026-04-05 by inspecting:
- Page: `https://www.myanonamouse.net/stats/top10Tor.php`
- JavaScript: `https://cdn.myanonamouse.net/1656044172/stats/js/top10Tor.js`
- Available periods: `https://cdn.myanonamouse.net/stats/js/top10TorAvailable.php`
- Network requests captured via browser DevTools
