use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context};
use rock_wire::{PlayerKey, farcaster::*};
use strum::{AsRefStr, Display, EnumString};

use crate::{
    clients::{FarcasterApi, farcaster::RegisterSignedKeyOptions},
    config::FarcasterConfig,
    meta_db::MetaDb,
    runtime::{
        GameModeClientApi, LuaResultExt, app_data, get_app_data,
        plugins::{
            farcaster::{
                protocol::{SignerRequestOptions, StoredSigner, WriteAsArgs, parse_lua_fid},
                rx::{FeedRx, FeedRxOpcodes, FeedRxParams},
            },
            player::PlayerHandle,
        },
    },
};

use super::protocol::{AsyncTask, GameModePlugin, PluginName};

mod rx;
use rx::{
    CastRx, CastRxOpcodes, CastRxParams, ChannelRx, ChannelRxOpcodes, ChannelRxParams,
    NotificationsRx, NotificationsRxOpcodes, SignerRx, SignerRxOpcodes, SignerRxParams, UserRx,
    UserRxOpcodes, UserRxParams,
};

mod protocol;
use protocol::CastIdentifier;

mod dispatch;

#[derive(Debug, Clone, Copy, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum FarcasterOp {
    CastGet,
    CastBulkFetch,
    CastConvo,
    CastReactions,
    CastSend,
    CastDelete,
    CastReactionPublish,
    CastReactionDelete,
    UserGetByUsername,
    UserGetByFids,
    UserSearchByUsername,
    GetUserCasts,
    UserFollow,
    UserUnfollow,
    UserNotifications,
    UserFollowers,
    UserRelevantFollowers,
    UserReciprocalFollowers,
    UserFollowing,
    UserFollowSuggestions,
    UserBestFriends,
    UserChannels,
    UserActiveChannels,
    UserChannelMemberships,
    UserRepliesAndRecasts,
    UserPopularCasts,
    UserReactions,
    UserNotificationsSeen,
    SignerRequest,
    SignerGet,
    SignerRefresh,
    FeedForYou,
    FeedFollowing,
    FeedByFilters,
    FeedTrending,
    FeedByChannelIds,
    ChannelList,
    ChannelSearch,
    ChannelBulk,
    ChannelGet,
    ChannelMembers,
    ChannelFollowers,
    ChannelRelevantFollowers,
    ChannelTrending,
    ChannelFollow,
    ChannelUnfollow,
}

pub(super) async fn register_and_store_signer(
    fc_api: &FarcasterApi,
    meta_db: &MetaDb,
    key: &str,
    opts: SignerRequestOptions,
    signer: SignerResponse,
) -> eyre::Result<serde_json::Value> {
    let signer = fc_api
        .register_signed_key(RegisterSignedKeyOptions {
            app_fid: opts.app_fid,
            deadline: opts.deadline,
            signer_uuid: signer.signer_uuid,
            public_key: signer.public_key,
            redirect_url: opts.redirect_url,
            sponsor: None,
        })
        .await?;
    let value = serde_json::to_value(StoredSigner {
        app_fid: opts.app_fid,
        player_fid: opts.player_fid,
        signer,
    })?;
    meta_db.update_key(key, Some(value.clone())).await?;
    Ok(value)
}

async fn require_approved_signer(
    meta_db: &MetaDb,
    app_fid: Fid,
    player_fid: Fid,
) -> eyre::Result<String> {
    let key = MetaDb::farcaster_signer_key(app_fid, player_fid);
    let (existing, _) = meta_db.get_or_ensure_key(&key).await?;
    if existing.is_null() {
        return Err(eyre::eyre!(
            "missing Farcaster signer for app_fid={} player_fid={}",
            app_fid,
            player_fid
        ));
    }
    let stored: StoredSigner = serde_json::from_value(existing)?;
    if stored.signer.status != SignerStatus::Approved {
        return Err(eyre::eyre!(
            "Farcaster signer is not approved for app_fid={} player_fid={} status={}",
            app_fid,
            player_fid,
            stored.signer.status.as_ref()
        ));
    }
    Ok(stored.signer.signer_uuid)
}

