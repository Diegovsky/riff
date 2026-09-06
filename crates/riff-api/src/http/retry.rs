use std::time::Duration;

use isahc::error::ErrorKind;
use isahc::http::StatusCode;
use isahc::{AsyncReadResponseExt, HttpClient, Request};
use thiserror::Error;

#[derive(Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub backoff_multiplier: u32,
    pub respect_retry_after: bool,
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: StatusCode,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Error, Debug)]
pub enum HttpError {
    #[error("network error: {0}")]
    Network(String),
    #[error("retries exhausted (last: {last_error})")]
    RetriesExhausted { last_error: String },
}

impl HttpError {
    fn is_retryable(status: StatusCode) -> bool {
        status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
    }
}

pub struct ServiceClient {
    inner: HttpClient,
    retry: RetryPolicy,
    timeout: Duration,
}

/// Outcome of a single execute attempt
enum Step {
    Done(Result<HttpResponse, HttpError>),
    Retry(Duration),
}

impl ServiceClient {
    pub fn new(inner: HttpClient, retry: RetryPolicy, timeout: Duration) -> Self {
        Self {
            inner,
            retry,
            timeout,
        }
    }

    pub async fn execute(&self, request: Request<Vec<u8>>) -> Result<HttpResponse, HttpError> {
        let mut delay = self.retry.initial_backoff;
        let mut last_error = String::new();

        let uri = request.uri().to_string();
        let method = request.method().to_string();

        for attempt in 0..self.retry.max_attempts {
            let attempt_no = attempt + 1;
            let req = clone_request(&request);
            debug!(
                "http: {method} {uri} attempt {attempt_no}/{} (backoff {delay:?})",
                self.retry.max_attempts
            );

            let step = match tokio::time::timeout(self.timeout, self.send_once(req)).await {
                Ok(Ok(resp)) => {
                    if resp.status == StatusCode::TOO_MANY_REQUESTS {
                        let wait = if self.retry.respect_retry_after {
                            parse_retry_after(&resp).unwrap_or(delay)
                        } else {
                            delay
                        };
                        last_error = "429 Too Many Requests".to_string();
                        warn!(
                            "http: {method} {uri} attempt {attempt_no} got 429, \
                             retrying after {wait:?}"
                        );
                        Step::Retry(wait)
                    } else if HttpError::is_retryable(resp.status) {
                        last_error = resp.status.to_string();
                        warn!(
                            "http: {method} {uri} attempt {attempt_no} got retryable \
                             status {}, retrying after {delay:?}",
                            resp.status
                        );
                        Step::Retry(delay)
                    } else {
                        // Non-retryable status.
                        if resp.status.is_success() && resp.body.is_empty() {
                            warn!(
                                "http: {method} {uri} attempt {attempt_no} returned \
                                 success status {} with an EMPTY body ({} bytes); \
                                 not retried",
                                resp.status,
                                resp.body.len()
                            );
                        } else {
                            debug!(
                                "http: {method} {uri} attempt {attempt_no} returned \
                                 status {} ({} bytes)",
                                resp.status,
                                resp.body.len()
                            );
                        }
                        Step::Done(Ok(resp))
                    }
                }
                Ok(Err(e)) => {
                    if is_transient(&e) {
                        last_error = e.to_string();
                        warn!(
                            "http: {method} {uri} attempt {attempt_no} transient \
                             error: {e}; retrying after {delay:?}"
                        );
                        Step::Retry(delay)
                    } else {
                        // Non-transient: fail immediately.
                        warn!(
                            "http: {method} {uri} attempt {attempt_no} non-transient \
                             network error (not retried): {e}"
                        );
                        Step::Done(Err(HttpError::Network(e.to_string())))
                    }
                }
                Err(_elapsed) => {
                    last_error = "timeout".to_string();
                    warn!(
                        "http: {method} {uri} attempt {attempt_no} timed out after \
                         {:?}; retrying after {delay:?}",
                        self.timeout
                    );
                    Step::Retry(delay)
                }
            };

            match step {
                Step::Done(result) => return result,
                Step::Retry(wait) => {
                    tokio::time::sleep(wait).await;
                    delay *= self.retry.backoff_multiplier;
                }
            }
        }

        warn!(
            "http: {method} {uri} exhausted all {} attempts (last error: {last_error})",
            self.retry.max_attempts
        );
        Err(HttpError::RetriesExhausted { last_error })
    }

