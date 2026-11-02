//! Connectors per bron. Alle URL's zijn instelbaar (`config/world.toml`), zodat tests een
//! lokale nepserver gebruiken.

use quick_xml::Reader;
use quick_xml::events::Event as X;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSpec {
    pub name: String,
    /// gdelt | rss | wikipedia_events | wikidata | reliefweb | usgs | eonet | worldbank | eurostat | cbs | arxiv | hackernews
    pub kind: String,
    pub url: String,
    #[serde(default = "sixty")]
    pub interval_min: u64,
    /// Land van de bron (voor bias) en richting (publiek, links, rechts, staats, …).
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Bronspecifiek (bv. query, indicator, land, tabel, categorie, limiet).
    #[serde(default)]
    pub params: toml::Table,
}
fn sixty() -> u64 {
    60
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub source: String,
    /// article | event | dataset | paper | story
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub url: Option<String>,
    pub published: Option<String>,
    pub lang: Option<String>,
    pub country: Option<String>,
    pub extra: Value,
}

fn hid(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("wi_{h:016x}")
}

fn item(
    spec: &SourceSpec,
    kind: &str,
    title: &str,
    summary: &str,
    url: Option<String>,
    published: Option<String>,
    extra: Value,
) -> Item {
    Item {
        id: hid(&format!("{}|{}|{}", spec.name, url.clone().unwrap_or_default(), title)),
        source: spec.name.clone(),
        kind: kind.into(),
        title: title.trim().chars().take(400).collect(),
        summary: strip(summary).chars().take(1500).collect(),
        url,
        published,
        lang: spec.params.get("lang").and_then(|v| v.as_str()).map(String::from),
        country: spec.country.clone(),
        extra,
    }
}

/// HTML-tags weg + entiteiten.
pub fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn p<'a>(s: &'a SourceSpec, k: &str, d: &'a str) -> String {
    s.params
        .get(k)
        .map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))
        .unwrap_or_else(|| d.to_string())
}

async fn get_json(http: &reqwest::Client, url: &str) -> Result<Value, String> {
    let r = http.get(url).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    r.json().await.map_err(|e| e.to_string())
}

async fn get_text(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let r = http.get(url).send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        return Err(format!("HTTP {}", r.status()));
    }
    r.text().await.map_err(|e| e.to_string())
}

/// RSS 2.0 / RDF / Atom → (titel, link, samenvatting, datum).
pub fn parse_feed(xml: &str) -> Vec<(String, Option<String>, String, Option<String>)> {
    let mut r = Reader::from_str(xml);
    r.config_mut().trim_text(true);
    let mut out = Vec::new();
    let (mut in_item, mut cur) = (false, String::new());
    let (mut t, mut l, mut d, mut dt): (String, Option<String>, String, Option<String>) =
        (String::new(), None, String::new(), None);
    loop {
        match r.read_event() {
            Ok(X::Start(e)) | Ok(X::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                if name == "item" || name == "entry" {
                    in_item = true;
                    (t, l, d, dt) = (String::new(), None, String::new(), None);
                } else if in_item && name == "link" {
                    // Atom: <link href="…"/>
                    if let Some(h) = e.attributes().flatten().find(|a| a.key.as_ref() == b"href") {
                        let rel = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"rel")
                            .map(|a| String::from_utf8_lossy(&a.value).to_string());
                        if rel.as_deref().is_none_or(|r| r == "alternate") {
                            l = Some(String::from_utf8_lossy(&h.value).to_string());
                        }
                    }
                }
                cur = name;
            }
            Ok(X::Text(txt)) if in_item => {
                let v = txt.unescape().map(|c| c.to_string()).unwrap_or_default();
                match cur.as_str() {
                    "title" => t.push_str(&v),
                    "link" if l.is_none() => l = Some(v.trim().to_string()),
                    "description" | "summary" | "content" | "encoded" => d.push_str(&v),
                    "pubdate" | "published" | "updated" | "date" if dt.is_none() => dt = Some(v.trim().to_string()),
                    _ => {}
                }
            }
            Ok(X::CData(c)) if in_item => {
                let v = String::from_utf8_lossy(&c).to_string();
                match cur.as_str() {
                    "title" => t.push_str(&v),
                    "description" | "summary" | "content" | "encoded" => d.push_str(&v),
                    _ => {}
                }
            }
            Ok(X::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_lowercase();
                if (name == "item" || name == "entry") && in_item {
                    in_item = false;
                    if !t.trim().is_empty() {
                        out.push((strip(&t), l.clone(), d.clone(), dt.clone()));
                    }
                }
                cur.clear();
            }
            Ok(X::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

/// Datum normaliseren naar RFC 3339 (RSS gebruikt RFC 2822).
pub fn norm_date(s: &str) -> Option<String> {
    let s = s.trim();
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&chrono::Utc).to_rfc3339());
    }
    if let Ok(d) = chrono::DateTime::parse_from_rfc2822(s) {
        return Some(d.with_timezone(&chrono::Utc).to_rfc3339());
    }
    // GDELT: 20261102T101500Z
    chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ").ok().map(|d| d.and_utc().to_rfc3339())
}