pub(super) async fn resolve_write_signer(
    meta_db: &MetaDb,
    client_api: &dyn GameModeClientApi,
    default_app_fid: Option<Fid>,
    pk: PlayerKey,
    write_args: WriteAsArgs,
) -> eyre::Result<String> {
    let app_fid = write_args
        .app_fid
        .or(default_app_fid)
        .ok_or_else(|| eyre::eyre!("missing app_fid and no default_app_fid configured"))?;
    let player_fid = client_api
        .fid(pk)
        .ok_or_else(|| eyre::eyre!("player fid is missing"))?;
    require_approved_signer(meta_db, app_fid, player_fid).await
}

pub(crate) struct FarcasterPlugin {
    pub fc_api: Arc<FarcasterApi>,
    pub meta_db: Arc<MetaDb>,
    pub config: Option<FarcasterConfig>,
}

impl GameModePlugin for FarcasterPlugin {
    fn name(&self) -> PluginName {
        PluginName::Farcaster
    }

    fn create_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let plugin_name = self.name().to_string();
        let name_in_uppercase = plugin_name.to_uppercase();
        let default_app_fid = self
            .config
            .as_ref()
            .and_then(|config| config.default_app_fid);
        let table = lua.create_table()?;

        let cast_opcodes = CastRxOpcodes {
            get: format!("{}_{}", name_in_uppercase, FarcasterOp::CastGet),
            bulk_fetch: format!("{}_{}", name_in_uppercase, FarcasterOp::CastBulkFetch),
            convo: format!("{}_{}", name_in_uppercase, FarcasterOp::CastConvo),
            reactions: format!("{}_{}", name_in_uppercase, FarcasterOp::CastReactions),
            send: format!("{}_{}", name_in_uppercase, FarcasterOp::CastSend),
            publish_reaction: format!("{}_{}", name_in_uppercase, FarcasterOp::CastReactionPublish),
            delete_reaction: format!("{}_{}", name_in_uppercase, FarcasterOp::CastReactionDelete),
            delete: format!("{}_{}", name_in_uppercase, FarcasterOp::CastDelete),
        };
        let cast_plugin_name = plugin_name.clone();
        let cast_fn = lua.create_function(move |_, v: mlua::Value| {
            let mut ids = Vec::new();
            match v {
                mlua::Value::String(s) => ids.push(
                    CastIdentifier::try_from(s.to_string_lossy()).map_err(mlua::Error::runtime)?,
                ),
                mlua::Value::Table(arr) => {
                    if arr.is_empty() {
                        return Err(mlua::Error::runtime(format!(
                            "{cast_plugin_name}.cast: cast ids list is empty"
                        )));
                    }
                    for value in arr.sequence_values::<String>() {
                        ids.push(CastIdentifier::try_from(value?).map_err(mlua::Error::runtime)?);
                    }
                }
                mlua::Value::Nil => {}
                _ => {
                    return Err(mlua::Error::runtime(format!(
                        "{cast_plugin_name}.cast: unknown argument type, expected a string, table or nil"
                    )));
                }
            }
            CastRx::new(CastRxParams {
                opcodes: cast_opcodes.clone(),
                ids,
            })
        })?;
        table.set("cast", cast_fn)?;