    async fn send_once(&self, request: Request<Vec<u8>>) -> Result<HttpResponse, isahc::Error> {
        let uri = request.uri().to_string();
        let mut resp = self.inner.send_async(request).await?;
        let status = resp.status();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let content_length = resp
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let is_encoded = resp
            .headers()
            .get("content-encoding")
            .and_then(|v| v.to_str().ok())
            .map(|v| !v.trim().is_empty() && !v.eq_ignore_ascii_case("identity"))
            .unwrap_or(false);
        trace!(
            "http: {uri} response headers received: status {status}, \
             content-length {content_length:?}"
        );
        let body = match resp.bytes().await {
            Ok(body) => body,
            Err(e) => {
                warn!("http: {uri} body read failed after status {status}: {e}");
                return Err(e.into());
            }
        };
        // Detect truncated downloads (body shorter than Content-Length).
        if !is_encoded {
            if let Some(expected) = content_length
                .as_deref()
                .and_then(|s| s.parse::<usize>().ok())
            {
                if body.len() < expected {
                    warn!(
                        "http: {uri} body length {} is short of Content-Length {} \
                         (status {status}); possible truncated download",
                        body.len(),
                        expected
                    );
                }
            }
        }
        trace!(
            "http: {uri} body read complete: {} bytes (status {status})",
            body.len()
        );
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn clone_request(req: &Request<Vec<u8>>) -> Request<Vec<u8>> {
    let mut builder = Request::builder()
        .method(req.method().clone())
        .uri(req.uri().clone());
    for (k, v) in req.headers() {
        builder = builder.header(k, v);
    }
    builder.body(req.body().clone()).unwrap()
}

fn parse_retry_after(resp: &HttpResponse) -> Option<Duration> {
    let value = resp
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
        .map(|(_, v)| v.trim())?;
    retry_after_to_duration(value, unix_now_secs())
}

/// Parse a `Retry-After` header value (delta-seconds or IMF-fixdate).
fn retry_after_to_duration(value: &str, now_secs: i64) -> Option<Duration> {
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let target = httpdate::parse_http_date(value).ok()?;
    let target_secs = target
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Some(Duration::from_secs((target_secs - now_secs).max(0) as u64))
}

fn unix_now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn is_transient(e: &isahc::Error) -> bool {
    // ProtocolViolation (malformed HTTP response) is transient
    e.is_network() || e.is_timeout() || e.kind() == ErrorKind::ProtocolViolation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_delta_seconds() {
        assert_eq!(
            retry_after_to_duration("120", 0),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            retry_after_to_duration("0", 0),
            Some(Duration::from_secs(0))
        );
    }

    #[test]
    fn retry_after_http_date_future() {
        // 2015-10-21 07:28:00 UTC == 1445412480
        let now = 1_445_412_480 - 60;
        assert_eq!(
            retry_after_to_duration("Wed, 21 Oct 2015 07:28:00 GMT", now),
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn retry_after_http_date_past_is_zero() {
        let now = 1_445_412_480 + 120;
        assert_eq!(
            retry_after_to_duration("Wed, 21 Oct 2015 07:28:00 GMT", now),
            Some(Duration::from_secs(0))
        );
    }

    #[test]
    fn retry_after_garbage_is_none() {
        assert_eq!(retry_after_to_duration("not-a-date", 0), None);
    }

    #[test]
    fn is_retryable_classification() {
        assert!(HttpError::is_retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(HttpError::is_retryable(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(HttpError::is_retryable(StatusCode::BAD_GATEWAY));
        assert!(!HttpError::is_retryable(StatusCode::OK));
        assert!(!HttpError::is_retryable(StatusCode::NOT_FOUND));
    }

    #[test]
    fn protocol_violation_is_transient() {
        let err = isahc::Error::from(ErrorKind::ProtocolViolation);
        assert!(!err.is_network());
        assert!(!err.is_timeout());
        assert!(is_transient(&err));
    }

    #[test]
    fn network_and_timeout_errors_are_transient() {
        assert!(is_transient(&isahc::Error::from(
            ErrorKind::ConnectionFailed
        )));
        assert!(is_transient(&isahc::Error::from(ErrorKind::Io)));
        assert!(is_transient(&isahc::Error::from(ErrorKind::NameResolution)));
        assert!(is_transient(&isahc::Error::from(ErrorKind::Timeout)));
    }

    #[test]
    fn non_transient_errors_are_not_retried() {
        assert!(!is_transient(&isahc::Error::from(
            ErrorKind::InvalidRequest
        )));
    }
}
