use super::*;
use test_log::test;

#[test(sqlx::test(
    migrations = "../db/migrations/",
    fixtures(
        path = "../../../../db/fixtures",
        scripts("users", "sources", "taxa", "samples", "projects")
    )
))]
async fn test_filter_samples(pool: Pool<Sqlite>) {
    let mut app = test_app(pool).await.expect("failed to create test app");

    // first log in:
    let cookie = login(&mut app).await.expect("Failed to log in");

    // then try to add a note
    let req = Request::builder()
        .uri(app_url("/sample/list?filter=ely"))
        .method("GET")
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
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
