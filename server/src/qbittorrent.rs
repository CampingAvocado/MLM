use anyhow::Result;
use qbit::{
    models::Torrent,
    parameters::{AddTorrent, TorrentListParams},
};

use crate::config::{Config, QbitConfig};

pub async fn ensure_category_exists(qbit: &qbit::Api, category: &str) -> Result<()> {
    if category.is_empty() {
        return Ok(());
    }

    match qbit.create_category(category, "").await {
        Ok(()) => Ok(()),
        // 409 means the category already exists — that's fine
        Err(qbit::Error::ReqwestError(e))
            if e.status() == Some(reqwest::StatusCode::CONFLICT) =>
        {
            Ok(())
        }
        Err(e) => Err(anyhow::Error::new(e)),
    }
}

pub async fn add_torrent_with_category(qbit: &qbit::Api, add_torrent: AddTorrent) -> Result<()> {
    if let Some(ref category) = add_torrent.category {
        if !category.is_empty() {
            ensure_category_exists(qbit, category).await?;
        }
    }

    qbit.add_torrent(add_torrent)
        .await
        .map_err(anyhow::Error::new)
}

pub async fn get_torrent<'a, 'b>(
    config: &'a Config,
    hash: &'b str,
) -> Result<Option<(Torrent, qbit::Api, &'a QbitConfig)>> {
    for qbit_conf in config.qbittorrent.iter() {
        let Ok(qbit) = qbit::Api::new_login_username_password(
            &qbit_conf.url,
            &qbit_conf.username,
            &qbit_conf.password,
        )
        .await
        else {
            continue;
        };
        let Some(torrent) = qbit
            .torrents(Some(TorrentListParams {
                hashes: Some(vec![hash.to_string()]),
                ..TorrentListParams::default()
            }))
            .await?
            .into_iter()
            .next()
        else {
            continue;
        };
        return Ok(Some((torrent, qbit, qbit_conf)));
    }
    Ok(None)
}
