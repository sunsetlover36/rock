use serde::{Deserialize, Serialize};

// ---- Responses
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub created_at: u64,
    pub event: WebhookEvent,
}
impl<'de> Deserialize<'de> for WebhookPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawPayload {
            created_at: u64,
            #[serde(rename = "type")]
            event_type: String,
            data: serde_json::Value,
        }

        let raw = RawPayload::deserialize(deserializer)?;
        let event = match raw.event_type.as_str() {
            "cast.created" => {
                let data: CastCreatedData =
                    serde_json::from_value(raw.data).map_err(serde::de::Error::custom)?;
                WebhookEvent::CastCreated(data)
            }
            other => {
                return Err(serde::de::Error::custom(format!(
                    "unknown event type: {other}"
                )));
            }
        };

        Ok(WebhookPayload {
            created_at: raw.created_at,
            event,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebhookEvent {
    #[serde(rename = "cast.created")]
    CastCreated(CastCreatedData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastCreatedData {
    pub object: String,
    pub hash: String,
    pub author: User,
    pub app: UserDehydrated,
    pub thread_hash: String,

    #[serde(default)]
    pub parent_hash: Option<String>,

    #[serde(default)]
    pub parent_url: Option<String>,

    #[serde(default)]
    pub root_parent_url: Option<String>,

    pub parent_author: Author,
    pub text: String,
    pub timestamp: String,
    pub embeds: Vec<serde_json::Value>,
    pub channel: Option<serde_json::Value>,
    pub reactions: Reactions,
    pub replies: Replies,
    pub mentioned_profiles: Vec<User>,
    pub mentioned_profiles_ranges: Vec<Range>,
    pub mentioned_channels: Vec<ChannelDehydrated>,
    pub mentioned_channels_ranges: Vec<Range>,
    pub event_timestamp: String,
}

pub type Fid = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Author {
    #[serde(default)]
    pub fid: Option<Fid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reactions {
    pub likes_count: u64,
    pub recasts_count: u64,
    pub likes: Vec<serde_json::Value>,
    pub recasts: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replies {
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub object: String,
    pub fid: Fid,
    pub username: String,
    pub display_name: String,
    pub pfp_url: String,
    pub custody_address: String,
    pub registered_at: String,

    #[serde(default)]
    pub pro: Option<Pro>,

    pub profile: Profile,
    pub follower_count: u64,
    pub following_count: u64,
    pub verifications: Vec<String>,
    pub verified_addresses: VerifiedAddresses,
    pub auth_addresses: Vec<AuthAddress>,
    pub verified_accounts: Vec<VerifiedAccount>,
    pub url: String,

    #[serde(default)]
    pub experimental: Option<Experimental>,

    #[serde(default)]
    pub viewer_context: Option<ViewerContext>,

    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDehydrated {
    pub object: String,
    pub fid: Fid,

    #[serde(default)]
    pub username: Option<String>,

    #[serde(default)]
    pub display_name: Option<String>,

    #[serde(default)]
    pub pfp_url: Option<String>,

    #[serde(default)]
    pub custody_address: Option<String>,

    #[serde(default)]
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pro {
    pub status: String,
    pub subscribed_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub bio: Bio,

    #[serde(default)]
    pub location: Option<Location>,

    #[serde(default)]
    pub banner: Option<Banner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bio {
    pub text: String,

    #[serde(default)]
    pub mentioned_profiles: Vec<UserDehydrated>,

    #[serde(default)]
    pub mentioned_profiles_ranges: Vec<Range>,

    #[serde(default)]
    pub mentioned_channels: Vec<ChannelDehydrated>,

    #[serde(default)]
    pub mentioned_channels_ranges: Vec<Range>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelDehydrated {
    pub object: String,
    pub id: String,
    pub name: String,
    pub image_url: String,

    #[serde(default)]
    pub viewer_context: Option<ChannelViewerContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub address: Address,

    #[serde(default)]
    pub radius: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub city: String,
    pub state: String,
    pub state_code: String,
    pub country: String,
    pub country_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Banner {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedAddresses {
    pub eth_addresses: Vec<String>,
    pub sol_addresses: Vec<String>,
    pub primary: PrimaryAddresses,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimaryAddresses {
    #[serde(default)]
    pub eth_address: Option<String>,

    #[serde(default)]
    pub sol_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthAddress {
    pub address: String,
    pub app: UserDehydrated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedAccount {
    pub platform: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experimental {
    pub neynar_user_score: f64,
    pub deprecation_notice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedCast {
    pub hash: String,
    pub author: Author,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerContext {
    pub following: bool,
    pub followed_by: bool,
    pub blocking: bool,
    pub blocked_by: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelViewerContext {
    pub following: bool,
    pub role: String,
}

// -- Send cast response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendCastResponse {
    pub success: bool,
    pub cast: CreatedCast,
}
// --

// -- Get user by username response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUserByUsernameResponse {
    pub user: User,
}
// --

// -- Get users by FIDs response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUsersByFidsResponse {
    pub users: Vec<User>,
}
// --
// ----

// ---- Requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendCastParams {
    pub signer_uuid: String,
    pub text: String,
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUserByUsernameParams {
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUsersByFidsParams {
    pub fids: Vec<Fid>,
}

// -- Strange Neynar API format for FIDs -> "1, 2, 3"
#[derive(Debug, Clone, Serialize)]
pub struct GetUsersByFidsRawQuery {
    pub fids: String,
}
impl From<&GetUsersByFidsParams> for GetUsersByFidsRawQuery {
    fn from(params: &GetUsersByFidsParams) -> Self {
        Self {
            fids: params
                .fids
                .iter()
                .map(|fid| fid.to_string())
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}
// ----