pub async fn fetch(
    http: &reqwest::Client,
    s: &SourceSpec,
    since: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<Item>, String> {
    let base = s.url.trim_end_matches('/');
    let limit: usize = p(s, "limit", "50").parse().unwrap_or(50);
    // Inhaal-modus: venster = sinds laatste geslaagde keer (max 7 dagen).
    let hours = since.map(|t| ((chrono::Utc::now() - t).num_minutes() as f64 / 60.0).clamp(1.0, 168.0)).unwrap_or(24.0);
    Ok(match s.kind.as_str() {
        "gdelt" => {
            let url = format!(
                "{base}?query={}&mode=artlist&format=json&maxrecords={limit}&timespan={}h&sort=datedesc",
                urlenc(&p(s, "query", "sourcelang:english")),
                hours.ceil() as u64
            );
            let v = get_json(http, &url).await?;
            v["articles"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| {
                    let mut it = item(
                        s,
                        "article",
                        a["title"].as_str().unwrap_or(""),
                        "",
                        a["url"].as_str().map(String::from),
                        a["seendate"].as_str().and_then(norm_date),
                        json!({"domain": a["domain"]}),
                    );
                    it.lang = a["language"].as_str().map(String::from);
                    it.country = a["sourcecountry"].as_str().map(String::from).or(it.country);
                    it
                })
                .collect()
        }
        "rss" => parse_feed(&get_text(http, &s.url).await?)
            .into_iter()
            .take(limit)
            .map(|(t, l, d, dt)| {
                item(s, "article", &t, &d, l, dt.as_deref().and_then(norm_date), json!({"orientation": s.orientation}))
            })
            .collect(),
        "wikipedia_events" => {
            let day = since.map(|t| t.date_naive()).unwrap_or_else(|| chrono::Utc::now().date_naive());
            let page = format!("Portal:Current_events%2F{}", day.format("%Y_%B_%-d"));
            let html = get_text(http, &format!("{base}/{page}")).await?;
            // Bovenste <li>-items van de dagpagina.
            html.split("<li")
                .skip(1)
                .filter_map(|li| li.split_once('>').map(|x| x.1))
                .map(|li| strip(li.split("</li>").next().unwrap_or(li)))
                .filter(|t| t.len() > 30)
                .take(limit)
                .map(|t| {
                    item(
                        s,
                        "event",
                        &t.chars().take(200).collect::<String>(),
                        &t,
                        Some(format!("https://en.wikipedia.org/wiki/{page}")),
                        Some(format!("{day}T12:00:00+00:00")),
                        json!({}),
                    )
                })
                .collect()
        }
        "reliefweb" => {
            let v = get_json(
                http,
                &format!(
                    "{base}?appname={}&limit={limit}&sort[]=date.created:desc&profile=list",
                    p(s, "appname", "vortex")
                ),
            )
            .await?;
            v["data"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|d| {
                    let f = &d["fields"];
                    item(
                        s,
                        "event",
                        f["title"].as_str().unwrap_or(""),
                        "",
                        f["url_alias"].as_str().or(f["url"].as_str()).or(d["href"].as_str()).map(String::from),
                        f["date"]["created"].as_str().and_then(norm_date),
                        json!({"id": d["id"]}),
                    )
                })
                .collect()
        }
        "usgs" => {
            let v = get_json(http, &s.url).await?;
            v["features"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|f| {
                    let pr = &f["properties"];
                    let t =
                        pr["time"].as_i64().and_then(chrono::DateTime::from_timestamp_millis).map(|d| d.to_rfc3339());
                    item(
                        s,
                        "event",
                        pr["title"].as_str().unwrap_or(""),
                        &format!("Aardbeving M{} — {}", pr["mag"], pr["place"].as_str().unwrap_or("")),
                        pr["url"].as_str().map(String::from),
                        t,
                        json!({"mag": pr["mag"], "coords": f["geometry"]["coordinates"]}),
                    )
                })
                .collect()
        }
        "eonet" => {
            let v =
                get_json(http, &format!("{base}?status=open&days={}", (hours / 24.0).ceil().max(1.0) as u64)).await?;
            v["events"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| {
                    let cat = e["categories"][0]["title"].as_str().unwrap_or("");
                    let date = e["geometry"]
                        .as_array()
                        .and_then(|g| g.last())
                        .and_then(|g| g["date"].as_str())
                        .and_then(norm_date);
                    item(
                        s,
                        "event",
                        e["title"].as_str().unwrap_or(""),
                        cat,
                        e["sources"][0]["url"].as_str().or(e["link"].as_str()).map(String::from),
                        date,
                        json!({"category": cat, "id": e["id"]}),
                    )
                })
                .collect()
        }
        "worldbank" => {
            let ind = p(s, "indicator", "NY.GDP.MKTP.KD.ZG");
            let v =
                get_json(http, &format!("{base}/country/{}/indicator/{ind}?format=json&mrv=5", p(s, "country", "NLD")))
                    .await?;
            v[1].as_array()
                .into_iter()
                .flatten()
                .filter(|r| !r["value"].is_null())
                .map(|r| {
                    let t = format!(
                        "{} — {} {}: {}",
                        r["indicator"]["value"].as_str().unwrap_or(&ind),
                        r["country"]["value"].as_str().unwrap_or(""),
                        r["date"].as_str().unwrap_or(""),
                        r["value"]
                    );
                    item(
                        s,
                        "dataset",
                        &t,
                        "",
                        Some(format!("https://data.worldbank.org/indicator/{ind}")),
                        r["date"].as_str().map(|y| format!("{y}-12-31T00:00:00+00:00")),
                        json!({"value": r["value"], "indicator": ind}),
                    )
                })
                .collect()
        }
        "eurostat" => {
            let ds = p(s, "dataset", "prc_hicp_manr");
            let v = get_json(http, &format!("{base}/{ds}?format=JSON&lang=EN{}", p(s, "filter", ""))).await?;
            // JSON-stat: laatste periode + waarde.
            let label = v["label"].as_str().unwrap_or(&ds).to_string();
            let times = v["dimension"]["time"]["category"]["index"].as_object().cloned().unwrap_or_default();
            let mut ts: Vec<(&String, i64)> = times.iter().map(|(k, i)| (k, i.as_i64().unwrap_or(0))).collect();
            ts.sort_by_key(|x| x.1);
            let last = ts.last().cloned();
            match last {
                Some((t, i)) => {
                    let val = v["value"][i.to_string()].clone();
                    vec![item(
                        s,
                        "dataset",
                        &format!("{label} — {t}: {val}"),
                        "",
                        Some(format!("https://ec.europa.eu/eurostat/databrowser/view/{ds}")),
                        None,
                        json!({"value": val, "period": t}),
                    )]
                }
                None => vec![],
            }
        }
        "cbs" => {
            let table = p(s, "table", "83131NED");
            let v = get_json(http, &format!("{base}/{table}/TypedDataSet?$top={limit}&$orderby=ID%20desc")).await?;
            v["value"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| {
                    let per = r["Perioden"].as_str().unwrap_or("");
                    let nums: Vec<String> = r
                        .as_object()
                        .map(|o| {
                            o.iter()
                                .filter(|(k, x)| x.is_number() && *k != "ID")
                                .take(3)
                                .map(|(k, x)| format!("{k}={x}"))
                                .collect()
                        })
                        .unwrap_or_default();
                    item(
                        s,
                        "dataset",
                        &format!("CBS {table} {per}: {}", nums.join(", ")),
                        "",
                        Some(format!("https://opendata.cbs.nl/statline/#/CBS/nl/dataset/{table}")),
                        None,
                        r.clone(),
                    )
                })
                .collect()
        }
        "arxiv" => parse_feed(
            &get_text(
                http,
                &format!(
                    "{base}?search_query={}&sortBy=submittedDate&sortOrder=descending&max_results={limit}",
                    urlenc(&p(s, "query", "cat:cs.AI"))
                ),
            )
            .await?,
        )
        .into_iter()
        .map(|(t, l, d, dt)| item(s, "paper", &t, &d, l, dt.as_deref().and_then(norm_date), json!({})))
        .collect(),
        "hackernews" => {
            let ids = get_json(http, &format!("{base}/topstories.json")).await?;
            let mut out = Vec::new();
            for id in ids.as_array().into_iter().flatten().take(limit.min(30)) {
                let Ok(st) = get_json(http, &format!("{base}/item/{id}.json")).await else { continue };
                let t =
                    st["time"].as_i64().and_then(|x| chrono::DateTime::from_timestamp(x, 0)).map(|d| d.to_rfc3339());
                out.push(item(
                    s,
                    "story",
                    st["title"].as_str().unwrap_or(""),
                    "",
                    st["url"].as_str().map(String::from).or(Some(format!("https://news.ycombinator.com/item?id={id}"))),
                    t,
                    json!({"score": st["score"], "comments": st["descendants"]}),
                ));
            }
            out
        }
        "wikidata" => vec![], // geen nieuwsbron: gebruikt voor entiteit-koppeling (zie `wikidata_lookup`)
        k => return Err(format!("onbekende bronsoort {k}")),
    })
}

/// Wikidata: naam → (Q-ID, label, beschrijving) voor entity linking.
pub async fn wikidata_lookup(
    http: &reqwest::Client,
    base: &str,
    name: &str,
    lang: &str,
) -> Option<(String, String, String)> {
    let v = get_json(
        http,
        &format!(
            "{}?action=wbsearchentities&search={}&language={lang}&format=json&limit=1",
            base.trim_end_matches('/'),
            urlenc(name)
        ),
    )
    .await
    .ok()?;
    let e = &v["search"][0];
    Some((
        e["id"].as_str()?.to_string(),
        e["label"].as_str().unwrap_or(name).to_string(),
        e["description"].as_str().unwrap_or("").to_string(),
    ))
}

fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => (b as char).to_string(),
            b' ' => "%20".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_and_atom() {
        let rss = r#"<rss><channel><item><title>Brand in haven</title><link>https://x.nl/1</link><description><![CDATA[<p>Grote brand</p>]]></description><pubDate>Mon, 02 Nov 2026 10:00:00 +0100</pubDate></item></channel></rss>"#;
        let v = parse_feed(rss);
        assert_eq!(v[0].0, "Brand in haven");
        assert_eq!(v[0].1.as_deref(), Some("https://x.nl/1"));
        assert_eq!(strip(&v[0].2), "Grote brand");
        assert_eq!(norm_date(v[0].3.as_deref().unwrap()).unwrap(), "2026-11-02T09:00:00+00:00");
        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>Paper A</title><link href="http://arxiv.org/abs/1" rel="alternate"/><summary>Over LLMs</summary><published>2026-11-01T00:00:00Z</published></entry></feed>"#;
        let a = parse_feed(atom);
        assert_eq!(a[0].1.as_deref(), Some("http://arxiv.org/abs/1"));
        assert_eq!(norm_date("20261102T101500Z").unwrap(), "2026-11-02T10:15:00+00:00");
    }
}
