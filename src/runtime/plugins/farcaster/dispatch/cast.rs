use color_eyre::eyre::{self, Context};
use rock_wire::farcaster::*;

use crate::runtime::plugins::{
    farcaster::{
        protocol::{DeleteCastOpParams, PublishReactionOpParams, SendCastOpParams, WriteAsOp},
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
            let params: $params = ctx.parse(lua, args, "cast", $method)?;
            let fc_api = ctx.fc_api.clone();
            let request_error = ctx.request_error("cast", $method);
            Ok(Some(Box::pin(async move {
                let response = fc_api.$call(&params).await.wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }};
    }

    match op {
        FarcasterOp::CastGet => read_op!(GetCastParams, get_cast, "get"),
        FarcasterOp::CastBulkFetch => read_op!(BulkFetchCastsParams, bulk_fetch_casts, "bulk"),
        FarcasterOp::CastConvo => read_op!(GetCastConversationParams, get_convo, "convo"),
        FarcasterOp::CastReactions => read_op!(GetReactionsParams, get_reactions, "reactions"),
        FarcasterOp::CastSend => {
            let payload: WriteAsOp<SendCastOpParams> = ctx.parse(lua, args, "cast", "send")?;
            let fc_api = ctx.fc_api.clone();
            let meta_db = ctx.meta_db.clone();
            let client_api = ctx.client_api.clone();
            let default_app_fid = ctx.default_app_fid;
            let signer_error = ctx.signer_error("cast", "send");
            let request_error = ctx.request_error("cast", "send");
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
                let params = SendCastParams {
                    signer_uuid,
                    text: payload.params.text,
                    parent: payload.params.parent,
                    channel_id: payload.params.channel_id,
                    embeds: Some(payload.params.embeds),
                };
                let response = fc_api.send_cast(&params).await.wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }
        FarcasterOp::CastDelete => {
            let payload: WriteAsOp<DeleteCastOpParams> = ctx.parse(lua, args, "cast", "delete")?;
            let fc_api = ctx.fc_api.clone();
            let meta_db = ctx.meta_db.clone();
            let client_api = ctx.client_api.clone();
            let default_app_fid = ctx.default_app_fid;
            let signer_error = ctx.signer_error("cast", "delete");
            let request_error = ctx.request_error("cast", "delete");
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
                let params = DeleteCastParams {
                    signer_uuid,
                    target_hash: payload.params.target_hash,
                };
                let response = fc_api.delete_cast(&params).await.wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }
        FarcasterOp::CastReactionPublish | FarcasterOp::CastReactionDelete => {
            let method = if matches!(op, FarcasterOp::CastReactionPublish) {
                "publish_reaction"
            } else {
                "delete_reaction"
            };
            let payload: WriteAsOp<PublishReactionOpParams> =
                ctx.parse(lua, args, "cast", method)?;
            let fc_api = ctx.fc_api.clone();
            let meta_db = ctx.meta_db.clone();
            let client_api = ctx.client_api.clone();
            let default_app_fid = ctx.default_app_fid;
            let signer_error = ctx.signer_error("cast", method);
            let request_error = ctx.request_error("cast", method);
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
                let params = PublishReactionParams {
                    signer_uuid,
                    reaction_type: payload.params.reaction_type,
                    target: payload.params.target,
                    idem: None,
                    target_author_fid: None,
                };
                let response = if matches!(op, FarcasterOp::CastReactionPublish) {
                    fc_api.publish_reaction(&params).await
                } else {
                    fc_api.delete_reaction(&params).await
                }
                .wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }
        _ => Err(eyre::eyre!(
            "internal Farcaster error: cast dispatch received {op}"
        )),
    }
}
