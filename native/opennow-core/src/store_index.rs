//! Compact local title/facet index. Full game records stay in the page cache;
//! only the selected page is materialized for Qt or the command palette.
use crate::{gfn::ServiceError, store_catalog_page};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};

pub struct Entry {
    id: String,
    title: String,
    words: Vec<String>,
    acronym: String,
    owned: bool,
    genres: Vec<String>,
    stores: Vec<String>,
    categories: HashSet<String>,
    key: Value,
    pointer: String,
}

#[derive(Default)]
pub struct StoreIndex {
    entries: Vec<Entry>,
    identities: HashMap<String, usize>,
    pub categories: Vec<Value>,
    pub complete: bool,
    pub next_upstream: String,
    pub pages: usize,
}

fn normalized(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn error(message: &str) -> ServiceError {
    ServiceError {
        code: "invalid_params",
        message: message.into(),
    }
}

fn cursor_digest(entries: &[Entry], matches: &[(usize, u32)]) -> String {
    let mut digest = Sha256::new();
    for &(at, _) in matches {
        let id = &entries[at].id;
        digest.update((id.len() as u64).to_le_bytes());
        digest.update(id.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

impl StoreIndex {
    pub fn add(&mut self, game: &Value, key: Value, pointer: String, category: Option<&str>) {
        let id = game["id"].as_str().unwrap_or("");
        if id.is_empty() {
            return;
        }
        if let Some(&at) = self.identities.get(id) {
            if let Some(category) = category {
                self.entries[at].categories.insert(category.into());
            }
            return;
        }
        if self.entries.len() >= 12000 {
            return;
        }
        let title = normalized(game["title"].as_str().unwrap_or(""));
        let words: Vec<String> = title.split_whitespace().map(str::to_owned).collect();
        let acronym = words
            .iter()
            .filter_map(|word| word.chars().next())
            .collect();
        let mut categories: HashSet<String> =
            strings(&game["storeCategories"]).into_iter().collect();
        if let Some(category) = category {
            categories.insert(category.into());
        }
        if game["freeToPlay"] == true
            || game["storeFree"] == true
            || game["playType"]
                .as_str()
                .unwrap_or("")
                .to_uppercase()
                .contains("FREE")
        {
            categories.insert("free".into());
        }
        let state = normalized(game["playabilityState"].as_str().unwrap_or(""));
        if state.contains("coming") || state.contains("unreleased") {
            categories.insert("coming".into());
        }
        if strings(&game["nvidiaTech"])
            .iter()
            .any(|tech| tech.to_lowercase().contains("rtx"))
        {
            categories.insert("rtx".into());
        }
        let mut stores = strings(&game["availableStores"]);
        for variant in game["variants"].as_array().into_iter().flatten() {
            if let Some(store) = variant["store"].as_str() {
                if !stores.iter().any(|s| s == store) {
                    stores.push(store.into());
                }
            }
        }
        self.identities.insert(id.into(), self.entries.len());
        self.entries.push(Entry {
            id: id.into(),
            title,
            words,
            acronym,
            owned: game["isInLibrary"] == true || game["storeOwned"] == true,
            genres: strings(&game["genres"]),
            stores,
            categories,
            key,
            pointer,
        });
    }

    pub fn add_panels(&mut self, panels: &Value, key: &Value) {
        for (p, panel) in panels["items"].as_array().into_iter().flatten().enumerate() {
            for (s, section) in panel["sections"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let category = format!("shelf:{p}:{s}");
                let title = section["title"].as_str().unwrap_or("Games");
                let lower = title.to_lowercase();
                let alias = if lower.contains("thursday")
                    || lower.contains("new")
                    || lower.contains("recent")
                {
                    Some("new")
                } else if lower.contains("rtx") {
                    Some("rtx")
                } else if lower.contains("free") {
                    Some("free")
                } else if lower.contains("coming") {
                    Some("coming")
                } else {
                    None
                };
                let games = section["games"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                for (g, game) in games.iter().enumerate() {
                    let pointer = format!("/items/{p}/sections/{s}/games/{g}");
                    self.add(game, key.clone(), pointer.clone(), Some(&category));
                    if let Some(alias) = alias {
                        self.add(game, key.clone(), pointer, Some(alias));
                    }
                }
                if !games.is_empty() {
                    self.categories
                        .push(json!({"id":category,"label":title,"count":games.len()}));
                }
            }
        }
    }

    pub fn query(
        &self,
        params: &Value,
        mut read: impl FnMut(&Value) -> Option<Value>,
    ) -> Result<Value, ServiceError> {
        let page = store_catalog_page::PageRequest::parse(params)?;
        let query = normalized(&page.search);
        let tokens: Vec<_> = query.split_whitespace().take(8).collect();
        let filter = |name: &str| -> Result<String, ServiceError> {
            match params.get(name) {
                None => Ok(String::new()),
                Some(Value::String(s)) if s.len() <= 256 => Ok(s.to_owned()),
                _ => Err(error("Invalid local Store filter")),
            }
        };
        let genre = filter("genre")?;
        let store = filter("store")?;
        let category = filter("categoryId")?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                json!([query, genre, store, category])
                    .to_string()
                    .as_bytes()
            )
        );
        let prefix = format!("local:{}:", &fingerprint[..16]);
        let continuation = if page.cursor.is_empty() {
            None
        } else {
            let invalid = || error("Local Store cursor does not match its search and filters");
            let mut parts = page
                .cursor
                .strip_prefix(&prefix)
                .ok_or_else(invalid)?
                .split(':');
            let number = |text: Option<&str>, max: usize| {
                text.filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
                    .and_then(|text| text.parse::<usize>().ok())
                    .filter(|n| *n <= max)
                    .ok_or_else(invalid)
            };
            let offset = number(parts.next(), 12000)?;
            let digest = parts
                .next()
                .filter(|text| {
                    text.len() == 64
                        && text
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
                .ok_or_else(invalid)?;
            let pages = number(parts.next(), 100)?;
            if parts.next().is_some() {
                return Err(invalid());
            }
            Some((offset, digest, pages))
        };
        let mut matches: Vec<_> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(at, entry)| {
                if !genre.is_empty() && !entry.genres.iter().any(|g| g.eq_ignore_ascii_case(&genre))
                {
                    return None;
                }
                if !store.is_empty() && !entry.stores.iter().any(|s| s.eq_ignore_ascii_case(&store))
                {
                    return None;
                }
                if !category.is_empty()
                    && category != "featured"
                    && category != "all"
                    && !entry.categories.contains(&category)
                {
                    return None;
                }
                score(entry, &query, &tokens).map(|score| (at, score))
            })
            .collect();
        if !query.is_empty() {
            matches.sort_by_key(|&(at, score)| (std::cmp::Reverse(score), at));
        }
        let (offset, replace) = match continuation {
            Some((offset, digest, pages))
                if pages > self.pages
                    || offset > matches.len()
                    || digest != cursor_digest(&self.entries, &matches[..offset]) =>
            {
                (0, true)
            }
            Some((offset, _, _)) => (offset, false),
            None => (0, false),
        };
        let genres: BTreeSet<_> = self
            .entries
            .iter()
            .flat_map(|e| e.genres.iter().cloned())
            .collect();
        let stores: BTreeSet<_> = self
            .entries
            .iter()
            .flat_map(|e| e.stores.iter().cloned())
            .collect();
        let facets = json!({"genres":genres,"stores":stores,"categories":self.categories});
        let mut loaded = HashMap::new();
        let selected: Vec<Value> = matches
            .iter()
            .skip(offset)
            .take(page.limit.min(60))
            .map(|&(at, _)| {
                let entry = &self.entries[at];
                let key = entry.key.to_string();
                let document = loaded.entry(key).or_insert_with(|| read(&entry.key));
                document
                    .as_ref()
                    .and_then(|doc| doc.pointer(&entry.pointer))
                    .filter(|game| game["id"] == entry.id)
                    .cloned()
                    .ok_or_else(|| ServiceError {
                        code: "store_cache_changed",
                        message: "Saved Store data changed. Refresh the results.".into(),
                    })
            })
            .collect::<Result<_, _>>()?;
        store_catalog_page::fetch_bounded_page(page.limit.min(60), |limit| {
            let count = selected.len().min(limit);
            let next = offset + count;
            let more = next < matches.len() || !self.complete;
            let next_cursor = if more {
                format!(
                    "{prefix}{next}:{}:{}",
                    cursor_digest(&self.entries, &matches[..next]),
                    self.pages
                )
            } else {
                String::new()
            };
            Ok(
                json!({"games":&selected[..count],"count":count,"totalCount":matches.len(),"catalogTotalCount":self.entries.len(),
                "hasNextPage":more,"nextCursor":next_cursor,"replace":replace,
                "source":"store-local","cacheHit":true,"cacheComplete":self.complete,
                "localHasNextPage":next < matches.len(),
                "upstreamCoverage":if self.complete {"complete-traversal"} else {"partial"},
                "facetsSource":"local-cache",
                "upstreamCursor":self.next_upstream,
                "facets":if offset == 0 {facets.clone()} else {Value::Null}}),
            )
        })
    }
}

fn score(entry: &Entry, query: &str, tokens: &[&str]) -> Option<u32> {
    if query.is_empty() {
        return Some(0);
    }
    if entry.title == query {
        return Some(12000 + if entry.owned { 300 } else { 0 });
    }
    if entry.acronym == query.replace(' ', "") {
        return Some(11000 + if entry.owned { 300 } else { 0 });
    }
    let mut score = if entry.title.starts_with(query) {
        9000
    } else if entry.title.contains(query) {
        7000
    } else {
        4000
    };
    for token in tokens {
        let best = entry
            .words
            .iter()
            .filter_map(|word| {
                if word == token {
                    Some(300)
                } else if word.starts_with(token) {
                    Some(200)
                } else if token.len() >= 3 && word.contains(token) {
                    Some(100)
                } else if token.len() >= 4 && one_typo(word, token) {
                    Some(30)
                } else {
                    None
                }
            })
            .max()?;
        score += best;
    }
    Some(score + if entry.owned { 300 } else { 0 })
}

// Bounded edit-distance-one matcher, including adjacent transpositions. It
// tolerates common typing errors without broad substring/subsequence noise.
fn one_typo(a: &str, b: &str) -> bool {
    let a: Vec<_> = a.chars().take(64).collect();
    let b: Vec<_> = b.chars().take(64).collect();
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let first = a
        .iter()
        .zip(&b)
        .position(|(a, b)| a != b)
        .unwrap_or(a.len().min(b.len()));
    if first == a.len().min(b.len()) {
        return true;
    }
    if a.len() == b.len() {
        a[first + 1..] == b[first + 1..]
            || (first + 1 < a.len()
                && a[first] == b[first + 1]
                && a[first + 1] == b[first]
                && a[first + 2..] == b[first + 2..])
    } else if a.len() > b.len() {
        a[first + 1..] == b[first..]
    } else {
        a[first..] == b[first + 1..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (StoreIndex, Value) {
        let games = json!({"games":[{"id":"1","title":"Counter-Strike 2","genres":["ACTION"],"availableStores":["STEAM"]},
            {"id":"2","title":"Fortnite","genres":["ACTION"],"availableStores":["EPIC"]},
            {"id":"3","title":"Fortnite Festival","genres":["MUSIC"],"availableStores":["EPIC"]}]});
        let mut index = StoreIndex {
            complete: true,
            ..StoreIndex::default()
        };
        for (i, g) in games["games"].as_array().unwrap().iter().enumerate() {
            index.add(g, json!("page"), format!("/games/{i}"), None);
        }
        (index, games)
    }
    #[test]
    fn ranked_search_handles_titles_acronyms_word_order_and_typos() {
        let (index, games) = fixture();
        for (query, id) in [
            ("fortnite", "2"),
            ("fortn", "2"),
            ("fortntie", "2"),
            ("cs2", "1"),
            ("2 counter", "1"),
        ] {
            let result = index
                .query(&json!({"searchQuery":query}), |_| Some(games.clone()))
                .unwrap();
            assert_eq!(result["games"][0]["id"], id, "{query}");
        }
        assert_eq!(
            index
                .query(&json!({"searchQuery":"zzzz"}), |_| panic!())
                .unwrap()["count"],
            0
        );
    }
    #[test]
    fn paging_is_bounded_and_facets_cover_unloaded_games() {
        let (index, games) = fixture();
        let first = index
            .query(&json!({"limit":1}), |_| Some(games.clone()))
            .unwrap();
        assert_eq!(first["count"], 1);
        assert_eq!(first["facets"]["genres"].as_array().unwrap().len(), 2);
        let next = index
            .query(&json!({"limit":1,"cursor":first["nextCursor"]}), |_| {
                Some(games.clone())
            })
            .unwrap();
        assert_eq!(next["games"][0]["id"], "2");
        assert!(
            index
                .query(
                    &json!({"searchQuery":"other","cursor":first["nextCursor"]}),
                    |_| Some(games.clone())
                )
                .is_err()
        );
        assert_eq!(
            index
                .query(&json!({"store":"EPIC","genre":"MUSIC"}), |_| Some(
                    games.clone()
                ))
                .unwrap()["games"][0]["id"],
            "3"
        );
    }

    #[test]
    fn ranked_insertion_before_offset_replaces_and_reaches_every_match() {
        let mut games = json!({"games":[
            {"id":"festival","title":"Fortnite Festival"},
            {"id":"zero","title":"Fortnite Zero"}
        ]});
        let mut index = StoreIndex {
            pages: 1,
            ..StoreIndex::default()
        };
        for (at, game) in games["games"].as_array().unwrap().iter().enumerate() {
            index.add(game, json!("page"), format!("/games/{at}"), None);
        }
        let first = index
            .query(&json!({"limit":1,"searchQuery":"fortnite"}), |_| {
                Some(games.clone())
            })
            .unwrap();
        assert_eq!(first["games"][0]["id"], "festival");
        let exact = json!({"id":"exact","title":"Fortnite"});
        index.add(&exact, json!("page"), "/games/2".into(), None);
        games["games"].as_array_mut().unwrap().push(exact);
        index.pages = 2;
        index.complete = true;

        let mut result = index
            .query(
                &json!({"limit":1,"searchQuery":"fortnite","cursor":first["nextCursor"]}),
                |_| Some(games.clone()),
            )
            .unwrap();
        assert_eq!(result["games"][0]["id"], "exact");
        assert_eq!(result["replace"], true);
        assert!(result["facets"].is_object());
        let mut reached = vec![result["games"][0]["id"].clone()];
        for _ in 0..2 {
            result = index
                .query(
                    &json!({"limit":1,"searchQuery":"fortnite","cursor":result["nextCursor"]}),
                    |_| Some(games.clone()),
                )
                .unwrap();
            assert_eq!(result["replace"], false);
            reached.push(result["games"][0]["id"].clone());
        }
        assert_eq!(
            reached,
            vec![json!("exact"), json!("festival"), json!("zero")]
        );
        assert_eq!(result["hasNextPage"], false);
        assert_eq!(result["nextCursor"], "");
    }

    #[test]
    fn local_cursor_requires_a_valid_complete_pages_suffix() {
        let (index, games) = fixture();
        let first = index
            .query(&json!({"limit":1}), |_| Some(games.clone()))
            .unwrap();
        let cursor = first["nextCursor"].as_str().unwrap();
        let (without_pages, _) = cursor.rsplit_once(':').unwrap();
        for invalid in [
            format!("{without_pages}:not-a-page"),
            format!("{without_pages}:101"),
            format!("{cursor}:ignored"),
            without_pages.to_owned(),
        ] {
            assert_eq!(
                index
                    .query(&json!({"limit":1,"cursor":invalid}), |_| panic!(
                        "invalid cursor read a page"
                    ))
                    .unwrap_err()
                    .code,
                "invalid_params"
            );
        }
    }

    #[test]
    fn growth_after_consumed_prefix_continues_without_replacement() {
        let (mut index, mut games) = fixture();
        index.pages = 1;
        index.complete = false;
        let first = index
            .query(&json!({"limit":1,"searchQuery":"fortnite"}), |_| {
                Some(games.clone())
            })
            .unwrap();
        assert_eq!(first["games"][0]["id"], "2");
        let added = json!({"id":"4","title":"Fortnite Zero"});
        index.add(&added, json!("page"), "/games/3".into(), None);
        games["games"].as_array_mut().unwrap().push(added);
        index.pages = 2;
        index.complete = true;
        let next = index
            .query(
                &json!({"limit":1,"searchQuery":"fortnite","cursor":first["nextCursor"]}),
                |_| Some(games.clone()),
            )
            .unwrap();
        assert_eq!(next["replace"], false);
        assert_eq!(next["games"][0]["id"], "3");
        assert_eq!(next["facets"], Value::Null);
        let last = index
            .query(
                &json!({"limit":1,"searchQuery":"fortnite","cursor":next["nextCursor"]}),
                |_| Some(games.clone()),
            )
            .unwrap();
        assert_eq!(last["replace"], false);
        assert_eq!(last["games"][0]["id"], "4");
        assert_eq!(last["hasNextPage"], false);
    }

    #[test]
    fn saved_page_loss_replaces_even_when_ranked_prefix_is_unchanged() {
        let (mut index, games) = fixture();
        index.pages = 2;
        let first = index
            .query(&json!({"limit":1}), |_| Some(games.clone()))
            .unwrap();
        index.pages = 1;
        let replacement = index
            .query(&json!({"limit":1,"cursor":first["nextCursor"]}), |_| {
                Some(games.clone())
            })
            .unwrap();
        assert_eq!(replacement["replace"], true);
        assert_eq!(replacement["games"], first["games"]);
        assert_ne!(replacement["nextCursor"], first["nextCursor"]);
        assert!(replacement["facets"].is_object());
    }

    #[test]
    fn empty_partial_search_progress_uses_pages_suffix_until_completion() {
        let (mut index, _) = fixture();
        index.complete = false;
        index.pages = 1;
        let first = index
            .query(&json!({"searchQuery":"no matching game"}), |_| panic!())
            .unwrap();
        assert_eq!(first["count"], 0);
        assert_eq!(first["hasNextPage"], true);
        index.pages = 2;
        let next = index
            .query(
                &json!({"searchQuery":"no matching game","cursor":first["nextCursor"]}),
                |_| panic!(),
            )
            .unwrap();
        assert_eq!(next["replace"], false);
        assert_eq!(next["count"], 0);
        assert_ne!(next["nextCursor"], first["nextCursor"]);
        index.complete = true;
        let last = index
            .query(
                &json!({"searchQuery":"no matching game","cursor":next["nextCursor"]}),
                |_| panic!(),
            )
            .unwrap();
        assert_eq!(last["replace"], false);
        assert_eq!(last["hasNextPage"], false);
        assert_eq!(last["nextCursor"], "");
    }
    #[test]
    fn only_selected_pages_are_read() {
        let (index, games) = fixture();
        let mut reads = 0;
        index
            .query(&json!({"limit":2}), |_| {
                reads += 1;
                Some(games.clone())
            })
            .unwrap();
        assert_eq!(reads, 1);
        assert!(!one_typo("fortnite", "fzzzzzzz"));
    }

    #[test]
    fn panel_categories_include_games_outside_the_first_page() {
        let (mut index, games) = fixture();
        let panels = json!({"items":[{"sections":[{"title":"GFN Thursday",
            "games":[games["games"][2].clone(), {"id":"4","title":"Coming Game"}]}]}]});
        index.add_panels(&panels, &json!("panels"));
        let result = index
            .query(&json!({"categoryId":"shelf:0:0"}), |key| {
                Some(if key == "panels" {
                    panels.clone()
                } else {
                    games.clone()
                })
            })
            .unwrap();
        assert_eq!(result["count"], 2);
        assert_eq!(result["games"][1]["id"], "4");
        assert_eq!(result["facets"]["categories"][0]["label"], "GFN Thursday");
    }

    #[test]
    fn owned_game_breaks_ambiguous_acronym_ties() {
        let games = json!({"games":[{"id":"1","title":"Clouds & Sheep 2"},
            {"id":"2","title":"Counter-Strike 2","isInLibrary":true}]});
        let mut index = StoreIndex {
            complete: true,
            ..StoreIndex::default()
        };
        for (i, game) in games["games"].as_array().unwrap().iter().enumerate() {
            index.add(game, json!("page"), format!("/games/{i}"), None);
        }
        let result = index
            .query(&json!({"searchQuery":"cs2"}), |_| Some(games.clone()))
            .unwrap();
        assert_eq!(result["games"][0]["id"], "2");
    }
}
