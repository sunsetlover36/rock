use color_eyre::eyre::{self, Context};
use rock_wire::farcaster::*;

use crate::runtime::plugins::{
    farcaster::{
        protocol::{FollowChannelOpParams, WriteAsOp},
        resolve_write_signer,
    },
    protocol::{AsyncTask, AsyncTaskResult},
};

use super::{super::FarcasterOp, DispatchContext};

pub(crate) fn handle(
    ctx: &DispatchContext,
    lua: &mlua::Lua,
    op: FarcasterOp,
    args: mlua::Value,
) -> eyre::Result<Option<AsyncTask>> {
    macro_rules! read_op {
        ($params:ty, $call:ident, $method:literal) => {{
            let params: $params = ctx.parse(lua, args, "channel", $method)?;
            let fc_api = ctx.fc_api.clone();
            let request_error = ctx.request_error("channel", $method);
            let future = Box::pin(async move {
                let response = fc_api.$call(&params).await.wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            });
            Ok(Some(future))
        }};
    }

    match op {
        FarcasterOp::ChannelList => read_op!(ListChannelsParams, list_channels, "list"),
        FarcasterOp::ChannelSearch => read_op!(SearchChannelsParams, search_channels, "search"),
        FarcasterOp::ChannelBulk => read_op!(BulkChannelsParams, bulk_channels, "bulk"),
        FarcasterOp::ChannelGet => read_op!(GetChannelParams, get_channel, "get"),
        FarcasterOp::ChannelMembers => {
            read_op!(GetChannelMembersParams, get_channel_members, "members")
        }
        FarcasterOp::ChannelFollowers => read_op!(
            GetChannelFollowersParams,
            get_channel_followers,
            "followers"
        ),
        FarcasterOp::ChannelRelevantFollowers => read_op!(
            GetRelevantChannelFollowersParams,
            get_relevant_channel_followers,
            "relevant_followers"
        ),
        FarcasterOp::ChannelTrending => {
            read_op!(GetTrendingChannelsParams, get_trending_channels, "trending")
        }
        FarcasterOp::ChannelFollow | FarcasterOp::ChannelUnfollow => {
            let method = if matches!(op, FarcasterOp::ChannelFollow) {
                "follow"
            } else {
                "unfollow"
            };
            let payload: WriteAsOp<FollowChannelOpParams> =
                ctx.parse(lua, args, "channel", method)?;
            let fc_api = ctx.fc_api.clone();
            let meta_db = ctx.meta_db.clone();
            let client_api = ctx.client_api.clone();
            let default_app_fid = ctx.default_app_fid;
            let signer_error = ctx.signer_error("channel", method);
            let request_error = ctx.request_error("channel", method);
            Ok(Some(Box::pin(async move {
                let signer_uuid = resolve_write_signer(
                    &meta_db,
                    client_api.as_ref(),
                    default_app_fid,
                    payload.pk(),
                    payload.write_args,
                )
                .await
                .wrap_err(signer_error)?;
                let params = FollowChannelParams {
                    channel_id: payload.params.channel_id,
                    signer_uuid,
                };
                let response = if matches!(op, FarcasterOp::ChannelFollow) {
                    fc_api.follow_channel(&params).await
                } else {
                    fc_api.unfollow_channel(&params).await
                }
                .wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }
        _ => Err(eyre::eyre!(
            "internal Farcaster error: channel dispatch received {op}"
        )),
    }
}
