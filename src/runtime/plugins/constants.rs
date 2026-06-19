use color_eyre::eyre;
use strum::IntoEnumIterator;

use crate::runtime::{
    network_replicator::protocol::AreaShape,
    plugins::input::protocol::{
        ControllerButton, ControllerStick, InputSource, KeyboardKey, MouseKey,
    },
};

use super::protocol::{GameModePlugin, PluginName};

pub(crate) struct ConstantsPlugin {}
impl ConstantsPlugin {
    fn create_string_table(
        &self,
        lua: &mlua::Lua,
        values: &[(&str, &str)],
    ) -> mlua::Result<mlua::Table> {
        let table = lua.create_table()?;
        for (name, value) in values {
            table.set(*name, *value)?;
        }
        Ok(table)
    }
    fn create_keys_table<T>(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Table>
    where
        T: IntoEnumIterator + AsRef<str> + Into<u8>,
    {
        let table = lua.create_table()?;

        for variant in T::iter() {
            let name = variant.as_ref().to_owned();
            let value: u8 = variant.into();
            table.set(name, value)?;
        }

        Ok(table)
    }
}
impl GameModePlugin for ConstantsPlugin {
    fn name(&self) -> PluginName {
        PluginName::Constants
    }

    fn create_api(&self, lua: &mlua::Lua) -> mlua::Result<Option<mlua::Table>> {
        let table = lua.create_table()?;

        let area_shape = lua.create_table()?;
        for shape in AreaShape::iter() {
            let shape = shape.as_ref();
            area_shape.set(shape, shape)?;
        }
        table.set("AreaShape", area_shape)?;

        let keys_table = lua.create_table()?;
        keys_table.set(
            InputSource::Keyboard.as_ref(),
            self.create_keys_table::<KeyboardKey>(lua)?,
        )?;
        keys_table.set(
            InputSource::Mouse.as_ref(),
            self.create_keys_table::<MouseKey>(lua)?,
        )?;
        keys_table.set(
            InputSource::Controller.as_ref(),
            self.create_keys_table::<ControllerButton>(lua)?,
        )?;
        keys_table.set(
            InputSource::Stick.as_ref(),
            self.create_keys_table::<ControllerStick>(lua)?,
        )?;
        table.set("Input", keys_table)?;

        table.set(
            "CastSort",
            self.create_string_table(
                lua,
                &[
                    ("Trending", "trending"),
                    ("Likes", "likes"),
                    ("Recasts", "recasts"),
                    ("Replies", "replies"),
                    ("Recent", "recent"),
                ],
            )?,
        )?;
        table.set(
            "NotificationKind",
            self.create_string_table(
                lua,
                &[
                    ("Follows", "follows"),
                    ("Recasts", "recasts"),
                    ("Likes", "likes"),
                    ("Mentions", "mentions"),
                    ("Replies", "replies"),
                    ("Quotes", "quotes"),
                ],
            )?,
        )?;
        table.set(
            "ReactionFilter",
            self.create_string_table(
                lua,
                &[("All", "all"), ("Likes", "likes"), ("Recasts", "recasts")],
            )?,
        )?;
        table.set(
            "FollowSort",
            self.create_string_table(
                lua,
                &[("DescChron", "desc_chron"), ("Algorithmic", "algorithmic")],
            )?,
        )?;
        table.set(
            "RepliesAndRecastsFilter",
            self.create_string_table(
                lua,
                &[
                    ("All", "all"),
                    ("Replies", "replies"),
                    ("Recasts", "recasts"),
                ],
            )?,
        )?;
        table.set(
            "ChannelIdentifier",
            self.create_string_table(lua, &[("Id", "id"), ("ParentUrl", "parent_url")])?,
        )?;
        table.set(
            "ChannelTrendingWindow",
            self.create_string_table(
                lua,
                &[("OneDay", "1d"), ("SevenDays", "7d"), ("ThirtyDays", "30d")],
            )?,
        )?;
        table.set(
            "FeedTrendingWindow",
            self.create_string_table(
                lua,
                &[
                    ("OneHour", "1h"),
                    ("SixHours", "6h"),
                    ("TwelveHours", "12h"),
                    ("TwentyFourHours", "24h"),
                    ("SevenDays", "7d"),
                ],
            )?,
        )?;
        table.set(
            "FeedFilter",
            self.create_string_table(
                lua,
                &[
                    ("Fids", "fids"),
                    ("ParentUrl", "parent_url"),
                    ("ChannelId", "channel_id"),
                    ("EmbedUrl", "embed_url"),
                    ("EmbedTypes", "embed_types"),
                    ("GlobalTrending", "global_trending"),
                ],
            )?,
        )?;
        table.set(
            "EmbedType",
            self.create_string_table(
                lua,
                &[
                    ("Text", "text"),
                    ("Image", "image"),
                    ("Video", "video"),
                    ("Audio", "audio"),
                    ("TextHtml", "text/html"),
                    ("TextPlain", "text/plain"),
                    ("ImageJpeg", "image/jpeg"),
                    ("ImagePng", "image/png"),
                    ("ImageGif", "image/gif"),
                    ("ImageWebp", "image/webp"),
                    ("ImageSvgXml", "image/svg+xml"),
                    ("ImageHeif", "image/heif"),
                    ("VideoMp4", "video/mp4"),
                    ("VideoQuicktime", "video/quicktime"),
                    ("AudioMpeg", "audio/mpeg"),
                    ("ApplicationPdf", "application/pdf"),
                    ("ApplicationJson", "application/json"),
                    ("ApplicationXMpegurl", "application/x-mpegurl"),
                ],
            )?,
        )?;
        table.set(
            "ReactionKind",
            self.create_string_table(lua, &[("Like", "like"), ("Recast", "recast")])?,
        )?;

        Ok(Some(table))
    }

    fn handle_op(
        &self,
        _: &mlua::Lua,
        _: &str,
        _: mlua::Value,
    ) -> eyre::Result<Option<super::protocol::AsyncTask>> {
        Ok(None)
    }
}
