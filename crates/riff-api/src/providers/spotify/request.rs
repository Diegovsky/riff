use spotify_api::apis;

use crate::error::DomainError;

pub(super) async fn send(
    request: reqwest_middleware::RequestBuilder,
) -> Result<Vec<u8>, DomainError> {
    let response = request
        .send()
        .await
        .map_err(|e| DomainError::Network(e.to_string()))?;
    let status = response.status();
    if status.is_success() {
        return response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|e| DomainError::Network(e.to_string()));
    }
    let content = response.text().await.unwrap_or_default();
    Err(apis::Error::<()>::ResponseError(apis::ResponseContent {
        status,
        content,
        entity: None,
    })
    .into())
}
