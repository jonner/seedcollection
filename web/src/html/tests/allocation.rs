use crate::{app_url, Result};
use crate::{auth::Credentials, test_app};
use axum::http::StatusCode;
use axum::{
    http::{header::CONTENT_TYPE, Request},
    Router,
};
use sqlx::{Pool, Sqlite};
use test_log::test;
use tower::Service;

async fn login(app: &mut Router) -> Result<String> {
    let creds = serde_urlencoded::to_string(Credentials {
        username: "testuser".to_string(),
        password: "topsecret123".to_string(),
        next: Some("url".to_string()),
    })?;
    let request = Request::builder()
        .uri(app_url("/auth/login"))
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
    fixtures(
        path = "../../../../db/fixtures",
        scripts("users", "sources", "taxa", "samples", "projects")
    )
))]
async fn test_new_note(pool: Pool<Sqlite>) {
    let mut app = test_app(pool).await.expect("failed to create test app");

    let params = serde_urlencoded::to_string(&[
        ("notetype", "Planting"),
        ("date", "2023-01-01"),
        ("summary", "This is a summary"),
        ("details", "This is a detail paragraph"),
    ])
    .expect("failed to serialize form");
    // make sure we can't add a note without logging in
    let req = Request::builder()
        .uri(app_url("/project/1/sample/1/note/new"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(params.clone())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // first log in:
    let cookie = login(&mut app).await.expect("Failed to log in");

    // then try to add a note
    let req = Request::builder()
        .uri(app_url("/project/1/sample/1/note/new"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("Cookie", cookie.clone())
        .body(params.clone())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::OK);

    // try to add a note to a sample that doesn't exist
    let req = Request::builder()
        .uri(app_url("/project/1/sample/99/note/new"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("Cookie", cookie.clone())
        .body(params.clone())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // this url specifies a sample for a different user that is not in this project
    let req = Request::builder()
        .uri(app_url("/project/1/sample/4/note/new"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("Cookie", cookie.clone())
        .body(params.clone())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // trying to add a note to a sample that is owned by a different user and also in a
    // different project owned by that user
    let req = Request::builder()
        .uri(app_url("/project/3/sample/4/note/new"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("Cookie", cookie.clone())
        .body(params.clone())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // validate form fields
    // missing summary
    let missing_summary = serde_urlencoded::to_string(&[
        ("notetype", "Planting"),
        ("date", "2023-01-01"),
        ("summary", ""),
        ("details", "This is a detail paragraph"),
    ])
    .expect("failed to serialize form");
    let req = Request::builder()
        .uri(app_url("/project/1/sample/1/note/new"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("Cookie", cookie.clone())
        .body(missing_summary.clone())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // validate note type
    let missing_type = serde_urlencoded::to_string(&[
        ("notetype", ""),
        ("date", "2023-01-01"),
        ("summary", "summary"),
        ("details", "This is a detail paragraph"),
    ])
    .expect("failed to serialize form");
    let req = Request::builder()
        .uri(app_url("/project/1/sample/1/note/new"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("Cookie", cookie.clone())
        .body(missing_type.clone())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // missing date
    let missing_date = serde_urlencoded::to_string(&[
        ("notetype", "Planting"),
        ("date", ""),
        ("summary", "summary"),
        ("details", "This is a detail paragraph"),
    ])
    .expect("failed to serialize form");
    let req = Request::builder()
        .uri(app_url("/project/1/sample/1/note/new"))
        .method("POST")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("Cookie", cookie.clone())
        .body(missing_date.clone())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
