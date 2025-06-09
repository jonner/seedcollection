use super::*;
use crate::test_app;
use sqlx::{Pool, Sqlite};
use test_log::test;

#[test(sqlx::test(
    migrations = "../db/migrations/",
    fixtures(
        path = "../../../../db/fixtures",
        scripts("users", "sources", "taxa", "samples", "projects")
    )
))]
async fn test_list_projects(pool: Pool<Sqlite>) {
    let mut app = test_app(pool).await.expect("failed to create test app").0;
    // first log in:
    let cookie = login(&mut app).await.expect("Failed to log in");

    let req = Request::builder()
        .uri(app_url("/project/list"))
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

#[test(sqlx::test(
    migrations = "../db/migrations/",
    fixtures(
        path = "../../../../db/fixtures",
        scripts("users", "sources", "taxa", "samples", "projects")
    )
))]
async fn test_new_project(pool: Pool<Sqlite>) {
    let mut app = test_app(pool).await.expect("failed to create test app").0;
    // first log in:
    let cookie = login(&mut app).await.expect("Failed to log in");

    let req = Request::builder()
        .uri(app_url("/project/new"))
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

    // try to post a new project without any form data
    let req = Request::builder()
        .uri(app_url("/project/new"))
        .method("POST")
        .header("Cookie", cookie.clone())
        .body(Body::empty())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // try to post a new project malformed form data
    let req = Request::builder()
        .uri(app_url("/project/new"))
        .method("POST")
        .header("Cookie", cookie.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body("foo".to_string())
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // well-formed form data, but not expected format
    let missing_name =
        serde_urlencoded::to_string([("foo", "bar")]).expect("failed to serialize form");
    let req = Request::builder()
        .uri(app_url("/project/new"))
        .method("POST")
        .header("Cookie", cookie.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(missing_name)
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // only name
    let form = serde_urlencoded::to_string([("name", "project name #1")])
        .expect("failed to serialize form");
    let req = Request::builder()
        .uri(app_url("/project/new"))
        .method("POST")
        .header("Cookie", cookie.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form)
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::OK);

    // empty name
    let form = serde_urlencoded::to_string([("name", "")]).expect("failed to serialize form");
    let req = Request::builder()
        .uri(app_url("/project/new"))
        .method("POST")
        .header("Cookie", cookie.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form)
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // name + empty description
    let form = serde_urlencoded::to_string([("name", "project name #2"), ("description", "")])
        .expect("failed to serialize form");
    let req = Request::builder()
        .uri(app_url("/project/new"))
        .method("POST")
        .header("Cookie", cookie.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form)
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("HX-Redirect").is_some());

    // name + description
    let form = serde_urlencoded::to_string([
        ("name", "project name #3"),
        ("description", "This is a description of the project"),
    ])
    .expect("failed to serialize form");
    let req = Request::builder()
        .uri(app_url("/project/new"))
        .method("POST")
        .header("Cookie", cookie.clone())
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form)
        .expect("Failed to build request");
    let response = app
        .as_service()
        .call(req)
        .await
        .expect("Failed to execute request");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("HX-Redirect").is_some());
}
