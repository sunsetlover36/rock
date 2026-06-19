use color_eyre::eyre::{self, Context};
use rock_wire::farcaster::SignerStatus;

use crate::{
    meta_db::{MetaDb, MetaDbError},
    runtime::plugins::{
        farcaster::{
            protocol::{SignerGetOptions, SignerRequestOptions, StoredSigner},
            register_and_store_signer,
        },
        protocol::{AsyncTask, AsyncTaskResult},
    },
};

use super::{super::FarcasterOp, DispatchContext};

fn with_storage_context<T>(result: Result<T, MetaDbError>, message: &str) -> eyre::Result<T> {
    result.map_err(|error| {
        let error: eyre::Report = error.into();
        eyre::eyre!("{message}: {error}")
    })
}

pub(crate) fn handle(
    ctx: &DispatchContext,
    lua: &mlua::Lua,
    op: FarcasterOp,
    args: mlua::Value,
) -> eyre::Result<Option<AsyncTask>> {
    match op {
        FarcasterOp::SignerRequest => {
            let opts: SignerRequestOptions = ctx.parse(lua, args, "signer", "request")?;
            let fc_api = ctx.fc_api.clone();
            let meta_db = ctx.meta_db.clone();
            let storage_error = ctx.storage_error("signer", "request");
            let request_error = ctx.request_error("signer", "request");
            Ok(Some(Box::pin(async move {
                let key = MetaDb::farcaster_signer_key(opts.app_fid, opts.player_fid);
                let (existing, _) =
                    with_storage_context(meta_db.get_or_ensure_key(&key).await, &storage_error)?;
                if !existing.is_null() {
                    let stored: StoredSigner =
                        serde_json::from_value(existing.clone()).wrap_err(storage_error.clone())?;
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
                            .await
                            .wrap_err(request_error)?;
                            return Ok(AsyncTaskResult::JsonValue(value));
                        }
                        SignerStatus::Revoked => {}
                    }
                }

                let signer = fc_api
                    .create_signer()
                    .await
                    .wrap_err(request_error.clone())?;
                let generated_value = serde_json::to_value(StoredSigner {
                    app_fid: opts.app_fid,
                    player_fid: opts.player_fid,
                    signer: signer.clone(),
                })
                .wrap_err(storage_error.clone())?;
                with_storage_context(
                    meta_db
                        .update_key(&key, Some(generated_value.clone()))
                        .await,
                    &storage_error,
                )?;
                let value = register_and_store_signer(&fc_api, &meta_db, &key, opts, signer)
                    .await
                    .wrap_err(request_error)?;
                Ok(AsyncTaskResult::JsonValue(value))
            })))
        }
        FarcasterOp::SignerGet | FarcasterOp::SignerRefresh => {
            let method = if matches!(op, FarcasterOp::SignerGet) {
                "get"
            } else {
                "refresh"
            };
            let SignerGetOptions {
                app_fid,
                player_fid,
            } = ctx.parse(lua, args, "signer", method)?;
            let fc_api = ctx.fc_api.clone();
            let meta_db = ctx.meta_db.clone();
            let storage_error = ctx.storage_error("signer", method);
            let request_error = ctx.request_error("signer", method);
            Ok(Some(Box::pin(async move {
                let key = MetaDb::farcaster_signer_key(app_fid, player_fid);
                let (existing, _) =
                    with_storage_context(meta_db.get_or_ensure_key(&key).await, &storage_error)?;
                if existing.is_null() {
                    return Ok(AsyncTaskResult::Nil);
                }
                if matches!(op, FarcasterOp::SignerGet) {
                    return Ok(AsyncTaskResult::JsonValue(existing));
                }

                let mut stored: StoredSigner =
                    serde_json::from_value(existing).wrap_err(storage_error.clone())?;
                let signer = fc_api
                    .lookup_signer(&stored.signer.signer_uuid)
                    .await
                    .wrap_err(request_error)?;
                stored.signer = signer;
                let value = serde_json::to_value(stored).wrap_err(storage_error.clone())?;
                with_storage_context(
                    meta_db.update_key(&key, Some(value.clone())).await,
                    &storage_error,
                )?;
                Ok(AsyncTaskResult::JsonValue(value))
            })))
        }
        _ => Err(eyre::eyre!(
            "internal Farcaster error: signer dispatch received {op}"
        )),
    }
}
