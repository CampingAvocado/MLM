use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "RawUserResponse")]
pub struct UserResponse {
    pub uid: u64,
    pub username: String,
    pub downloaded_bytes: f64,
    pub uploaded_bytes: f64,
    pub seedbonus: i64,
    pub wedges: u64,
    pub unsat: Unsats,
    // pub classname: UserClass,
    // pub connectable: String,
    // pub country_code: Option<String>,
    // pub country_name: Option<String>,
    // pub created: u64,
    // pub downloaded: String,
    // pub duplicates: Duplicates,
    // #[serde(rename = "inactHnr")]
    // pub inact_hnr: InactHnr,
    // #[serde(rename = "inactSat")]
    // pub inact_sat: InactHnr,
    // #[serde(rename = "inactUnsat")]
    // pub inact_unsat: InactHnr,
    // pub ipv6_mac: bool,
    // pub ite: Ite,
    // pub last_access: Option<String>,
    // pub last_access_ago: Option<String>,
    // pub leeching: InactHnr,
    // pub partial: bool,
    // pub ratio: f64,
    // pub recently_deleted: u64,
    // pub reseed: Reseed,
    // #[serde(rename = "sSat")]
    // pub s_sat: InactHnr,
    // #[serde(rename = "seedHnr")]
    // pub seed_hnr: InactHnr,
    // #[serde(rename = "seedUnsat")]
    // pub seed_unsat: InactHnr,
    // #[serde(rename = "upAct")]
    // pub up_act: InactHnr,
    // #[serde(rename = "upInact")]
    // pub up_inact: InactHnr,
    // pub update: u64,
    // pub uploaded: String,
    // pub username: String,
    // pub v6_connectable: bool,
    // pub vip_until: Option<String>,
}

/// Wire format of jsonLoad.php. MaM used to return the snatch summary fields
/// (including `unsat`) at the top level, but now nests them under
/// `snatch_summary`. Accept both.
#[derive(Deserialize)]
struct RawUserResponse {
    uid: u64,
    username: String,
    downloaded_bytes: f64,
    uploaded_bytes: f64,
    seedbonus: i64,
    wedges: u64,
    unsat: Option<Unsats>,
    snatch_summary: Option<SnatchSummary>,
}

#[derive(Deserialize)]
struct SnatchSummary {
    unsat: Option<Unsats>,
}

impl TryFrom<RawUserResponse> for UserResponse {
    type Error = &'static str;

    fn try_from(raw: RawUserResponse) -> Result<Self, Self::Error> {
        let unsat = raw
            .unsat
            .or_else(|| raw.snatch_summary.and_then(|s| s.unsat))
            .ok_or("missing unsat in user data (top level or snatch_summary)")?;
        Ok(UserResponse {
            uid: raw.uid,
            username: raw.username,
            downloaded_bytes: raw.downloaded_bytes,
            uploaded_bytes: raw.uploaded_bytes,
            seedbonus: raw.seedbonus,
            wedges: raw.wedges,
            unsat,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unsats {
    pub count: u64,
    // Not present in the nested snatch_summary.unsat format
    #[serde(default)]
    pub red: bool,
    pub size: Option<u64>,
    pub limit: u64,
}

// #[derive(Debug, Serialize, Deserialize)]
// pub struct Duplicates {
//     pub count: u64,
//     pub red: bool,
// }
//
// #[derive(Debug, Serialize, Deserialize)]
// pub struct InactHnr {
//     pub count: u64,
//     pub red: bool,
//     pub size: Option<u64>,
// }
//
// #[derive(Debug, Serialize, Deserialize)]
// pub struct Ite {
//     pub count: u64,
//     pub latest: u64,
// }
//
// #[derive(Debug, Serialize, Deserialize)]
// pub struct Reseed {
//     pub count: u64,
//     pub inactive: u64,
//     pub red: bool,
// }

#[cfg(test)]
mod tests {
    use super::*;

    const UNSAT: &str = r#"{"count":3,"red":false,"size":null,"limit":50}"#;

    fn user_json(extra: &str) -> String {
        format!(
            r#"{{"uid":1,"username":"u","downloaded_bytes":1.0,"uploaded_bytes":2.0,"seedbonus":3,"wedges":4,{extra}}}"#
        )
    }

    #[test]
    fn unsat_at_top_level() {
        let user: UserResponse =
            serde_json::from_str(&user_json(&format!(r#""unsat":{UNSAT}"#))).unwrap();
        assert_eq!(user.unsat.limit, 50);
    }

    #[test]
    fn unsat_in_snatch_summary() {
        let user: UserResponse = serde_json::from_str(&user_json(&format!(
            r#""unsat":null,"snatch_summary":{{"unsat":{{"name":"Unsatisfied","count":3,"limit":50,"size":null}}}}"#
        )))
        .unwrap();
        assert_eq!(user.unsat.count, 3);
    }

    #[test]
    fn unsat_missing() {
        assert!(serde_json::from_str::<UserResponse>(&user_json(r#""unsat":null"#)).is_err());
    }
}
