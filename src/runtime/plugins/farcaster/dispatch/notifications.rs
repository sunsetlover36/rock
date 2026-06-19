use color_eyre::eyre::{self, Context};
use rock_wire::farcaster::{GetNotificationsParams, MarkNotificationsSeenParams};

use crate::runtime::plugins::{
    farcaster::{
        protocol::{MarkNotificationsSeenOpParams, WriteAsOp},
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
    match op {
        FarcasterOp::UserNotifications => {
            let params: GetNotificationsParams =
                ctx.parse(lua, args, "notifications", "get_for")?;
            let fc_api = ctx.fc_api.clone();
            let request_error = ctx.request_error("notifications", "get_for");
            Ok(Some(Box::pin(async move {
                let response = fc_api
                    .get_notifications(&params)
                    .await
                    .wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }
        FarcasterOp::UserNotificationsSeen => {
            let payload: WriteAsOp<MarkNotificationsSeenOpParams> =
                ctx.parse(lua, args, "notifications", "mark_seen_as")?;
            let fc_api = ctx.fc_api.clone();
            let meta_db = ctx.meta_db.clone();
            let client_api = ctx.client_api.clone();
            let default_app_fid = ctx.default_app_fid;
            let signer_error = ctx.signer_error("notifications", "mark_seen_as");
            let request_error = ctx.request_error("notifications", "mark_seen_as");
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
                let params = MarkNotificationsSeenParams {
                    signer_uuid,
                    kind: payload.params.kind,
                };
                let response = fc_api
                    .mark_notifications_seen(&params)
                    .await
                    .wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(serde_json::to_value(response)?))
            })))
        }
        _ => Err(eyre::eyre!(
            "internal Farcaster error: notifications dispatch received {op}"
        )),
    }
}
