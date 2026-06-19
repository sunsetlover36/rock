use color_eyre::eyre::{self, Context};
use rock_wire::farcaster::*;

use crate::runtime::plugins::{
    farcaster::{
        protocol::{FollowUserOpParams, WriteAsOp},
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
            let params: $params = ctx.parse(lua, args, "user", $method)?;
            let fc_api = ctx.fc_api.clone();
            let request_error = ctx.request_error("user", $method);
            Ok(Some(Box::pin(async move {
                let response = fc_api.$call(&params).await.wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }};
    }

    match op {
        FarcasterOp::UserGetByUsername => {
            read_op!(
                GetUserByUsernameParams,
                get_user_by_username,
                "get_by_username"
            )
        }
        FarcasterOp::UserGetByFids => {
            read_op!(GetUsersByFidsParams, get_users_by_fids, "get_by_fids")
        }
        FarcasterOp::UserSearchByUsername => {
            read_op!(SearchUsersParams, search_users, "search")
        }
        FarcasterOp::GetUserCasts => read_op!(GetUserCastsParams, get_user_casts, "casts"),
        FarcasterOp::UserFollowers => read_op!(GetFollowersParams, get_followers, "followers"),
        FarcasterOp::UserRelevantFollowers => read_op!(
            GetRelevantFollowersParams,
            get_relevant_followers,
            "relevant_followers"
        ),
        FarcasterOp::UserReciprocalFollowers => read_op!(
            GetReciprocalFollowersParams,
            get_reciprocal_followers,
            "reciprocal_followers"
        ),
        FarcasterOp::UserFollowing => read_op!(GetFollowingParams, get_following, "following"),
        FarcasterOp::UserFollowSuggestions => {
            read_op!(
                GetFollowSuggestionsParams,
                get_follow_suggestions,
                "suggested_follows"
            )
        }
        FarcasterOp::UserBestFriends => {
            read_op!(GetBestFriendsParams, get_best_friends, "best_friends")
        }
        FarcasterOp::UserChannels => read_op!(GetUserChannelsParams, get_user_channels, "channels"),
        FarcasterOp::UserActiveChannels => read_op!(
            GetUsersActiveChannelsParams,
            get_users_active_channels,
            "active_channels"
        ),
        FarcasterOp::UserChannelMemberships => read_op!(
            GetUserChannelMembershipsParams,
            get_user_channel_memberships,
            "channel_memberships"
        ),
        FarcasterOp::UserRepliesAndRecasts => read_op!(
            GetUserRepliesAndRecastsParams,
            get_user_replies_and_recasts,
            "replies_and_recasts"
        ),
        FarcasterOp::UserPopularCasts => {
            read_op!(
                GetUserPopularCastsParams,
                get_user_popular_casts,
                "popular_casts"
            )
        }
        FarcasterOp::UserReactions => {
            read_op!(GetUserReactionsParams, get_user_reactions, "reactions")
        }
        FarcasterOp::UserFollow | FarcasterOp::UserUnfollow => {
            let method = if matches!(op, FarcasterOp::UserFollow) {
                "follow"
            } else {
                "unfollow"
            };
            let payload: WriteAsOp<FollowUserOpParams> = ctx.parse(lua, args, "user", method)?;
            let fc_api = ctx.fc_api.clone();
            let meta_db = ctx.meta_db.clone();
            let client_api = ctx.client_api.clone();
            let default_app_fid = ctx.default_app_fid;
            let signer_error = ctx.signer_error("user", method);
            let request_error = ctx.request_error("user", method);
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
                let params = FollowUserParams {
                    signer_uuid,
                    target_fids: payload.params.target_fids,
                };
                let response = if matches!(op, FarcasterOp::UserFollow) {
                    fc_api.follow_user(&params).await
                } else {
                    fc_api.unfollow_user(&params).await
                }
                .wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }
        _ => Err(eyre::eyre!(
            "internal Farcaster error: user dispatch received {op}"
        )),
    }
}
