use super::error_alert_response;
use crate::{
    app_url,
    auth::{AuthSession, Credentials},
    error,
    state::AppState,
    TemplateKey,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    empty_string_as_none,
    loadable::Loadable,
    user::{User, UserStatus},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use time::{macros::format_description, Duration, OffsetDateTime, PrimitiveDateTime};
use tracing::{debug, error};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(show_login).post(do_login))
        .route("/logout", post(logout))
        .route("/verify/:key", get(show_verification).post(verify_user))
}

#[derive(Clone, Deserialize)]
pub struct RegisterParams {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub next: Option<String>,
}

#[allow(dead_code)]
async fn register_user(
    auth: AuthSession,
    Form(params): Form<RegisterParams>,
) -> Result<impl IntoResponse, error::Error> {
    auth.backend
        .register(params.username, params.email, params.password)
        .await
}

#[allow(dead_code)]
async fn show_register(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), ()))
}

#[derive(Debug, Deserialize)]
pub struct NextUrl {
    next: Option<String>,
}

async fn show_login(
    TemplateKey(key): TemplateKey,
    auth: AuthSession,
    State(state): State<AppState>,
    Query(NextUrl { next }): Query<NextUrl>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user, next => next),
    ))
}

fn login_failure_response<E: std::fmt::Debug>(
    state: &AppState,
    context: &str,
    err: Option<E>,
) -> impl IntoResponse {
    error!("{context}: {err:?}");
    error_alert_response(
        state,
        StatusCode::UNAUTHORIZED,
        "Incorrect username or password. Please double-check and try again.".to_string(),
    )
}

async fn do_login(
    mut auth: AuthSession,
    State(state): State<AppState>,
    Form(creds): Form<Credentials>,
) -> impl IntoResponse {
    match auth.authenticate(creds.clone()).await {
        Ok(authenticated) => match authenticated {
            Some(user) => match auth.login(&user).await {
                Ok(()) => (
                    [(
                        "HX-Redirect",
                        creds.next.as_ref().cloned().unwrap_or(app_url("/")),
                    )],
                    "",
                )
                    .into_response(),
                Err(e) => {
                    login_failure_response(&state, "Failed to login", Some(e)).into_response()
                }
            },
            None => login_failure_response::<&str>(
                &state,
                &format!("Failed to find a user '{}'", creds.username),
                None,
            )
            .into_response(),
        },
        Err(e) => {
            login_failure_response(&state, "Failed to authenticate", Some(&e)).into_response()
        }
    }
}

