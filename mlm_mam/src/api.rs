use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::{Error, Result, bail};
use bytes::Bytes;
use cookie::Cookie;
use mlm_db::DatabaseExt as _;
use native_db::Database;
use reqwest::Url;
use reqwest_cookie_store::CookieStoreRwLock;
use time::{OffsetDateTime, UtcDateTime};
use tokio::sync::Mutex;
use tracing::{debug, error, info, trace, warn};

use crate::{
    // config::{Config, SnatchlistType},
    enums::SnatchlistType,
    search::{MaMTorrent, SearchError, SearchFields, SearchQuery, SearchResult, Tor},
    user_data::UserResponse,
    user_torrent::UserDetailsTorrentResponse,
};

#[derive(thiserror::Error, Debug)]
#[error("Account is blocked or unsat limit reached (HTTP 406 Not Acceptable)")]
pub struct AccountBlockedError;

#[derive(thiserror::Error, Debug)]
#[error("Hit MaM rate limit")]
pub struct RateLimitError;

impl RateLimitError {
    pub fn maybe(error: reqwest::Error) -> anyhow::Error {
        if error.status().is_some_and(|s| s == 429) {
            anyhow::Error::new(RateLimitError)
        } else if error.status().is_some_and(|s| s == 406){
            anyhow::Error::new(AccountBlockedError)
        } else {
            anyhow::Error::new(error)
        }
    }

    pub fn maybe_resp(response: &reqwest::Response) -> Result<(), anyhow::Error> {
        if response.status() == 429 {
            Err(anyhow::Error::new(RateLimitError))
        } else if response.status() == 406 {
            Err(anyhow::Error::new(AccountBlockedError))
        } else {
            Ok(())
        }
    }
}

pub struct MaM<'a> {
    jar: Arc<CookieStoreRwLock>,
    client: reqwest::Client,
    db: Arc<Database<'a>>,
    pub user: Arc<Mutex<Option<(SystemTime, UserResponse)>>>,
}

