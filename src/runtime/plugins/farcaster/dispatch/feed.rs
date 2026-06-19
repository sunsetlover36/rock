use color_eyre::eyre::{self, Context};
use rock_wire::farcaster::*;

use crate::runtime::plugins::protocol::{AsyncTask, AsyncTaskResult};

use super::{super::FarcasterOp, DispatchContext};

pub(crate) fn handle(
    ctx: &DispatchContext,
    lua: &mlua::Lua,
    op: FarcasterOp,
    args: mlua::Value,
) -> eyre::Result<Option<AsyncTask>> {
    macro_rules! read_op {
        ($params:ty, $call:ident, $method:literal) => {{
            let params: $params = ctx.parse(lua, args, "feed", $method)?;
            let fc_api = ctx.fc_api.clone();
            let request_error = ctx.request_error("feed", $method);
            Ok(Some(Box::pin(async move {
                let response = fc_api.$call(&params).await.wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }};
    }
    match op {
        FarcasterOp::FeedForYou => read_op!(GetForYouFeedParams, get_for_you_feed, "for_you"),
        FarcasterOp::FeedFollowing => {
            read_op!(GetFollowingFeedParams, get_following_feed, "following")
        }
        FarcasterOp::FeedByFilters => {
            read_op!(GetFeedByFiltersParams, get_feed_by_filters, "by_filters")
        }
        FarcasterOp::FeedTrending => read_op!(GetTrendingFeedParams, get_trending_feed, "trending"),
        FarcasterOp::FeedByChannelIds => {
            read_op!(
                GetFeedByChannelIdsParams,
                get_feed_by_channel_ids,
                "by_channel_ids"
            )
        }
        _ => Err(eyre::eyre!(
            "internal Farcaster error: feed dispatch received {op}"
        )),
    }
}