async fn logout(mut auth: AuthSession) -> impl IntoResponse {
    match auth.logout().await {
        Ok(_) => Redirect::to("login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Serialize, PartialEq, Debug)]
enum VerifyStatus {
    VerificationCodeExpired,
    VerificationCodeNotFound,
    AlreadyVerified,
    VerificationCodeValid,
    VerificationSuccessful,
}

struct VerificationRow {
    #[allow(dead_code)]
    uvid: i64,
    userid: i64,
    #[allow(dead_code)]
    uvkey: String,
    uvrequested: String,
    uvexpiration: i64,
    #[allow(dead_code)]
    uvconfirmed: i64,
}

fn parse_sqlite_datetime(timestamp: &str) -> anyhow::Result<OffsetDateTime> {
    PrimitiveDateTime::parse(
        timestamp,
        format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
    )
    .map(|p| p.assume_utc())
    .map_err(|e| e.into())
}

async fn check_verification_code(
    key: &str,
    pool: &Pool<Sqlite>,
) -> Result<VerifyStatus, error::Error> {
    let row = sqlx::query_as!(
        VerificationRow,
        "SELECT * FROM sc_user_verification WHERE uvkey=?",
        key
    )
    .fetch_optional(pool)
    .await?;
    let status = match row {
        Some(row) => {
            debug!("requested date from db: {}", row.uvrequested);
            let requestdate = parse_sqlite_datetime(&row.uvrequested)?;
            debug!("parsed requested date: {}", requestdate);
            debug!("expiration from db: {}", row.uvexpiration);
            let expiration = requestdate + Duration::new(row.uvexpiration * 60 * 60, 0);
            debug!("calculated expiration date: {}", expiration);
            debug!("now: {}", OffsetDateTime::now_utc());
            if expiration < OffsetDateTime::now_utc() {
                VerifyStatus::VerificationCodeExpired
            } else {
                let user = User::load(row.userid, pool).await?;
                if user.status == UserStatus::Verified {
                    VerifyStatus::AlreadyVerified
                } else {
                    VerifyStatus::VerificationCodeValid
                }
            }
        }
        None => VerifyStatus::VerificationCodeNotFound,
    };
    Ok(status)
}

async fn show_verification(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(vkey): Path<String>,
) -> Result<impl IntoResponse, error::Error> {
    let status = check_verification_code(&vkey, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(verification_status => status, user => auth.user),
    ))
}

async fn do_verification(key: &str, pool: &Pool<Sqlite>) -> Result<VerifyStatus, error::Error> {
    let mut status = check_verification_code(key, pool).await?;
    if status == VerifyStatus::VerificationCodeValid {
        sqlx::query!(
            r#"BEGIN TRANSACTION;
            UPDATE sc_user_verification SET uvconfirmed=1 WHERE uvkey=?;
            UPDATE sc_users AS U SET userstatus=?
            FROM ( SELECT userid FROM sc_user_verification WHERE uvkey=?) AS UV
            WHERE U.userid = UV.userid;
            COMMIT;
            "#,
            key,
            UserStatus::Verified as i64,
            key,
        )
        .execute(pool)
        .await?;
        status = VerifyStatus::VerificationSuccessful;
    }
    Ok(status)
}
async fn verify_user(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(vkey): Path<String>,
) -> Result<impl IntoResponse, error::Error> {
    let status = do_verification(&vkey, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(verification_status => status),
    ))
}

#[cfg(test)]
mod test {
    use super::*;
    use test_log::test;

    fn format_sqlite_datetime(date: &OffsetDateTime) -> anyhow::Result<String> {
        date.format(format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .map_err(|e| e.into())
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../../db/fixtures", scripts("users", "sources", "taxa"))
    ))]
    async fn test_verification(pool: Pool<Sqlite>) {
        // expires yesterday
        const KEY1: &str = "aRbitrarykeyvalue21908fs0fqwaerilkiljanslaoi";
        // expires in an hour
        const KEY2: &str = "aRbitrarykeyvaluej0asvdo-q134f@#$%@~!3r42i1o";
        const USERID1: i64 = 1;

        let yesterday = OffsetDateTime::now_utc() - Duration::new(24 * 60 * 60, 0);
        let yesterday = format_sqlite_datetime(&yesterday).expect("unable to format timestamp");
        let now = OffsetDateTime::now_utc();
        let now = format_sqlite_datetime(&now).expect("unable to format timestamp");
        sqlx::query!(
            r#"INSERT INTO sc_user_verification
                (uvid, userid, uvkey, uvrequested, uvexpiration, uvconfirmed)
            VALUES
                (1, ?, ?, ?, 0, 0);
            INSERT INTO sc_user_verification
                (uvid, userid, uvkey, uvrequested, uvexpiration, uvconfirmed)
            VALUES
                (2, ?, ?, ?, 1, 0)"#,
            USERID1,
            KEY1,
            yesterday,
            USERID1,
            KEY2,
            now,
        )
        .execute(&pool)
        .await
        .expect("Failed to insert user verification rows");

        assert_eq!(
            VerifyStatus::VerificationCodeNotFound,
            do_verification("NON-EXISTENT KEY", &pool)
                .await
                .expect("Failed to do verification"),
        );
        assert_eq!(
            VerifyStatus::VerificationCodeExpired,
            do_verification(KEY1, &pool)
                .await
                .expect("Failed to do verification"),
        );

        // make sure that the user is unverified before this
        let user = User::load(USERID1, &pool)
            .await
            .expect("Failed to load user");
        assert_eq!(UserStatus::Unverified, user.status);
        assert_eq!(
            VerifyStatus::VerificationSuccessful,
            do_verification(KEY2, &pool)
                .await
                .expect("Failed to do verification"),
        );

        let row = sqlx::query_as!(
            VerificationRow,
            "SELECT * FROM sc_user_verification WHERE uvid=?",
            2
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch verification row");
        assert_eq!(2, row.uvid);
        assert_eq!(KEY2, row.uvkey);
        assert_eq!(1, row.uvconfirmed);
        let user = User::load(USERID1, &pool)
            .await
            .expect("Failed to load user");
        assert_eq!(UserStatus::Verified, user.status);
    }
}
