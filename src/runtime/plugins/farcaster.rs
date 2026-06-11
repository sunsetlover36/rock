use std::{str::FromStr, sync::Arc};

use color_eyre::eyre::{self, Context};
use mlua::LuaSerdeExt;
use rock_wire::{
    PlayerKey,
    farcaster::{
        BulkFetchCastsParams, DeleteCastParams, DeleteReactionParams, Fid, FollowUserParams,
        GetCastConversationParams, GetCastParams, GetFollowingFeedParams, GetForYouFeedParams,
        GetNotificationsParams, GetReactionsParams, GetUserByUsernameParams, GetUserCastsParams,
        GetUsersByFidsParams, PublishReactionParams, SearchUsersParams, SendCastParams,
        SignerResponse, SignerStatus, UnfollowUserParams,
    },
};
use strum::{AsRefStr, Display, EnumString};

use crate::{
    clients::{FarcasterApi, farcaster::RegisterSignedKeyOptions},
    config::FarcasterConfig,
    meta_db::MetaDb,
    runtime::{
        GameModeClientApi, LuaResultExt, app_data, get_app_data,
        plugins::{
            farcaster::{
                protocol::{
                    DeleteCastOpParams, DeleteReactionOpParams, FollowUserOpParams,
                    PublishReactionOpParams, SendCastOpParams, SignerGetOptions,
                    UnfollowUserOpParams, WriteAsArgs, WriteAsOp,
                },
                rx::{FeedRx, FeedRxOpcodes, FeedRxParams},
            },
            player::PlayerHandle,
            protocol::AsyncTaskResult,
        },
    },
};

use super::protocol::{AsyncTask, GameModePlugin, PluginName};

mod rx;
use rx::{
    CastRx, CastRxOpcodes, CastRxParams, SignerRx, SignerRxOpcodes, SignerRxParams, UserRx,
    UserRxOpcodes, UserRxParams,
};

mod protocol;
use protocol::{CastIdentifier, SignerRequestOptions, StoredSigner};

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
    SignerRequest,
    SignerGet,
    SignerRefresh,
    FeedForYou,
    FeedFollowing,
}

