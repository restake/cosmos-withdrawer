use cosmrs::{
    rpc::{
        Client, HttpClient, Method, Request, Response, SimpleRequest, dialect::Dialect,
        request::RequestMessage,
    },
    tendermint::chain::Id,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StatusRequest;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StatusResponse {
    /// Node information
    pub node_info: NodeInfo,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NodeInfo {
    pub network: Id,
}

impl Response for StatusResponse {}

impl RequestMessage for StatusRequest {
    fn method(&self) -> Method {
        Method::Status
    }
}

impl<S: Dialect> Request<S> for StatusRequest {
    type Response = StatusResponse;
}

impl<S: Dialect> SimpleRequest<S> for StatusRequest {
    type Output = StatusResponse;
}

pub async fn get_status(client: &HttpClient) -> Result<StatusResponse, cosmrs::rpc::Error> {
    client.perform(StatusRequest).await
}
