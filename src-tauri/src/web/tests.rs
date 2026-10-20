use super::*;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn serve(body: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(body.to_string())
                .insert_header("content-type", "text/html"),
        )
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn reads_the_posting_text() {
    let server = serve(
        "<html><body><nav>Careers Login</nav><article><h1>Senior Backend Engineer</h1>\
         <p>You will build payment rails in Rust and own reliability for millions of transfers every day.</p>\
         <p>Requirements: five years of backend experience and a taste for correctness.</p>\
         </article></body></html>",
    )
    .await;
    let text = fetch_text(&server.uri(), FetchPolicy::LocalFirstHopOnly, 10_000)
        .await
        .unwrap();
    assert!(text.contains("payment rails in Rust"), "{text}");
    assert!(text.contains("five years of backend experience"), "{text}");
}

#[tokio::test]
async fn cuts_long_text_with_an_ellipsis() {
    let server = serve(&format!(
        "<html><body><p>{}</p></body></html>",
        "word ".repeat(200)
    ))
    .await;
    let text = fetch_text(&server.uri(), FetchPolicy::LocalFirstHopOnly, 50)
        .await
        .unwrap();
    assert_eq!(text.chars().count(), 51);
    assert!(text.ends_with('…'), "{text}");
}

#[tokio::test]
async fn empty_page_is_an_error() {
    let server = serve("<html><body></body></html>").await;
    let err = fetch_text(&server.uri(), FetchPolicy::LocalFirstHopOnly, 100)
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Empty), "{err}");
}

#[tokio::test]
async fn http_errors_are_reported() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let err = fetch_text(&server.uri(), FetchPolicy::LocalFirstHopOnly, 100)
        .await
        .unwrap_err();
    assert!(err.to_string().starts_with("HTTP 404"), "{err}");
}

#[tokio::test]
async fn only_public_hosts_by_default() {
    let err = fetch_text("http://127.0.0.1:9/job", FetchPolicy::PublicOnly, 100)
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Private(_)), "{err}");
}

#[tokio::test]
async fn rejects_non_http_schemes() {
    let err = fetch_text("file:///etc/passwd", FetchPolicy::PublicOnly, 100)
        .await
        .unwrap_err();
    assert!(matches!(err, FetchError::Scheme(_)), "{err}");
}
