//! The HTTP seam.
//!
//! The client talks to a [`Transport`] rather than to the network, so every rule
//! that matters — paging, quota headers, error wording, the timer race — is
//! tested against recorded responses instead of a live account.

use std::collections::BTreeMap;

/// One HTTP response, reduced to what the client needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    /// Header names are lowercased by the transport.
    pub headers: BTreeMap<String, String>,
}

impl HttpResponse {
    /// A successful response with no headers, for tests and stubs.
    #[must_use]
    pub fn ok(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            headers: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(String::as_str)
    }

    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}

/// An HTTP request the client wants made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: &'static str,
    pub url: String,
    pub token: String,
    /// JSON body, for requests that have one.
    pub body: Option<String>,
}

/// Something that can perform an HTTP request.
pub trait Transport {
    /// Perform the request, or report why it could not be sent at all.
    ///
    /// A response with an error status is a success here: only transport
    /// failures — DNS, TLS, timeouts — belong in `Err`.
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String>;
}

/// The real transport, over `ureq`.
#[derive(Debug, Default)]
pub struct NetworkTransport;

impl Transport for NetworkTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        // ureq types requests with and without a body differently, so the two
        // shapes are built separately rather than unified behind a match.
        let agent = ureq::agent();
        let authorization = format!("Bearer {}", request.token);
        let result = match &request.body {
            Some(body) => agent
                .post(&request.url)
                .header("Authorization", &authorization)
                .header("Content-Type", "application/json")
                .send(body.as_str()),
            None => agent
                .get(&request.url)
                .header("Authorization", &authorization)
                .header("Content-Type", "application/json")
                .call(),
        };

        // ureq treats a 4xx or 5xx as an error; the client wants to read it.
        let mut response = match result {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(status)) => {
                return Ok(HttpResponse {
                    status,
                    body: String::new(),
                    headers: BTreeMap::new(),
                });
            }
            Err(error) => return Err(error.to_string()),
        };

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_lowercase(), value.to_owned()))
            })
            .collect();
        let body = response.body_mut().read_to_string().unwrap_or_default();

        Ok(HttpResponse {
            status,
            body,
            headers,
        })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    use super::{HttpRequest, HttpResponse, Transport};

    /// A transport that replays queued responses and records what was asked.
    pub(crate) struct FakeTransport {
        responses: RefCell<Vec<Result<HttpResponse, String>>>,
        pub(crate) requests: RefCell<Vec<HttpRequest>>,
    }

    impl FakeTransport {
        /// Queue responses, returned in order.
        pub(crate) fn new(responses: Vec<Result<HttpResponse, String>>) -> Self {
            Self {
                responses: RefCell::new(responses.into_iter().rev().collect()),
                requests: RefCell::new(Vec::new()),
            }
        }

        pub(crate) fn json(bodies: &[&str]) -> Self {
            Self::new(
                bodies
                    .iter()
                    .map(|body| Ok(HttpResponse::ok(*body)))
                    .collect(),
            )
        }

        /// A single response carrying quota headers.
        pub(crate) fn with_quota(body: &str, remaining: &str, resets_in: &str) -> Self {
            let mut headers = BTreeMap::new();
            headers.insert("x-toggl-quota-remaining".to_owned(), remaining.to_owned());
            headers.insert("x-toggl-quota-resets-in".to_owned(), resets_in.to_owned());
            Self::new(vec![Ok(HttpResponse {
                status: 200,
                body: body.to_owned(),
                headers,
            })])
        }

        pub(crate) fn status(status: u16, body: &str) -> Self {
            Self::new(vec![Ok(HttpResponse {
                status,
                body: body.to_owned(),
                headers: BTreeMap::new(),
            })])
        }

        /// The URLs requested, in order.
        pub(crate) fn urls(&self) -> Vec<String> {
            self.requests
                .borrow()
                .iter()
                .map(|request| request.url.clone())
                .collect()
        }

        pub(crate) fn bodies(&self) -> Vec<Option<String>> {
            self.requests
                .borrow()
                .iter()
                .map(|request| request.body.clone())
                .collect()
        }
    }

    impl Transport for FakeTransport {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            self.requests.borrow_mut().push(request.clone());
            self.responses
                .borrow_mut()
                .pop()
                .unwrap_or_else(|| Err("no response queued".to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HttpResponse;

    #[test]
    fn headers_are_matched_case_insensitively() {
        let mut response = HttpResponse::ok("{}");
        response
            .headers
            .insert("x-toggl-quota-remaining".to_owned(), "28".to_owned());
        assert_eq!(response.header("X-Toggl-Quota-Remaining"), Some("28"));
        assert_eq!(response.header("missing"), None);
    }

    #[test]
    fn success_covers_the_two_hundreds_only() {
        assert!(HttpResponse::ok("{}").is_success());
        for status in [199, 300, 401, 500] {
            let response = HttpResponse {
                status,
                ..HttpResponse::ok("")
            };
            assert!(!response.is_success(), "{status} should not be a success");
        }
    }
}
