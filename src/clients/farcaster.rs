use color_eyre::eyre;
use reqwest::{
    Client,
    header::{self, HeaderMap, HeaderName, HeaderValue},
};
use rock_wire::farcaster::{
    BulkFetchCastsParams, BulkFetchCastsRawQuery, BulkFetchCastsResponse, Cast,
    CastConversationResponse, CreatedCast, GetCastConversationParams, GetCastParams,
    GetCastResponse, GetReactionsParams, GetReactionsRawQuery, GetUserByUsernameParams,
    GetUserByUsernameResponse, GetUsersByFidsParams, GetUsersByFidsRawQuery,
    GetUsersByFidsResponse, ReactionsResponse, SendCastParams, SendCastResponse, User,
};

#[derive(Clone)]
pub(crate) struct FarcasterApi {
    client: Client,
    base_url: String,
}
impl FarcasterApi {
    pub fn new(api_key: &str) -> eyre::Result<Self> {
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
        })
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
}
