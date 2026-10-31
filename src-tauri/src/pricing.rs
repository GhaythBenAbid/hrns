//! API prices from models.dev, cached on disk. Prices rarely change, so the
//! catalog is fetched once and only re-fetched (at most daily) when a model we
//! need is missing from the cache.

use crate::store::now_ms;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

const CATALOG_URL: &str = "https://models.dev/api.json";
const REFETCH_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

/// USD per 1M tokens.
#[derive(Clone, Serialize, Deserialize)]
pub struct Price {
    pub input: Option<f64>,
    pub output: Option<f64>,
    pub cached: Option<f64>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    fetched_at: i64,
    /// Keyed by "provider/model".
    prices: HashMap<String, Price>,
}

pub struct Pricing {
    path: PathBuf,
    cache: Mutex<Option<Cache>>,
}

impl Pricing {
    pub fn new(path: PathBuf) -> Self {
        Self { path, cache: Mutex::new(None) }
    }

    /// Prices for `provider`'s models, keyed by model id. Hits the network only
    /// when there is no cache yet or one of `wanted` is unknown.
    pub fn get(&self, provider: &str, wanted: &[String]) -> HashMap<String, Price> {
        let mut guard = self.cache.lock().unwrap();
        let cache = guard.get_or_insert_with(|| self.load());

        let key = |m: &str| format!("{provider}/{m}");
        let missing = cache.prices.is_empty() || wanted.iter().any(|m| !cache.prices.contains_key(&key(m)));
        if missing && now_ms() - cache.fetched_at > REFETCH_AFTER_MS {
            if let Some(prices) = fetch_catalog() {
                *cache = Cache { fetched_at: now_ms(), prices };
            } else {
                // Offline: don't retry until the window passes; seed from opencode's copy if we have nothing.
                cache.fetched_at = now_ms();
                if cache.prices.is_empty() {
                    cache.prices = opencode_catalog().unwrap_or_default();
                }
            }
            self.save(cache);
        }

        let prefix = format!("{provider}/");
        let mut out: HashMap<String, Price> = cache
            .prices
            .iter()
            .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|id| (id.to_owned(), v.clone())))
            .collect();
        // Models served through other providers (custom Codex providers, proxies): match by id anywhere.
        let missing: Vec<&String> = wanted.iter().filter(|m| !out.contains_key(*m)).collect();
        for m in missing {
            let suffix = format!("/{m}");
            let mut hits: Vec<_> = cache.prices.iter().filter(|(k, _)| k.ends_with(&suffix)).collect();
            hits.sort_by(|a, b| a.0.cmp(b.0));
            if let Some((_, p)) = hits.into_iter().find(|(_, p)| p.input.is_some() && p.output.is_some()) {
                out.insert(m.clone(), p.clone());
            }
        }
        out
    }

    fn load(&self) -> Cache {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, cache: &Cache) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec(cache) {
            let tmp = self.path.with_extension("json.tmp");
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(tmp, &self.path);
            }
        }
    }
}

fn fetch_catalog() -> Option<HashMap<String, Price>> {
    // curl ships with macOS; avoids bundling an HTTP/TLS stack for one request.
    let out = Command::new("/usr/bin/curl")
        .args(["-sfL", "--max-time", "20", CATALOG_URL])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let prices = parse_catalog(&serde_json::from_slice(&out.stdout).ok()?);
    (!prices.is_empty()).then_some(prices)
}

/// opencode keeps a copy of the same catalog; used only as an offline fallback.
fn opencode_catalog() -> Option<HashMap<String, Price>> {
    let path = PathBuf::from(std::env::var("HOME").ok()?).join(".cache/opencode/models.json");
    Some(parse_catalog(&serde_json::from_slice(&std::fs::read(path).ok()?).ok()?))
}

fn parse_catalog(catalog: &Value) -> HashMap<String, Price> {
    let mut prices = HashMap::new();
    for (provider, p) in catalog.as_object().into_iter().flatten() {
        for (id, m) in p["models"].as_object().into_iter().flatten() {
            let c = &m["cost"];
            if c.is_object() {
                prices.insert(
                    format!("{provider}/{id}"),
                    Price { input: c["input"].as_f64(), output: c["output"].as_f64(), cached: c["cache_read"].as_f64() },
                );
            }
        }
    }
    prices
}
