#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_codec::FeedInfo;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    FeedInfo { feed_hash: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    FeedInfo { info: Option<FeedInfo> },
}

pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::FeedInfo { feed_hash } => Ok(Response::FeedInfo { info: kind::<transit_codec::Feed>().require(&feed_hash)?.info() }),
    }
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the feed capability has no bulk op of its own")))
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "gtfs",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_codec::Feed],
    request: Request,
    speaks: &[("feed", &[1])],
}
