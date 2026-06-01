use alloy::{
    network::Ethereum,
    primitives::{Address, Bytes, U256, address, utils::format_units},
    providers::{
        Identity, ProviderBuilder, RootProvider,
        fillers::{
            BlobGasFiller, CachedNonceManager, ChainIdFiller, FillProvider, GasFiller, JoinFill,
            NonceFiller,
        },
    },
    signers::local::{MnemonicBuilder, PrivateKeySigner, coins_bip39::English},
    sol,
};
use color_eyre::eyre;

use crate::config::SignerConfig;

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address owner) external view returns (uint256);
        function decimals() external view returns (uint8);
    }

    struct SignedKeyRequest {
        uint256 requestFid;
        bytes key;
        uint256 deadline;
    }
}

const ROCK_TOKEN: Address = address!("0x0dF425f1E02BB508A7A6952B1853C9238Fb7CB07");

pub(crate) fn parse_hex_bytes(s: &str) -> eyre::Result<Bytes> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    Ok(Bytes::from(hex::decode(s)?))
}

pub(crate) fn signer_from_config(cfg: &SignerConfig) -> eyre::Result<PrivateKeySigner> {
    let mnemonic = std::env::var(&cfg.mnemonic_env)
        .map_err(|_| eyre::eyre!("missing mnemonic env {}", cfg.mnemonic_env))?;

    let mut builder = MnemonicBuilder::<English>::default().phrase(mnemonic.as_str());
    if let Some(path) = &cfg.derivation_path {
        builder = builder.derivation_path(path)?;
    }

    Ok(builder.build()?)
}

type AlloyProvider = FillProvider<
    JoinFill<
        Identity,
        JoinFill<
            GasFiller,
            JoinFill<BlobGasFiller, JoinFill<NonceFiller<CachedNonceManager>, ChainIdFiller>>,
        >,
    >,
    RootProvider<Ethereum>,
    Ethereum,
>;
type IERC20Instance = IERC20::IERC20Instance<AlloyProvider>;

#[derive(Clone)]
pub(crate) struct Crypto {
    provider: AlloyProvider,
    token: IERC20Instance,
}
impl Crypto {
    pub fn new(rpc_url: &str) -> eyre::Result<Self> {
        let provider = ProviderBuilder::new().connect_http(rpc_url.parse()?);
        let token = IERC20::new(ROCK_TOKEN, provider.clone());

        Ok(Self { provider, token })
    }

    pub async fn rock_decimals(&self) -> eyre::Result<u8> {
        let decimals = self.token.decimals().call().await?;
        Ok(decimals)
    }

    pub async fn rock_balance(&self, owner: Address) -> eyre::Result<U256> {
        let balance = self.token.balanceOf(owner).call().await?;
        Ok(balance)
    }
    pub async fn rock_balance_display(&self, owner: Address) -> eyre::Result<String> {
        let balance = self.rock_balance(owner).await?;
        let decimals = self.token.decimals().call().await?;

        Ok(format_units(balance, decimals)?)
    }

    pub async fn is_rock_holder(&self, owner: Address) -> eyre::Result<bool> {
        Ok(self.rock_balance(owner).await? > U256::ZERO)
    }
}