impl<'a> MaM<'a> {
    pub async fn new(mam_id: &str, db: Arc<Database<'a>>, version: &str) -> Result<MaM<'a>> {
        let jar: CookieStoreRwLock = Default::default();
        let url = "https://www.myanonamouse.net/json".parse::<Url>().unwrap();

        let stored_mam_id = db
            .r_transaction()
            .and_then(|r| r.get().primary::<mlm_db::Config>("mam_id"))
            .ok()
            .flatten()
            .map(|c| c.value);
        if stored_mam_id.is_some() {
            info!("Restoring mam_id");
        } else {
            info!("Using mam_id from config");
        }
        let has_stored_mam_id = stored_mam_id.is_some();
        let cookie = Cookie::build(("mam_id", stored_mam_id.unwrap_or(mam_id.to_owned())))
            .expires(OffsetDateTime::now_local()? + Duration::from_mins(10))
            .build();
        jar.write()
            .unwrap()
            .store_response_cookies([cookie].into_iter(), &url);

        let jar = Arc::new(jar);
        let client = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .user_agent(format!("MLM/{version}"))
            .timeout(Duration::from_secs(20))
            .build()?;

        let mam = MaM {
            jar,
            client,
            db,
            user: Default::default(),
        };
        if let Err(err) = mam.user_info().await {
            if has_stored_mam_id {
                warn!("Stored mam_id failed with {err}, falling back to config value");
                let cookie = Cookie::build(("mam_id", mam_id.to_owned()))
                    .expires(OffsetDateTime::now_local()? + Duration::from_mins(10))
                    .build();
                mam.jar
                    .write()
                    .unwrap()
                    .store_response_cookies([cookie].into_iter(), &url);

                // Retry with user_info()
                mam.user_info().await?;
            } else {
                return Err(err);
            }
        }

        Ok(mam)
    }

    pub async fn check_mam_id(&self) -> Result<()> {
        let resp = self
            .client
            .get("https://www.myanonamouse.net/json/checkCookie.php")
            .send()
            .await?;

        RateLimitError::maybe_resp(&resp)?;

        let status = resp.status();
        let text = resp.text().await?;
        info!("checked mam_id: {text:?}");

        if status.is_client_error() || status.is_server_error() {
            bail!("Bad status for checkCookie: {}", status)
        }

        // Check if the body contains "Success":true
        if !text.contains("\"Success\":true") {
            bail!(
                "Session check failed (Success: false in response): {}",
                text
            )
        }

        self.store_cookies().await;
        Ok(())
    }

    pub async fn user_info(&self) -> Result<UserResponse> {
        let mut cache = self.user.lock().await;
        if let Some((at, user_info)) = cache.as_ref()
            && SystemTime::now()
                .duration_since(*at)
                .is_ok_and(|d| d < Duration::from_secs(60))
        {
            return Ok(user_info.clone());
        }
        let resp: UserResponse = self
            .client
            .get("https://www.myanonamouse.net/jsonLoad.php?snatch_summary=true")
            .send()
            .await?
            .error_for_status()
            .map_err(RateLimitError::maybe)?
            .json()
            .await?;
        self.store_cookies().await;
        cache.replace((SystemTime::now(), resp.clone()));
        Ok(resp)
    }

    pub async fn cached_user_info(&self) -> Option<UserResponse> {
        self.user
            .lock()
            .await
            .as_ref()
            .map(|(_, user_info)| user_info.clone())
    }

    pub async fn add_unsats(&self, unsats: u64) {
        if let Some((_, user)) = self.user.lock().await.as_mut() {
            user.unsat.count += unsats;
        }
    }

    pub async fn get_torrent_file(&self, dl_hash: &str, tid: u64, wedge: bool) -> Result<Bytes> {
        let resp = self
            .client
            .get(torrent_file_url(dl_hash, tid, wedge))
            .send()
            .await?
            .error_for_status()
            .map_err(RateLimitError::maybe)?
            .bytes()
            .await?;
        Ok(resp)
    }

    pub async fn get_torrent_info(&self, hash: &str) -> Result<Option<MaMTorrent>> {
        let mut resp = self
            .search(&SearchQuery {
                fields: SearchFields {
                    description: true,
                    isbn: true,
                    ..Default::default()
                },
                tor: Tor {
                    hash: hash.to_string(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .await?;
        Ok(resp.data.pop().map(|mut t| {
            t.fix();
            t
        }))
    }
    pub async fn get_torrent_info_by_id(&self, mam_id: u64) -> Result<Option<MaMTorrent>> {
        let mut resp = self
            .search(&SearchQuery {
                fields: SearchFields {
                    description: true,
                    media_info: true,
                    isbn: true,
                    dl_link: true,
                    ..Default::default()
                },
                tor: Tor {
                    id: mam_id,
                    ..Default::default()
                },
                ..Default::default()
            })
            .await?;
        Ok(resp.data.pop().map(|mut t| {
            t.fix();
            t
        }))
    }

    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
        debug!("search: {}", serde_json::to_string_pretty(query)?);
        let resp = self
            .client
            .post("https://www.myanonamouse.net/tor/js/loadSearchJSONbasic.php")
            .json(query)
            .send()
            .await?
            .error_for_status()
            .map_err(RateLimitError::maybe)?
            .text()
            .await?;
        if let Ok(resp) = serde_json::from_str::<SearchError>(&resp) {
            if resp.error == "Nothing returned, out of 0" {
                return Ok(SearchResult::default());
            } else {
                return Err(Error::msg(resp.error));
            }
        };
        let mut resp: SearchResult = serde_json::from_str(&resp).map_err(|err| {
            error!("Error parsing mam response: {err}\nResponse: {resp}");
            err
        })?;
        for t in &mut resp.data {
            t.fix();
        }
        self.store_cookies().await;
        Ok(resp)
    }

    pub async fn snatchlist(
        &self,
        kind: SnatchlistType,
        page: u64,
        timestamp: UtcDateTime,
    ) -> Result<UserDetailsTorrentResponse> {
        debug!("snatchlist: {:?} {}", kind, page);
        let user = self.user_info().await?;
        let resp = self
            .client
            .get("https://cdn.myanonamouse.net/json/loadUserDetailsTorrents.php")
            .query(&(
                ("uid", user.uid.to_string()),
                ("iteration", page.to_string()),
                ("type", kind.to_string()),
                ("cacheTime", timestamp.unix_timestamp().to_string()),
            ))
            .send()
            .await?
            .error_for_status()
            .map_err(RateLimitError::maybe)?
            .text()
            .await?;
        let resp: UserDetailsTorrentResponse = serde_json::from_str(&resp).map_err(|err| {
            error!("Error parsing mam response: {err}\nResponse: {resp}");
            err
        })?;
        self.store_cookies().await;
        Ok(resp)
    }

    async fn store_cookies(&self) {
        let Ok(jar) = self.jar.read() else {
            return;
        };
        let Some(cookie) = jar.get("myanonamouse.net", "/", "mam_id") else {
            return;
        };
        let value = cookie.value().to_string();
        drop(jar);
        let Ok((_guard, rw)) = self.db.rw_try() else {
            return;
        };
        let ok = rw
            .upsert(mlm_db::Config {
                key: "mam_id".to_string(),
                value,
            })
            .and_then(|_| rw.commit())
            .is_ok();

        if ok {
            trace!("stored new mam_id");
        }
    }
}

/// Build the download URL for a torrent.
///
/// `tid` is required. The API docs give the dl hash form as
/// `/tor/download.php/<hash>?tid=<id>`; without the `tid` argument the request
/// is invalid and the site rejects it.
///
/// `wedge` adds the `fl` flag, which spends a personal freeleech wedge on the
/// torrent as part of the download. It replaces the bonusBuy.php endpoint,
/// which the site no longer exposes for wedges.
fn torrent_file_url(dl_hash: &str, tid: u64, wedge: bool) -> String {
    let fl = if wedge { "&fl" } else { "" };
    format!("https://www.myanonamouse.net/tor/download.php/{dl_hash}?tid={tid}{fl}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_torrent_file_url_includes_tid() {
        assert_eq!(
            torrent_file_url("abc123", 1234567890, false),
            "https://www.myanonamouse.net/tor/download.php/abc123?tid=1234567890"
        );
    }

    #[test]
    fn test_torrent_file_url_wedge_adds_fl() {
        assert_eq!(
            torrent_file_url("abc123", 1234567890, true),
            "https://www.myanonamouse.net/tor/download.php/abc123?tid=1234567890&fl"
        );
    }
}
