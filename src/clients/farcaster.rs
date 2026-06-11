use alloy::{
    primitives::{U256, address},
    signers::Signer,
    sol_types::eip712_domain,
};
use color_eyre::eyre;
use reqwest::{
    Client,
    header::{self, HeaderMap, HeaderName, HeaderValue},
};
use rock_wire::farcaster::{
    BulkFetchCastsParams, BulkFetchCastsRawQuery, BulkFetchCastsResponse, Cast,
    CastConversationResponse, CreatedCast, DeleteCastParams, DeleteCastResponse,
    DeleteReactionParams, Fid, FollowUserParams, FollowUserResponse, FollowingFeedResponse,
    ForYouFeedResponse, GetCastConversationParams, GetCastParams, GetCastResponse,
    GetFollowingFeedParams, GetForYouFeedParams, GetNotificationsParams, GetNotificationsRawQuery,
    GetReactionsParams, GetReactionsRawQuery, GetSignerStatusParams, GetUserByUsernameParams,
    GetUserByUsernameResponse, GetUserCastsParams, GetUsersByFidsParams, GetUsersByFidsRawQuery,
    GetUsersByFidsResponse, NotificationsResponse, PublishReactionParams, ReactionResponse,
    ReactionsResponse, RegisterSignedKeyParams, SearchUsersParams, SearchUsersResponse,
    SearchUsersResult, SendCastParams, SendCastResponse, SignedKeyRequestSponsor, SignerResponse,
    UnfollowUserParams, UnfollowUserResponse, User, UserCastsResponse,
};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    config::SignerConfig,
    crypto::{SignedKeyRequest, parse_hex_bytes, signer_from_config},
};

#[derive(Clone)]
pub(crate) struct RegisterSignedKeyOptions {
    pub app_fid: Fid,
    pub deadline: Option<u64>,
    pub signer_uuid: String,
    pub public_key: String,
    pub redirect_url: Option<String>,
    pub sponsor: Option<SignedKeyRequestSponsor>,
}

#[derive(Clone)]
pub(crate) struct FarcasterApi {
    client: Client,
    base_url: String,
    app_signers: HashMap<Fid, SignerConfig>,
}
impl FarcasterApi {
    pub fn new(api_key: &str, app_signers: HashMap<Fid, SignerConfig>) -> eyre::Result<Self> {
        let mut headers = HeaderMap::new();

        let mut auth = HeaderValue::from_str(api_key)?;
        auth.set_sensitive(true);
        headers.insert(HeaderName::from_static("x-api-key"), auth);

        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let client = Client::builder().default_headers(headers).build()?;
        Ok(Self {
            client,
            base_url: "https://api.neynar.com/v2".into(),
            app_signers,
        })
    }

    async fn sign_signed_key_req(
        &self,
        signer_cfg: &SignerConfig,
        app_fid: Fid,
        deadline: u64,
        public_key: &str,
    ) -> eyre::Result<String> {
        let signer = signer_from_config(signer_cfg)?;

        let domain = eip712_domain! {
            name: "Farcaster SignedKeyRequestValidator",
            version: "1",
            chain_id: 10u64,
            verifying_contract: address!("00000000FC700472606ED4fA22623Acf62c60553"),
        };
        let payload = SignedKeyRequest {
            requestFid: U256::from(app_fid),
            key: parse_hex_bytes(public_key)?,
            deadline: U256::from(deadline),
        };

        let sig = signer.sign_typed_data(&payload, &domain).await?;
        Ok(format!("0x{}", hex::encode(sig.as_bytes())))
    }

    pub async fn get_cast(&self, params: &GetCastParams) -> eyre::Result<Cast> {
        let url = format!("{}/farcaster/cast", self.base_url);

        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<GetCastResponse>()
            .await?;
        Ok(response.cast)
    }

    pub async fn bulk_fetch_casts(&self, params: &BulkFetchCastsParams) -> eyre::Result<Vec<Cast>> {
        let url = format!("{}/farcaster/casts", self.base_url);
        let query = BulkFetchCastsRawQuery::from(params);

        let response = self
            .client
            .get(url)
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<BulkFetchCastsResponse>()
            .await?;
        Ok(response.result.casts)
    }

    pub async fn get_convo(
        &self,
        params: &GetCastConversationParams,
    ) -> eyre::Result<CastConversationResponse> {
        let url = format!("{}/farcaster/cast/conversation", self.base_url);

        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<CastConversationResponse>()
            .await?;
        Ok(response)
    }

    pub async fn get_reactions(
        &self,
        params: &GetReactionsParams,
    ) -> eyre::Result<ReactionsResponse> {
        let url = format!("{}/farcaster/reactions/cast", self.base_url);
        let query = GetReactionsRawQuery::from(params);

        let response = self
            .client
            .get(url)
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<ReactionsResponse>()
            .await?;
        Ok(response)
    }

    pub async fn send_cast(&self, params: &SendCastParams) -> eyre::Result<CreatedCast> {
        let url = format!("{}/farcaster/cast", self.base_url);

        let response = self
            .client
            .post(url)
            .json(params)
            .send()
            .await?
            .error_for_status()?
            .json::<SendCastResponse>()
            .await?;
        Ok(response.cast)
    }
    pub async fn delete_cast(&self, params: &DeleteCastParams) -> eyre::Result<DeleteCastResponse> {
        let url = format!("{}/farcaster/cast", self.base_url);

        let response = self
            .client
            .delete(url)
            .json(params)
            .send()
            .await?
            .error_for_status()?
            .json::<DeleteCastResponse>()
            .await?;
        Ok(response)
    }

