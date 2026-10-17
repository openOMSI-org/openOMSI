//! The web gateway's other pages: a request it does not answer itself - a dedicated server's
//! administration, `/api/admin/…` and the panel at `/admin/…` (not `/admin`, the machine's own
//! `POST /admin`, see `ws::local_admin`) - goes to whoever set a handler (`set_api`), on the
//! gateway's thread. Its answer carries the handler's headers and none of the status page's
//! (no `Access-Control-Allow-Origin`).

use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

/// A request as the gateway read it (the body at most 4 KiB).
pub struct ApiRequest<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    /// The header lines, `Name: value` one a line.
    pub headers: &'a str,
    pub body: &'a [u8],
    pub peer: Option<SocketAddr>,
}

pub struct ApiResponse {
    pub status: &'static str,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub headers: Vec<(&'static str, String)>,
}

/// Who answers the gateway's other pages.
pub trait HttpApi: Send + Sync {
    /// The answer, or None: not one of its pages.
    fn handle(&self, req: &ApiRequest) -> Option<ApiResponse>;
}

static API: RwLock<Option<Arc<dyn HttpApi>>> = RwLock::new(None);

/// Set (or take away) the handler of the gateway's other pages, for every gateway of this
/// process.
pub fn set_api(api: Option<Arc<dyn HttpApi>>) {
    *API.write().unwrap_or_else(|e| e.into_inner()) = api;
}

/// Whether a path is one of the other pages'.
pub(crate) fn wants(path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    path == "/api/admin" || path.starts_with("/api/admin/") || path.starts_with("/admin/")
}

/// The answer to a whole request (head and body as read), from the handler - or a 404 when
/// there is none or it does not know the page.
pub(crate) fn answer(request: &[u8], peer: Option<SocketAddr>) -> ApiResponse {
    // (the head's end found in the bytes: an offset into the lossy text is not one into them)
    let head_end = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap_or(request.len());
    let text = String::from_utf8_lossy(&request[..head_end]);
    let head: &str = &text;
    let body = request.get((head_end + 4).min(request.len())..).unwrap_or(&[]);
    let (first, headers) = head.split_once("\r\n").unwrap_or((head, ""));
    let mut words = first.split_whitespace();
    let method = words.next().unwrap_or("");
    let target = words.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let api = API.read().unwrap_or_else(|e| e.into_inner()).clone();
    let req = ApiRequest { method, path, query, headers, body, peer };
    api.and_then(|a| a.handle(&req)).unwrap_or_else(|| ApiResponse { status: "404 Not Found", content_type: "text/plain; charset=utf-8", body: b"not here".to_vec(), headers: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_administrations_pages_and_no_other() {
        assert!(wants("/api/admin/login"));
        assert!(wants("/api/admin/players?q=x"));
        assert!(wants("/admin/"));
        assert!(wants("/admin/panel.js"));
        // the machine's own door and the status page stay the gateway's
        assert!(!wants("/admin"));
        assert!(!wants("/status"));
        assert!(!wants("/players"));
        assert!(!wants("/api/adminx"));
    }

    #[test]
    fn a_request_is_read_into_its_parts() {
        struct Echo;
        impl HttpApi for Echo {
            fn handle(&self, r: &ApiRequest) -> Option<ApiResponse> {
                let text = format!("{}|{}|{}|{}|{}", r.method, r.path, r.query, r.headers.lines().count(), String::from_utf8_lossy(r.body));
                Some(ApiResponse { status: "200 OK", content_type: "text/plain", body: text.into_bytes(), headers: Vec::new() })
            }
        }
        set_api(Some(Arc::new(Echo)));
        let r = answer(b"POST /api/admin/ops?x=1 HTTP/1.1\r\nHost: a\r\nContent-Type: application/json\r\n\r\n{\"op\":1}", None);
        assert_eq!(String::from_utf8_lossy(&r.body), "POST|/api/admin/ops|x=1|2|{\"op\":1}");
        set_api(None);
        assert_eq!(answer(b"GET /api/admin/state HTTP/1.1\r\n\r\n", None).status, "404 Not Found");
    }
}