        let user_opcodes = UserRxOpcodes {
            get_by_username: format!("{}_{}", name_in_uppercase, FarcasterOp::UserGetByUsername),
            get_by_fids: format!("{}_{}", name_in_uppercase, FarcasterOp::UserGetByFids),
            search_by_username: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::UserSearchByUsername
            ),
            get_user_casts: format!("{}_{}", name_in_uppercase, FarcasterOp::GetUserCasts),
            follow_user: format!("{}_{}", name_in_uppercase, FarcasterOp::UserFollow),
            unfollow_user: format!("{}_{}", name_in_uppercase, FarcasterOp::UserUnfollow),
            followers: format!("{}_{}", name_in_uppercase, FarcasterOp::UserFollowers),
            relevant_followers: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::UserRelevantFollowers
            ),
            reciprocal_followers: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::UserReciprocalFollowers
            ),
            following: format!("{}_{}", name_in_uppercase, FarcasterOp::UserFollowing),
            follow_suggestions: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::UserFollowSuggestions
            ),
            best_friends: format!("{}_{}", name_in_uppercase, FarcasterOp::UserBestFriends),
            channels: format!("{}_{}", name_in_uppercase, FarcasterOp::UserChannels),
            active_channels: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::UserActiveChannels
            ),
            channel_memberships: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::UserChannelMemberships
            ),
            replies_and_recasts: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::UserRepliesAndRecasts
            ),
            popular_casts: format!("{}_{}", name_in_uppercase, FarcasterOp::UserPopularCasts),
            reactions: format!("{}_{}", name_in_uppercase, FarcasterOp::UserReactions),
        };
        let user_plugin_name = plugin_name.clone();
        let user_fn = lua.create_function(move |_, v: mlua::Value| {
            let mut username = None;
            let mut fids = Vec::new();
            match v {
                mlua::Value::String(s) => username = Some(s.to_string_lossy()),
                mlua::Value::Integer(fid) => fids.push(parse_lua_fid(fid, "user")?),
                mlua::Value::Table(arr) => {
                    if arr.is_empty() {
                        return Err(mlua::Error::runtime(format!(
                            "{user_plugin_name}.user: fids list is empty"
                        )));
                    }
                    for value in arr.sequence_values::<i64>() {
                        fids.push(parse_lua_fid(value?, "user")?);
                    }
                }
                _ => {
                    return Err(mlua::Error::runtime(format!(
                        "{user_plugin_name}.user: unknown argument type, expected a string or a table"
                    )));
                }
            }
            Ok(UserRx::new(UserRxParams {
                opcodes: user_opcodes.clone(),
                username,
                fids,
            }))
        })?;
        table.set("user", user_fn)?;

        let notifications_opcodes = NotificationsRxOpcodes {
            get: format!("{}_{}", name_in_uppercase, FarcasterOp::UserNotifications),
            mark_seen: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::UserNotificationsSeen
            ),
        };
        table.set(
            "notifications",
            lua.create_function(move |_, _: ()| {
                Ok(NotificationsRx::new(notifications_opcodes.clone()))
            })?,
        )?;

        let signer_opcodes = SignerRxOpcodes {
            request: format!("{}_{}", name_in_uppercase, FarcasterOp::SignerRequest),
            get: format!("{}_{}", name_in_uppercase, FarcasterOp::SignerGet),
            refresh: format!("{}_{}", name_in_uppercase, FarcasterOp::SignerRefresh),
        };
        let signer_fn = lua.create_function(move |_, ud: mlua::AnyUserData| {
            let player = ud.borrow::<PlayerHandle>()?;
            Ok(SignerRx::new(SignerRxParams {
                opcodes: signer_opcodes.clone(),
                pk: player.key(),
                default_app_fid,
            }))
        })?;
        table.set("signer", signer_fn)?;

        let feed_opcodes = FeedRxOpcodes {
            for_you: format!("{}_{}", name_in_uppercase, FarcasterOp::FeedForYou),
            following: format!("{}_{}", name_in_uppercase, FarcasterOp::FeedFollowing),
            by_filters: format!("{}_{}", name_in_uppercase, FarcasterOp::FeedByFilters),
            trending: format!("{}_{}", name_in_uppercase, FarcasterOp::FeedTrending),
            by_channel_ids: format!("{}_{}", name_in_uppercase, FarcasterOp::FeedByChannelIds),
        };
        table.set(
            "feed",
            lua.create_function(move |_, fid: Option<Fid>| {
                Ok(FeedRx::new(FeedRxParams {
                    opcodes: feed_opcodes.clone(),
                    fid,
                }))
            })?,
        )?;

        let channel_opcodes = ChannelRxOpcodes {
            list: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelList),
            search: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelSearch),
            bulk: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelBulk),
            get: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelGet),
            members: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelMembers),
            followers: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelFollowers),
            relevant_followers: format!(
                "{}_{}",
                name_in_uppercase,
                FarcasterOp::ChannelRelevantFollowers
            ),
            trending: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelTrending),
            follow: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelFollow),
            unfollow: format!("{}_{}", name_in_uppercase, FarcasterOp::ChannelUnfollow),
        };
        table.set(
            "channel",
            lua.create_function(move |_, id: Option<String>| {
                Ok(ChannelRx::new(ChannelRxParams {
                    opcodes: channel_opcodes.clone(),
                    id,
                }))
            })?,
        )?;

        Ok(Some(table))
    }

    fn handle_op(
        &self,
        lua: &mlua::Lua,
        op: &str,
        args: mlua::Value,
    ) -> eyre::Result<Option<AsyncTask>> {
        let plugin_name = self.name();
        let op = FarcasterOp::from_str(op)
            .wrap_err_with(|| format!("{plugin_name}: unknown plugin op `{op}`"))?;
        let default_app_fid = self
            .config
            .as_ref()
            .and_then(|config| config.default_app_fid);
        let client_api: Arc<dyn GameModeClientApi> = get_app_data::<app_data::ClientApi>(lua)
            .wrap_err("App data is not initialized")?
            .0
            .clone();
        let ctx = dispatch::DispatchContext {
            fc_api: self.fc_api.clone(),
            meta_db: self.meta_db.clone(),
            client_api,
            default_app_fid,
            plugin_name: plugin_name.to_string(),
        };

        match op {
            FarcasterOp::CastGet
            | FarcasterOp::CastBulkFetch
            | FarcasterOp::CastConvo
            | FarcasterOp::CastReactions
            | FarcasterOp::CastSend
            | FarcasterOp::CastDelete
            | FarcasterOp::CastReactionPublish
            | FarcasterOp::CastReactionDelete => dispatch::cast::handle(&ctx, lua, op, args),
            FarcasterOp::UserGetByUsername
            | FarcasterOp::UserGetByFids
            | FarcasterOp::UserSearchByUsername
            | FarcasterOp::GetUserCasts
            | FarcasterOp::UserFollow
            | FarcasterOp::UserUnfollow
            | FarcasterOp::UserFollowers
            | FarcasterOp::UserRelevantFollowers
            | FarcasterOp::UserReciprocalFollowers
            | FarcasterOp::UserFollowing
            | FarcasterOp::UserFollowSuggestions
            | FarcasterOp::UserBestFriends
            | FarcasterOp::UserChannels
            | FarcasterOp::UserActiveChannels
            | FarcasterOp::UserChannelMemberships
            | FarcasterOp::UserRepliesAndRecasts
            | FarcasterOp::UserPopularCasts
            | FarcasterOp::UserReactions => dispatch::user::handle(&ctx, lua, op, args),
            FarcasterOp::UserNotifications | FarcasterOp::UserNotificationsSeen => {
                dispatch::notifications::handle(&ctx, lua, op, args)
            }
            FarcasterOp::ChannelList
            | FarcasterOp::ChannelSearch
            | FarcasterOp::ChannelBulk
            | FarcasterOp::ChannelGet
            | FarcasterOp::ChannelMembers
            | FarcasterOp::ChannelFollowers
            | FarcasterOp::ChannelRelevantFollowers
            | FarcasterOp::ChannelTrending
            | FarcasterOp::ChannelFollow
            | FarcasterOp::ChannelUnfollow => dispatch::channel::handle(&ctx, lua, op, args),
            FarcasterOp::SignerRequest | FarcasterOp::SignerGet | FarcasterOp::SignerRefresh => {
                dispatch::signer::handle(&ctx, lua, op, args)
            }
            FarcasterOp::FeedForYou
            | FarcasterOp::FeedFollowing
            | FarcasterOp::FeedByFilters
            | FarcasterOp::FeedTrending
            | FarcasterOp::FeedByChannelIds => dispatch::feed::handle(&ctx, lua, op, args),
        }
    }
}