    pub async fn publish_reaction(
        &self,
        params: &PublishReactionParams,
    ) -> eyre::Result<ReactionResponse> {
        let url = format!("{}/farcaster/reaction", self.base_url);

        let response = self
            .client
            .post(url)
            .json(params)
            .send()
            .await?
            .error_for_status()?
            .json::<ReactionResponse>()
            .await?;
        Ok(response)
    }
    pub async fn delete_reaction(
        &self,
        params: &DeleteReactionParams,
    ) -> eyre::Result<ReactionResponse> {
        let url = format!("{}/farcaster/reaction", self.base_url);

        let response = self
            .client
            .delete(url)
            .json(params)
            .send()
            .await?
            .error_for_status()?
            .json::<ReactionResponse>()
            .await?;
        Ok(response)
    }

    pub async fn get_user_by_username(
        &self,
        params: &GetUserByUsernameParams,
    ) -> eyre::Result<User> {
        let url = format!("{}/farcaster/user/by_username", self.base_url);

        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<GetUserByUsernameResponse>()
            .await?;
        Ok(response.user)
    }

    pub async fn get_users_by_fids(
        &self,
        params: &GetUsersByFidsParams,
    ) -> eyre::Result<Vec<User>> {
        let url = format!("{}/farcaster/user/bulk", self.base_url);
        let query = GetUsersByFidsRawQuery::from(params);

        let response = self
            .client
            .get(url)
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<GetUsersByFidsResponse>()
            .await?;
        Ok(response.users)
    }

    pub async fn search_users(
        &self,
        params: &SearchUsersParams,
    ) -> eyre::Result<SearchUsersResult> {
        let url = format!("{}/farcaster/user/search", self.base_url);

        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<SearchUsersResponse>()
            .await?;

        Ok(response.result)
    }

    pub async fn get_user_casts(
        &self,
        params: &GetUserCastsParams,
    ) -> eyre::Result<UserCastsResponse> {
        let url = format!("{}/farcaster/feed/user/casts", self.base_url);

        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<UserCastsResponse>()
            .await?;

        Ok(response)
    }

    pub async fn follow_user(&self, params: &FollowUserParams) -> eyre::Result<FollowUserResponse> {
        let url = format!("{}/farcaster/user/follow", self.base_url);

        let response = self
            .client
            .post(url)
            .json(params)
            .send()
            .await?
            .error_for_status()?
            .json::<FollowUserResponse>()
            .await?;

        Ok(response)
    }
    pub async fn unfollow_user(
        &self,
        params: &UnfollowUserParams,
    ) -> eyre::Result<UnfollowUserResponse> {
        let url = format!("{}/farcaster/user/follow", self.base_url);

        let response = self
            .client
            .delete(url)
            .json(params)
            .send()
            .await?
            .error_for_status()?
            .json::<UnfollowUserResponse>()
            .await?;

        Ok(response)
    }

    pub async fn get_signer_status(
        &self,
        params: &GetSignerStatusParams,
    ) -> eyre::Result<SignerResponse> {
        let url = format!("{}/farcaster/signer", self.base_url);

        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<SignerResponse>()
            .await?;
        Ok(response)
    }
    pub async fn create_signer(&self) -> eyre::Result<SignerResponse> {
        let url = format!("{}/farcaster/signer", self.base_url);

        let response = self
            .client
            .post(url)
            .send()
            .await?
            .error_for_status()?
            .json::<SignerResponse>()
            .await?;
        Ok(response)
    }
    pub async fn register_signed_key(
        &self,
        opts: RegisterSignedKeyOptions,
    ) -> eyre::Result<SignerResponse> {
        let deadline = match opts.deadline {
            Some(deadline) => deadline,
            None => {
                let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
                now + 24 * 60 * 60
            }
        };

        let signer_cfg = self
            .app_signers
            .get(&opts.app_fid)
            .ok_or_else(|| eyre::eyre!("no signer config for app FID {}", opts.app_fid))?;
        let signature = self
            .sign_signed_key_req(signer_cfg, opts.app_fid, deadline, &opts.public_key)
            .await?;

        let req = RegisterSignedKeyParams {
            app_fid: opts.app_fid,
            deadline,
            signature,
            signer_uuid: opts.signer_uuid,
            redirect_url: opts.redirect_url,
            sponsor: opts.sponsor,
        };
        let url = format!("{}/farcaster/signer/signed_key", self.base_url);
        let response = self
            .client
            .post(url)
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json::<SignerResponse>()
            .await?;
        Ok(response)
    }

    pub async fn lookup_signer(&self, signer_uuid: &str) -> eyre::Result<SignerResponse> {
        let url = format!("{}/farcaster/signer", self.base_url);

        let response = self
            .client
            .get(url)
            .query(&[("signer_uuid", signer_uuid)])
            .send()
            .await?
            .error_for_status()?
            .json::<SignerResponse>()
            .await?;
        Ok(response)
    }

    pub async fn get_for_you_feed(
        &self,
        params: &GetForYouFeedParams,
    ) -> eyre::Result<ForYouFeedResponse> {
        let url = format!("{}/farcaster/feed/for_you", self.base_url);

        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<ForYouFeedResponse>()
            .await?;

        Ok(response)
    }

    pub async fn get_following_feed(
        &self,
        params: &GetFollowingFeedParams,
    ) -> eyre::Result<FollowingFeedResponse> {
        let url = format!("{}/farcaster/feed/following", self.base_url);

        let response = self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<FollowingFeedResponse>()
            .await?;

        Ok(response)
    }

    pub async fn get_notifications(
        &self,
        params: &GetNotificationsParams,
    ) -> eyre::Result<NotificationsResponse> {
        let url = format!("{}/farcaster/notifications", self.base_url);
        let query = GetNotificationsRawQuery::from(params);

        let response = self
            .client
            .get(url)
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<NotificationsResponse>()
            .await?;

        Ok(response)
    }
}