async fn register_and_store_signer(
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
        signer: signer.clone(),
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

async fn resolve_write_signer(
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

    fn create_global_api(&self, _: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        Ok(None)
    }

    fn create_scene_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let plugin_name = self.name().to_string();
        let name_in_uppercase = plugin_name.to_uppercase();
        let default_app_fid = self
            .config
            .as_ref()
            .and_then(|config| config.default_app_fid);

        let table = lua.create_table()?;

        let cast_opcodes = CastRxOpcodes {
            get: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastGet),
            bulk_fetch: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastBulkFetch),
            convo: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastConvo),
            reactions: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastReactions),
            send: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastSend),
            publish_reaction: format!(
                "{}_{}",
                &name_in_uppercase,
                FarcasterOp::CastReactionPublish
            ),
            delete_reaction: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastReactionDelete),
            delete: format!("{}_{}", &name_in_uppercase, FarcasterOp::CastDelete),
        };

        let cast_plugin_name = plugin_name.clone();
        let cast_fn = lua.create_function(move |_, v: mlua::Value| {
            let mut ids: Vec<CastIdentifier> = Vec::new();
            match v {
                mlua::Value::String(s) => {
                    ids.push(
                        CastIdentifier::try_from(s.to_string_lossy())
                            .map_err(mlua::Error::runtime)?,
                    );
                }
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
            get_by_username: format!("{}_{}", &name_in_uppercase, FarcasterOp::UserGetByUsername),
            get_by_fids: format!("{}_{}", &name_in_uppercase, FarcasterOp::UserGetByFids),
            search_by_username: format!(
                "{}_{}",
                &name_in_uppercase,
                FarcasterOp::UserSearchByUsername
            ),
            get_user_casts: format!("{}_{}", &name_in_uppercase, FarcasterOp::GetUserCasts),
            follow_user: format!("{}_{}", &name_in_uppercase, FarcasterOp::UserFollow),
            unfollow_user: format!("{}_{}", &name_in_uppercase, FarcasterOp::UserUnfollow),
            get_notifications: format!("{}_{}", &name_in_uppercase, FarcasterOp::UserNotifications),
        };

        let user_plugin_name = plugin_name.clone();
        let user_fn = lua.create_function(move |_, v: mlua::Value| {
            let mut username: Option<String> = None;
            let mut fids: Vec<Fid> = vec![];

            match v {
                mlua::Value::String(s) => {
                    username = Some(s.to_string_lossy());
                }
                mlua::Value::Integer(fid) => {
                    fids.push(fid as Fid);
                }
                mlua::Value::Table(arr) => {
                    if arr.is_empty() {
                        return Err(mlua::Error::runtime(format!(
                            "{user_plugin_name}.user: fids list is empty"
                        )));
                    }
                    for value in arr.sequence_values::<u64>() {
                        fids.push(value?);
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

        let signer_opcodes = SignerRxOpcodes {
            request: format!("{}_{}", &name_in_uppercase, FarcasterOp::SignerRequest),
            get: format!("{}_{}", &name_in_uppercase, FarcasterOp::SignerGet),
            refresh: format!("{}_{}", &name_in_uppercase, FarcasterOp::SignerRefresh),
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
            for_you: format!("{}_{}", &name_in_uppercase, FarcasterOp::FeedForYou),
            following: format!("{}_{}", &name_in_uppercase, FarcasterOp::FeedFollowing),
        };
        let feed_fn = lua.create_function(move |_, fid: Fid| {
            Ok(FeedRx::new(FeedRxParams {
                opcodes: feed_opcodes.clone(),
                fid,
            }))
        })?;
        table.set("feed", feed_fn)?;

        Ok(Some(table))
    }
    fn handle_op(
        &self,
        lua: &mlua::Lua,
        op: &str,
        args: mlua::Value,
    ) -> eyre::Result<Option<AsyncTask>> {
        let plugin_name = self.name();
        let default_app_fid = self
            .config
            .as_ref()
            .and_then(|config| config.default_app_fid);

        let fc_api = self.fc_api.clone();
        let meta_db = self.meta_db.clone();
        let client_api: Arc<dyn GameModeClientApi> = get_app_data::<app_data::ClientApi>(lua)
            .wrap_err("App data is not initialized")?
            .0
            .clone();

        let op = FarcasterOp::from_str(op)
            .wrap_err_with(|| format!("{plugin_name}: unknown plugin op `{op}`"))?;
        match op {
            FarcasterOp::CastGet => {
                let params: GetCastParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [get method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let cast = fc_api.get_cast(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(cast)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastBulkFetch => {
                let params: BulkFetchCastsParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [bulk fetch method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let casts = fc_api.bulk_fetch_casts(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(casts)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastConvo => {
                let params: GetCastConversationParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [convo method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let convo = fc_api.get_convo(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(convo)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastReactions => {
                let params: GetReactionsParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.cast [reactions method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let reactions = fc_api.get_reactions(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(reactions)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastSend => {
                let payload: WriteAsOp<SendCastOpParams> = lua.from_value(args).wrap_err(
                    &format!("{plugin_name}.cast [send method]: incorrect cast body"),
                )?;

                let future = Box::pin(async move {
                    let signer_uuid = resolve_write_signer(
                        &meta_db,
                        client_api.as_ref(),
                        default_app_fid,
                        payload.pk(),
                        payload.write_args,
                    )
                    .await?;
                    let params = SendCastParams {
                        signer_uuid,
                        text: payload.params.text,
                        parent: payload.params.parent,
                        channel_id: payload.params.channel_id,
                        embeds: Some(payload.params.embeds),
                    };
                    let cast = fc_api.send_cast(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(cast)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastDelete => {
                let payload: WriteAsOp<DeleteCastOpParams> = lua.from_value(args).wrap_err(
                    &format!("{plugin_name}.cast [delete method]: incorrect params"),
                )?;

                let future = Box::pin(async move {
                    let signer_uuid = resolve_write_signer(
                        &meta_db,
                        client_api.as_ref(),
                        default_app_fid,
                        payload.pk(),
                        payload.write_args,
                    )
                    .await?;
                    let params = DeleteCastParams {
                        signer_uuid,
                        target_hash: payload.params.target_hash,
                    };
                    let res = fc_api.delete_cast(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(res)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastReactionPublish => {
                let payload: WriteAsOp<PublishReactionOpParams> = lua.from_value(args).wrap_err(
                    &format!("{plugin_name}.cast [publish reaction method]: incorrect params"),
                )?;

                let future = Box::pin(async move {
                    let signer_uuid = resolve_write_signer(
                        &meta_db,
                        client_api.as_ref(),
                        default_app_fid,
                        payload.pk(),
                        payload.write_args,
                    )
                    .await?;
                    let params = PublishReactionParams {
                        signer_uuid,
                        reaction_type: payload.params.reaction_type,
                        target: payload.params.target,
                        idem: None,
                        target_author_fid: None,
                    };
                    let res = fc_api.publish_reaction(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(res)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::CastReactionDelete => {
                let payload: WriteAsOp<DeleteReactionOpParams> = lua.from_value(args).wrap_err(
                    &format!("{plugin_name}.cast [delete reaction method]: incorrect params"),
                )?;

                let future = Box::pin(async move {
                    let signer_uuid = resolve_write_signer(
                        &meta_db,
                        client_api.as_ref(),
                        default_app_fid,
                        payload.pk(),
                        payload.write_args,
                    )
                    .await?;
                    let params = DeleteReactionParams {
                        signer_uuid,
                        reaction_type: payload.params.reaction_type,
                        target: payload.params.target,
                        idem: None,
                        target_author_fid: None,
                    };
                    let res = fc_api.delete_reaction(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(res)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::UserGetByUsername => {
                let params: GetUserByUsernameParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.user [get method]: incorrect params for 'GetUserByUsername'"
                ))?;

                let future = Box::pin(async move {
                    let user = fc_api.get_user_by_username(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(user)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::UserGetByFids => {
                let params: GetUsersByFidsParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.user [get method]: incorrect params for 'GetUsersByFids'"
                ))?;

                let future = Box::pin(async move {
                    let users = fc_api.get_users_by_fids(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(users)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::UserSearchByUsername => {
                let params: SearchUsersParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.user [search method]: incorrect args for 'SearchUsersByUsername'"
                ))?;

                let future = Box::pin(async move {
                    let res = fc_api.search_users(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(res)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::GetUserCasts => {
                let params: GetUserCastsParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.user [get casts method]: incorrect args for 'GetUserCastsParams'"
                ))?;

                let future = Box::pin(async move {
                    let res = fc_api.get_user_casts(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(res)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::UserFollow => {
                let payload: WriteAsOp<FollowUserOpParams> = lua.from_value(args).wrap_err(
                    &format!("{plugin_name}.user [follow method]: incorrect params"),
                )?;

                let future = Box::pin(async move {
                    let signer_uuid = resolve_write_signer(
                        &meta_db,
                        client_api.as_ref(),
                        default_app_fid,
                        payload.pk(),
                        payload.write_args,
                    )
                    .await?;
                    let params = FollowUserParams {
                        signer_uuid,
                        target_fids: payload.params.target_fids,
                    };
                    let res = fc_api.follow_user(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(res)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::UserUnfollow => {
                let payload: WriteAsOp<UnfollowUserOpParams> = lua.from_value(args).wrap_err(
                    &format!("{plugin_name}.user [unfollow method]: incorrect params"),
                )?;

                let future = Box::pin(async move {
                    let signer_uuid = resolve_write_signer(
                        &meta_db,
                        client_api.as_ref(),
                        default_app_fid,
                        payload.pk(),
                        payload.write_args,
                    )
                    .await?;
                    let params = UnfollowUserParams {
                        signer_uuid,
                        target_fids: payload.params.target_fids,
                    };
                    let res = fc_api.unfollow_user(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(res)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::UserNotifications => {
                let params: GetNotificationsParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.user [notifications method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let res = fc_api.get_notifications(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(res)?))
                });
                Ok(Some(future))
            }

            FarcasterOp::SignerRequest => {
                let opts: SignerRequestOptions = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.signer [request method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let key = MetaDb::farcaster_signer_key(opts.app_fid, opts.player_fid);
                    let (existing, _) = meta_db.get_or_ensure_key(&key).await?;
                    if !existing.is_null() {
                        let stored: StoredSigner = serde_json::from_value(existing.clone())?;

                        match stored.signer.status {
                            SignerStatus::PendingApproval | SignerStatus::Approved => {
                                return Ok(AsyncTaskResult::JsonValue(existing));
                            }
                            SignerStatus::Generated => {
                                let value = register_and_store_signer(
                                    &fc_api,
                                    &meta_db,
                                    &key,
                                    opts,
                                    stored.signer,
                                )
                                .await?;
                                return Ok(AsyncTaskResult::JsonValue(value));
                            }
                            SignerStatus::Revoked => {}
                        }
                    }

                    let signer = fc_api.create_signer().await?;
                    let generated_value = serde_json::to_value(StoredSigner {
                        app_fid: opts.app_fid,
                        player_fid: opts.player_fid,
                        signer: signer.clone(),
                    })?;
                    meta_db
                        .update_key(&key, Some(generated_value.clone()))
                        .await?;

                    let value =
                        register_and_store_signer(&fc_api, &meta_db, &key, opts, signer).await?;
                    Ok(AsyncTaskResult::JsonValue(value))
                });

                Ok(Some(future))
            }
            FarcasterOp::SignerGet => {
                let SignerGetOptions {
                    app_fid,
                    player_fid,
                } = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.signer [get method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let key = MetaDb::farcaster_signer_key(app_fid, player_fid);
                    let (existing, _) = meta_db.get_or_ensure_key(&key).await?;

                    if existing.is_null() {
                        return Ok(AsyncTaskResult::Nil);
                    }

                    Ok(AsyncTaskResult::JsonValue(existing))
                });
                Ok(Some(future))
            }
            FarcasterOp::SignerRefresh => {
                let SignerGetOptions {
                    app_fid,
                    player_fid,
                } = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.signer [status method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let key = MetaDb::farcaster_signer_key(app_fid, player_fid);
                    let (existing, _) = meta_db.get_or_ensure_key(&key).await?;

                    if existing.is_null() {
                        return Ok(AsyncTaskResult::Nil);
                    }

                    let mut stored: StoredSigner = serde_json::from_value(existing)?;
                    let signer = fc_api.lookup_signer(&stored.signer.signer_uuid).await?;
                    stored.signer = signer;

                    let value = serde_json::to_value(stored)?;
                    meta_db.update_key(&key, Some(value.clone())).await?;

                    Ok(AsyncTaskResult::JsonValue(value))
                });
                Ok(Some(future))
            }
            FarcasterOp::FeedForYou => {
                let params: GetForYouFeedParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.feed [for you method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let feed = fc_api.get_for_you_feed(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(feed)?))
                });
                Ok(Some(future))
            }
            FarcasterOp::FeedFollowing => {
                let params: GetFollowingFeedParams = lua.from_value(args).wrap_err(&format!(
                    "{plugin_name}.feed [following method]: incorrect params"
                ))?;

                let future = Box::pin(async move {
                    let feed = fc_api.get_following_feed(&params).await?;
                    Ok(AsyncTaskResult::JsonValue(serde_json::to_value(feed)?))
                });
                Ok(Some(future))
            }
        }
    }
}
