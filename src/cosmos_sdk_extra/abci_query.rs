use cosmrs::{
    proto::{
        cosmos::{
            auth::v1beta1::{
                Bech32PrefixRequest, Bech32PrefixResponse, QueryAccountRequest,
                QueryAccountResponse,
            },
            authz::v1beta1::{QueryGrantsRequest, QueryGrantsResponse},
            distribution::v1beta1::{
                QueryDelegationTotalRewardsRequest, QueryDelegationTotalRewardsResponse,
                QueryParamsRequest as QueryDistributionParamsRequest,
                QueryParamsResponse as QueryDistributionParamsResponse,
                QueryValidatorCommissionRequest, QueryValidatorCommissionResponse,
            },
            staking::v1beta1::{
                QueryDelegatorDelegationsRequest, QueryDelegatorDelegationsResponse,
            },
            tx::v1beta1::{SimulateRequest, SimulateResponse},
        },
        prost::{Message, Name},
    },
    rpc::{Client, HttpClient},
};
use eyre::{Context, eyre};
use paste::paste;

// TODO: only unfortunate part is that I need to specify the path. This can be found from gRPC client implementation though.
// TODO: I'm pretty sure I can put together a clever hack to implement a custom gRPC transport which uses /abci_query instead.
pub async fn execute_abci_query<T: CosmosABCIQuery>(
    client: &HttpClient,
    request: T::Request,
) -> eyre::Result<T::Response> {
    let data = request.encode_to_vec();
    let response = client
        .abci_query(Some(T::QUERY_PATH.to_string()), data, None, false)
        .await
        .wrap_err("failed to do abci query")?;

    if response.code.is_err() {
        return Err(eyre!(
            "rpc error code = {} desc = {}",
            response.code.value(),
            response.log
        ));
    }

    let buf = response.value.as_slice();

    T::Response::decode(buf).wrap_err("failed to decode response")
}

macro_rules! define_query {
    ($path:expr, $name:ident $(,)?) => {
        define_query!($path, $name, $name);
    };
    ($path:expr, $name:ident, $type_prefix:expr $(,)?) => {
        pub struct $name {}
        paste! {
            impl CosmosABCIQuery for $name {
                const QUERY_PATH: &'static str = $path;
                type Request = [<$type_prefix Request>];
                type Response = [<$type_prefix Response>];
            }
        }
    };
}

pub trait CosmosABCIQuery {
    /// gRPC query url + method
    const QUERY_PATH: &'static str;

    /// Request type
    type Request: Message + Name;

    /// Response type
    type Response: Message + Name + Default;
}

define_query!("/cosmos.auth.v1beta1.Query/Account", QueryAccount);
define_query!("/cosmos.auth.v1beta1.Query/Bech32Prefix", Bech32Prefix);
define_query!("/cosmos.authz.v1beta1.Query/Grants", QueryGrants);
define_query!(
    "/cosmos.distribution.v1beta1.Query/DelegationTotalRewards",
    QueryDelegationTotalRewards
);
define_query!(
    "/cosmos.distribution.v1beta1.Query/Params",
    QueryDistributionParams,
);
define_query!(
    "/cosmos.distribution.v1beta1.Query/ValidatorCommission",
    QueryValidatorCommission,
);
define_query!(
    "/cosmos.staking.v1beta1.Query/DelegatorDelegations",
    QueryDelegatorDelegations,
);
define_query!("/cosmos.tx.v1beta1.Service/Simulate", Simulate);
