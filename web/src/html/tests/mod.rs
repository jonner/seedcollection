use crate::{Result, state::AppState, test_app};
use axum::{
    Router,
    body::{Body, Bytes, HttpBody},
    http::{Request, StatusCode, header::CONTENT_TYPE},
};
use http_body_util::BodyExt;
use sqlx::{Pool, Sqlite};
use test_log::test;
use tower::Service;

mod allocation;
mod project;
mod sample;

/// usage:
/// let (_parts, body) = response.into_parts();
/// print_response_body(body).await;
///
/// note that this consumes body, so it can't be used again
#[allow(dead_code)]
async fn print_response_body<B>(body: B)
where
    B: HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            tracing::warn!("failed to collect body: {err}");
            return;
        }
    };

    if let Ok(body) = std::str::from_utf8(&bytes) {
        tracing::debug!("body = {body:?}");
    } else {
        tracing::warn!("Couldn't convert body to utf8");
    }
}

/// logs the user into the app and returns a cookie value that can be used in subsequent requests
async fn login(app: &mut Router, state: AppState) -> Result<String> {
    let creds = serde_urlencoded::to_string([
        ("username", "testuser"),
        ("password", "topsecret123"),
        ("next", "url"),
    ])?;
    let request = Request::builder()
        .uri(state.path("/auth/login"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(creds)?;
    let response = app.as_service().call(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    // extract cookie
    Ok(response
        .headers()
        .get("set-cookie")
        .expect("no set-cookie header")
        .to_str()?
        .to_string())
}

#[test(sqlx::test(
    migrations = "../db/migrations/",
    fixtures(path = "../../../../db/fixtures", scripts("users"))
))]
async fn test_login(pool: Pool<Sqlite>) {
    let (mut app, state) = test_app(pool).await.expect("failed to create test app").0;
    let cookie = login(&mut app, state.clone())
        .await
        .expect("Failed to log in");
    assert!(!cookie.is_empty());

    // now make sure we can't access pages that are protected without the cookie
    let req = Request::builder()
        .uri(state.path("/project/list"))
        .method("GET")
        .body(Body::empty())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // ...but we can with the cookie
    let req = Request::builder()
        .uri(state.path("/project/list"))
        .method("GET")
        .header("Cookie", cookie.clone())
        .body(Body::empty())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::OK);
}
