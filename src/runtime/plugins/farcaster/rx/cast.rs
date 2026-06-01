use std::str::FromStr;

use mlua::{LuaSerdeExt, UserData};
use rock_wire::farcaster::{
    BulkFetchCastsParams, CastConversationOptions, CastSortKind, GetCastConversationParams,
    GetCastParams, GetReactionsOptions, GetReactionsParams, ReactionFilter, ReactionKind,
};

use crate::{
    runtime::plugins::{
        farcaster::protocol::{
            CastGetOptions, CastIdentifier, DeleteCastOpParams, PublishReactionOpParams,
            SendCastOpParams, WriteAsArgs, WriteAsOp,
        },
        player::PlayerHandle,
    },
    rx::CursorRx,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum CastRxOpcodeKey {
    Get,
    BulkFetch,
    Convo,
    Reactions,
    Send,
    PublishReaction,
    DeleteReaction,
    Delete,
}

#[derive(Clone)]
pub(crate) struct CastRxOpcodes {
    pub get: String,
    pub bulk_fetch: String,
    pub convo: String,
    pub reactions: String,
    pub send: String,
    pub publish_reaction: String,
    pub delete_reaction: String,
    pub delete: String,
}
impl CastRxOpcodes {
    pub fn get(&self, key: CastRxOpcodeKey) -> &str {
        match key {
            CastRxOpcodeKey::Get => &self.get,
            CastRxOpcodeKey::BulkFetch => &self.bulk_fetch,
            CastRxOpcodeKey::Convo => &self.convo,
            CastRxOpcodeKey::Reactions => &self.reactions,
            CastRxOpcodeKey::Send => &self.send,
            CastRxOpcodeKey::PublishReaction => &self.publish_reaction,
            CastRxOpcodeKey::DeleteReaction => &self.delete_reaction,
            CastRxOpcodeKey::Delete => &self.delete,
        }
    }
}

pub(crate) struct CastRxParams {
    pub opcodes: CastRxOpcodes,
    pub ids: Vec<CastIdentifier>,
}

#[derive(Clone)]
pub(crate) struct CastRx {
    opcodes: CastRxOpcodes,
    text: Option<String>,
    reply_hash: Option<String>,
    channel_id: Option<String>,
    ids: Vec<CastIdentifier>,
}
impl CastRx {
    pub fn new(params: CastRxParams) -> mlua::Result<Self> {
        Ok(CastRx {
            opcodes: params.opcodes,
            text: None,
            reply_hash: None,
            channel_id: None,
            ids: params.ids,
        })
    }

    fn get_op(
        &self,
        lua: &mlua::Lua,
        op: CastRxOpcodeKey,
        args: &mlua::Value,
    ) -> mlua::Result<mlua::Table> {
        let table = lua.create_table()?;
        table.set("opcode", self.opcodes.get(op))?;
        table.set("args", args)?;
        Ok(table)
    }

    fn get_first_cast_id(&self) -> mlua::Result<CastIdentifier> {
        let id = self
            .ids
            .first()
            .ok_or_else(|| mlua::Error::runtime("cast convo: cast id was not specified"))?;
        if self.ids.len() > 1 {
            Err(mlua::Error::runtime(
                "cast convo: cannot specify more than one cast id",
            ))?
        } else {
            Ok(id.clone())
        }
    }

    fn get_reactions(
        &self,
        lua: &mlua::Lua,
        options: mlua::Value,
        kinds: Vec<ReactionFilter>,
    ) -> mlua::Result<CursorRx> {
        let options = match options {
            mlua::Value::Table(t) => {
                lua.from_value::<GetReactionsOptions>(mlua::Value::Table(t))?
            }
            mlua::Value::Nil => GetReactionsOptions::default(),
            _ => {
                return Err(mlua::Error::runtime(
                    "cast reactions: unknown argument type for options, expected a table",
                ));
            }
        };

        let id = self.get_first_cast_id()?;
        if !matches!(id, CastIdentifier::Hash(_)) {
            return Err(mlua::Error::runtime(
                "cast reactions: cannot get reactions, type of cast id is not hash",
            ));
        }

        let params = GetReactionsParams {
            hash: id.as_str().to_owned(),
            types: kinds,
            options,
        };
        let args = lua.to_value(&params)?;
        let op = self.get_op(lua, CastRxOpcodeKey::Reactions, &args)?;
        Ok(CursorRx::new(op))
    }

    async fn process_reaction(
        &self,
        lua: &mlua::Lua,
        ud: mlua::AnyUserData,
        write_args: Option<WriteAsArgs>,
        kind: ReactionKind,
        opcode_key: CastRxOpcodeKey,
    ) -> mlua::Result<mlua::Value> {
        let player = ud.borrow::<PlayerHandle>()?;

        let payload = WriteAsOp {
            pid: player.key().pack(),
            write_args: write_args.unwrap_or_default(),
            params: PublishReactionOpParams {
                reaction_type: kind,
                target: self.get_first_cast_id()?.as_str().to_owned(),
            },
        };
        let args = lua.to_value(&payload)?;
        let op = self.get_op(lua, opcode_key, &args)?;

        lua.yield_with::<mlua::Value>(op).await
    }
}

impl UserData for CastRx {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("get", async |lua, this, options: Option<CastGetOptions>| {
            let options = options.unwrap_or_default();

            let sort_type = match &options.sort_type {
                Some(s) => CastSortKind::from_str(s.as_ref())
                    .map_err(|_| mlua::Error::runtime("cast get: unknown sort type"))?,
                None => CastSortKind::Recent,
            };

            let op = if this.ids.len() > 1 {
                let ids: Vec<String> = this.ids.iter().map(|id| id.as_str().to_owned()).collect();

                let args = lua.to_value(&BulkFetchCastsParams {
                    casts: ids,
                    sort_type,
                })?;
                this.get_op(&lua, CastRxOpcodeKey::BulkFetch, &args)?
            } else {
                let raw_id = match this.ids.first() {
                    Some(id) => Ok(id.raw()),
                    None => Err(mlua::Error::runtime(
                        "cast get: cannot get casts if cast id was not present",
                    )),
                }?;

                let args = lua.to_value(&GetCastParams {
                    identifier: raw_id.id,
                    id_type: raw_id.kind,
                    viewer_fid: options.viewer_fid,
                })?;
                this.get_op(&lua, CastRxOpcodeKey::Get, &args)?
            };
            lua.yield_with::<mlua::Value>(op).await
        });

        methods.add_async_method("convo", async |lua, this, options: mlua::Value| {
            let options = match options {
                mlua::Value::Table(t) => {
                    lua.from_value::<CastConversationOptions>(mlua::Value::Table(t))?
                }
                mlua::Value::Nil => CastConversationOptions::default(),
                _ => {
                    return Err(mlua::Error::runtime(
                        "cast convo: unknown argument type for options, expected a table",
                    ));
                }
            };

            let id = this.get_first_cast_id()?;
            let raw_id = id.raw();
            let args = lua.to_value(&GetCastConversationParams::new(
                raw_id.id,
                raw_id.kind,
                options,
            ))?;
            let op = this.get_op(&lua, CastRxOpcodeKey::Convo, &args)?;
            Ok(CursorRx::new(op))
        });

        // -- reactions
        methods.add_method("reactions", |lua, this, options: mlua::Value| {
            this.get_reactions(lua, options, vec![ReactionFilter::All])
        });
        methods.add_method("likes", |lua, this, options: mlua::Value| {
            this.get_reactions(lua, options, vec![ReactionFilter::Likes])
        });
        methods.add_method("recasts", |lua, this, options: mlua::Value| {
            this.get_reactions(lua, options, vec![ReactionFilter::Recasts])
        });
        // --

        methods.add_method("text", |_, this, text: String| {
            let mut next = this.clone();
            next.text = Some(text);
            Ok(next)
        });

        methods.add_method("reply_to", |_, this, hash: String| {
            let mut next = this.clone();
            next.reply_hash = Some(hash);
            Ok(next)
        });

        methods.add_method("channel", |_, this, id: String| {
            let mut next = this.clone();
            next.channel_id = Some(id);
            Ok(next)
        });

        methods.add_async_method(
            "send_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                let player = ud.borrow::<PlayerHandle>()?;

                let text = this
                    .text
                    .clone()
                    .ok_or_else(|| mlua::Error::runtime("cast send: cannot send an empty cast"))?;

                let payload = WriteAsOp {
                    pid: player.key().pack(),
                    write_args: write_args.unwrap_or_default(),
                    params: SendCastOpParams {
                        text,
                        parent: this.reply_hash.clone(),
                        channel_id: this.channel_id.clone(),
                    },
                };
                let args = lua.to_value(&payload)?;
                let op = this.get_op(&lua, CastRxOpcodeKey::Send, &args)?;

                lua.yield_with::<mlua::Value>(op).await
            },
        );
        methods.add_async_method(
            "like_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                this.process_reaction(
                    &lua,
                    ud,
                    write_args,
                    ReactionKind::Like,
                    CastRxOpcodeKey::PublishReaction,
                )
                .await
            },
        );
        methods.add_async_method(
            "unlike_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                this.process_reaction(
                    &lua,
                    ud,
                    write_args,
                    ReactionKind::Like,
                    CastRxOpcodeKey::DeleteReaction,
                )
                .await
            },
        );
        methods.add_async_method(
            "recast_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                this.process_reaction(
                    &lua,
                    ud,
                    write_args,
                    ReactionKind::Recast,
                    CastRxOpcodeKey::PublishReaction,
                )
                .await
            },
        );
        methods.add_async_method(
            "unrecast_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                this.process_reaction(
                    &lua,
                    ud,
                    write_args,
                    ReactionKind::Recast,
                    CastRxOpcodeKey::DeleteReaction,
                )
                .await
            },
        );
        methods.add_async_method(
            "delete_as",
            async |lua, this, (ud, write_args): (mlua::AnyUserData, Option<WriteAsArgs>)| {
                let player = ud.borrow::<PlayerHandle>()?;

                let target_hash = match this.get_first_cast_id()? {
                    CastIdentifier::Hash(hash) => hash,
                    _ => {
                        return Err(mlua::Error::runtime(
                            "cast delete: expected cast hash, got url or missing cast id",
                        ));
                    }
                };

                let payload = WriteAsOp {
                    pid: player.key().pack(),
                    write_args: write_args.unwrap_or_default(),
                    params: DeleteCastOpParams {
                        target_hash: target_hash.as_str().to_owned(),
                    },
                };
                let args = lua.to_value(&payload)?;
                let op = this.get_op(&lua, CastRxOpcodeKey::Delete, &args)?;

                lua.yield_with::<mlua::Value>(op).await
            },
        );
    }
}
