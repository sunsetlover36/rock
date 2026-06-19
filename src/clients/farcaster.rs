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
use rock_wire::farcaster::*;
use serde::{Serialize, de::DeserializeOwned};
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
    async fn get<T: DeserializeOwned, Q: Serialize + ?Sized>(
        &self,
        path: &str,
        query: &Q,
        experimental: bool,
    ) -> eyre::Result<T> {
        let mut request = self
            .client
            .get(format!("{}/farcaster/{path}", self.base_url))
            .query(query);
        if experimental {
            request = request.header("x-neynar-experimental", "true");
        }
        Ok(request
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await?)
    }
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

    pub async fn list_channels(&self, p: &ListChannelsParams) -> eyre::Result<ChannelsResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel/list", p, false).await
    }
    pub async fn get_users_active_channels(
        &self,
        p: &GetUsersActiveChannelsParams,
    ) -> eyre::Result<ActiveChannelsResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel/user", p, false).await
    }
    pub async fn search_channels(
        &self,
        p: &SearchChannelsParams,
    ) -> eyre::Result<ChannelSearchResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel/search", p, false).await
    }
    pub async fn bulk_channels(
        &self,
        p: &BulkChannelsParams,
    ) -> eyre::Result<BulkChannelsResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel/bulk", &BulkChannelsRawQuery::from(p), false)
            .await
    }
    pub async fn get_channel(&self, p: &GetChannelParams) -> eyre::Result<ChannelResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel", p, false).await
    }
    pub async fn get_channel_members(
        &self,
        p: &GetChannelMembersParams,
    ) -> eyre::Result<ChannelMembersResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel/member/list", p, p.experimental).await
    }
    pub async fn get_channel_followers(
        &self,
        p: &GetChannelFollowersParams,
    ) -> eyre::Result<ChannelFollowersResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel/followers", p, p.experimental).await
    }

    pub async fn follow_channel(&self, params: &FollowChannelParams) -> eyre::Result<FollowChannelResponse> {
        Ok(self.client.post(format!("{}/farcaster/channel/follow", self.base_url)).json(params).send().await?.error_for_status()?.json().await?)
    }
    pub async fn unfollow_channel(&self, params: &UnfollowChannelParams) -> eyre::Result<UnfollowChannelResponse> {
        Ok(self.client.delete(format!("{}/farcaster/channel/follow", self.base_url)).json(params).send().await?.error_for_status()?.json().await?)
    }
    pub async fn get_relevant_channel_followers(
        &self,
        p: &GetRelevantChannelFollowersParams,
    ) -> eyre::Result<RelevantChannelFollowersResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel/followers/relevant", p, p.experimental)
            .await
    }
    pub async fn get_user_channels(
        &self,
        p: &GetUserChannelsParams,
    ) -> eyre::Result<UserChannelsResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("user/channels", p, false).await
    }
    pub async fn get_user_channel_memberships(
        &self,
        p: &GetUserChannelMembershipsParams,
    ) -> eyre::Result<ChannelMembersResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("user/memberships/list", p, false).await
    }
    pub async fn get_trending_channels(
        &self,
        p: &GetTrendingChannelsParams,
    ) -> eyre::Result<TrendingChannelsResponse> {
        p.validate().map_err(|e| eyre::eyre!(e))?;
        self.get("channel/trending", p, false).await
    }

    pub async fn get_followers(
        &self,
        params: &GetFollowersParams,
    ) -> eyre::Result<FollowersResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/followers", self.base_url);
        let mut request = self.client.get(url).query(params);
        if params.experimental {
            request = request.header("x-neynar-experimental", "true");
        }

        Ok(request
            .send()
            .await?
            .error_for_status()?
            .json::<FollowersResponse>()
            .await?)
    }

    pub async fn get_relevant_followers(
        &self,
        params: &GetRelevantFollowersParams,
    ) -> eyre::Result<RelevantFollowersResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/followers/relevant", self.base_url);
        let mut request = self.client.get(url).query(params);
        if params.experimental {
            request = request.header("x-neynar-experimental", "true");
        }

        Ok(request
            .send()
            .await?
            .error_for_status()?
            .json::<RelevantFollowersResponse>()
            .await?)
    }

    pub async fn get_reciprocal_followers(
        &self,
        params: &GetReciprocalFollowersParams,
    ) -> eyre::Result<ReciprocalFollowersResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/followers/reciprocal", self.base_url);
        let mut request = self.client.get(url).query(params);
        if params.experimental {
            request = request.header("x-neynar-experimental", "true");
        }

        Ok(request
            .send()
            .await?
            .error_for_status()?
            .json::<ReciprocalFollowersResponse>()
            .await?)
    }

    pub async fn get_following(
        &self,
        params: &GetFollowingParams,
    ) -> eyre::Result<FollowingResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/following", self.base_url);
        let mut request = self.client.get(url).query(params);
        if params.experimental {
            request = request.header("x-neynar-experimental", "true");
        }

        Ok(request
            .send()
            .await?
            .error_for_status()?
            .json::<FollowingResponse>()
            .await?)
    }

    pub async fn get_follow_suggestions(
        &self,
        params: &GetFollowSuggestionsParams,
    ) -> eyre::Result<FollowSuggestionsResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/following/suggested", self.base_url);
        let mut request = self.client.get(url).query(params);
        if params.experimental {
            request = request.header("x-neynar-experimental", "true");
        }

        Ok(request
            .send()
            .await?
            .error_for_status()?
            .json::<FollowSuggestionsResponse>()
            .await?)
    }

    pub async fn get_best_friends(
        &self,
        params: &GetBestFriendsParams,
    ) -> eyre::Result<BestFriendsResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/user/best_friends", self.base_url);

        Ok(self
            .client
            .get(url)
            .query(params)
            .send()
            .await?
            .error_for_status()?
            .json::<BestFriendsResponse>()
            .await?)
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

    pub async fn get_feed_by_filters(
        &self,
        params: &GetFeedByFiltersParams,
    ) -> eyre::Result<TrendingFeedResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/feed", self.base_url);
        let query = GetFeedByFiltersRawQuery::from(params);
        let mut request = self.client.get(url).query(&query);
        if params.experimental {
            request = request.header("x-neynar-experimental", "true");
        }

        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<TrendingFeedResponse>()
            .await?;

        Ok(response)
    }

    pub async fn get_trending_feed(
        &self,
        params: &GetTrendingFeedParams,
    ) -> eyre::Result<TrendingFeedResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/feed/trending", self.base_url);
        let mut request = self.client.get(url).query(params);
        if params.experimental {
            request = request.header("x-neynar-experimental", "true");
        }

        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<TrendingFeedResponse>()
            .await?;

        Ok(response)
    }

    pub async fn get_feed_by_channel_ids(
        &self,
        params: &GetFeedByChannelIdsParams,
    ) -> eyre::Result<TrendingFeedResponse> {
        params.validate().map_err(|err| eyre::eyre!(err))?;
        let url = format!("{}/farcaster/feed/channels", self.base_url);
        let query = GetFeedByChannelIdsRawQuery::from(params);
        let mut request = self.client.get(url).query(&query);
        if params.experimental {
            request = request.header("x-neynar-experimental", "true");
        }

        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<TrendingFeedResponse>()
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
