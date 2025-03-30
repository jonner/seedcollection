use crate::{
    Error,
    core::{
        error::VerificationError,
        loadable::Indexable,
        query::{CompoundFilter, Op},
    },
};
use async_trait::async_trait;
use rand::{
    distributions::{Alphanumeric, DistString},
    rngs::OsRng,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Sqlite, sqlite::SqliteQueryResult};
use time::{Duration, OffsetDateTime};
use tracing::debug;

use crate::{
    Database, Result,
    core::{
        loadable::{ExternalRef, Loadable},
        query::{DynFilterPart, FilterPart},
    },
    user::User,
};

#[derive(FromRow, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserVerification {
    /// the database ID for this user
    #[sqlx(rename = "uvid")]
    pub id: i64,
    /// The user associated with this verification request
    #[sqlx(rename = "userid")]
    pub user: ExternalRef<User>,
    /// The randomly-generated key associated with this verification request
    #[sqlx(rename = "uvkey")]
    pub key: String,
    /// The date and time that verification was requested
    #[sqlx(rename = "uvrequested")]
    pub requested: Option<OffsetDateTime>,
    /// Number of hours after request time that the verification key expires
    #[sqlx(rename = "uvexpiration")]
    pub expiration: i64,
    /// Whether the user completed this verification request
    #[sqlx(rename = "uvconfirmed")]
    pub confirmed: bool,
}

impl UserVerification {
    const DEFAULT_EXPIRATION: i64 = 10 * 60 * 60;

    /// Generate a new code that can be used to verify a user account.
    fn new_key() -> String {
        Alphanumeric.sample_string(&mut OsRng, 24)
    }

    /// Create a new user verification request for the given user
    pub fn new(user: ExternalRef<User>, expiration: Option<i64>) -> Self {
        Self {
            id: <Self as Loadable>::Id::invalid_value(),
            user,
            key: Self::new_key(),
            requested: None,
            expiration: expiration.unwrap_or(Self::DEFAULT_EXPIRATION),
            confirmed: false,
        }
    }

    fn query_builder<'q>(filter: Option<DynFilterPart>) -> QueryBuilder<'q, Sqlite> {
        let mut builder = QueryBuilder::new(
            r#"SELECT
            *
            FROM
                sc_user_verification"#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder
    }

    /// Insert this new user verification request into the database
    pub async fn insert(&mut self, db: &Database) -> Result<()> {
        if self.id != Self::invalid_id() {
            return Err(Error::InvalidInsertObjectAlreadyExists(self.id));
        }
        self.requested = Some(OffsetDateTime::now_utc());
        debug!(?self, "Inserting user verification into database");
        let uv = sqlx::query_as(
            r#"INSERT INTO sc_user_verification
            (userid, uvkey, uvexpiration)
            VALUES (?, ?, ?) RETURNING *"#,
        )
        .bind(self.user.id())
        .bind(&self.key)
        .bind(self.expiration)
        .fetch_one(db.pool())
        .await?;
        *self = uv;
        Ok(())
    }

    /// Update this user verification request in the database
    pub async fn update(&self, db: &Database) -> Result<()> {
        if self.id == Self::invalid_id() {
            return Err(Error::InvalidUpdateObjectNotFound);
        }
        if self.requested.is_none() {
            return Err(Error::InvalidStateMissingAttribute("request date".into()));
        }
        debug!(?self, "Updating user verification in database");
        sqlx::query(
            r#"UPDATE sc_user_verification
            SET userid=?, uvkey=?, uvrequested=?, uvexpiration=?, uvconfirmed=? WHERE uvid=?
            RETURNING *"#,
        )
        .bind(self.user.id())
        .bind(&self.key)
        .bind(self.requested)
        .bind(self.expiration)
        .bind(self.confirmed)
        .bind(self.id)
        .execute(db.pool())
        .await?;
        Ok(())
    }

    /// Search the database for a user verification request with the given key
    pub async fn find(
        userid: i64,
        key: &str,
        db: &Database,
    ) -> Result<UserVerification, VerificationError> {
        let f = CompoundFilter::builder(Op::And)
            .push(Filter::Key(key.into()))
            .push(Filter::Userid(userid))
            .build();
        let mut uvs: Vec<UserVerification> = Self::query_builder(Some(f))
            .build_query_as()
            .fetch_all(db.pool())
            .await
            .map_err(VerificationError::InternalError)?;
        match uvs.len() {
            0 => Err(VerificationError::KeyNotFound),
            1 => {
                let uv = uvs.pop().unwrap();
                if uv.confirmed {
                    Err(VerificationError::AlreadyVerified)
                } else if uv.is_expired() {
                    Err(VerificationError::Expired)
                } else {
                    Ok(uv)
                }
            }
            _ => Err(VerificationError::MultipleKeysFound),
        }
    }

    /// Mark this verification request as verified
    pub async fn verify(&mut self, db: &Database) -> Result<()> {
        self.confirmed = true;
        self.update(db).await
    }

    /// Check if this user verification request has expired
    fn is_expired(&self) -> bool {
        self.requested
            .map(|requested| {
                (requested + Duration::hours(self.expiration)) < OffsetDateTime::now_utc()
            })
            // if no requested date was set, just consider the request to be expired
            .unwrap_or(true)
    }
}

#[async_trait]
impl Loadable for UserVerification {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn load(id: Self::Id, db: &Database) -> Result<Self> {
        Self::query_builder(Some(Filter::Id(id).into()))
            .build_query_as()
            .fetch_one(db.pool())
            .await
            .map_err(Into::into)
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<SqliteQueryResult> {
        sqlx::query(r#"DELETE FROM sc_user_verification WHERE uvid=?"#)
            .bind(id)
            .execute(db.pool())
            .await
            .map_err(|e| e.into())
    }
}

pub enum Filter {
    Id(i64),
    Userid(i64),
    Key(String),
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Filter::Id(id) => builder.push(" uvid=").push_bind(*id),
            Filter::Userid(id) => builder.push(" userid=").push_bind(*id),
            Filter::Key(key) => builder.push(" uvkey=").push_bind(key.clone()),
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::user::UserStatus;

    use super::*;
    use sqlx::Pool;
    use test_log::test;
    // already expired
    const EXPIRED_KEY: &str = "aRbitrarykeyvalue21908fs0fqwaerilkiljanslaoi";
    // expires in an hour
    const KEY: &str = "aRbitrarykeyvaluej0asvdo-q134f@#$%@~!3r42i1o";
    // user id associated with the valid key
    const USERID: i64 = 1;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../../db/fixtures", scripts("users"))
    ))]
    async fn create_user_verification(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        let mut uv = UserVerification::new(ExternalRef::Stub(1), None);
        uv.insert(&db)
            .await
            .expect("Failed to insert user verification");

        let loaded = UserVerification::load(uv.id, &db)
            .await
            .expect("Unable to load new user");
        assert_eq!(uv.id, loaded.id);
        assert_eq!(uv.user.id(), loaded.user.id());
        assert_eq!(uv.key, loaded.key);
        assert_eq!(uv.requested, loaded.requested);
        assert_eq!(uv.expiration, loaded.expiration);
        assert_eq!(uv.confirmed, loaded.confirmed);
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../../db/fixtures", scripts("users"))
    ))]
    async fn modify_user_verification(pool: Pool<Sqlite>) {
        let _db = Database::from(pool);
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../../db/fixtures", scripts("users"))
    ))]
    async fn delete_user_verification(pool: Pool<Sqlite>) {
        let _db = Database::from(pool);
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "user-verifications")
        )
    ))]
    async fn test_missing_key(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        let res = UserVerification::find(USERID, "NON-EXISTENT KEY", &db).await;
        let err = res.expect_err("Should have failed");
        println!("{err:?}");
        assert!(matches!(err, VerificationError::KeyNotFound));
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "user-verifications")
        )
    ))]
    async fn test_wrong_user(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        let res = UserVerification::find(USERID + 1, KEY, &db).await;
        let err = res.expect_err("Should have failed");
        println!("{err:?}");
        assert!(matches!(err, VerificationError::KeyNotFound));
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "user-verifications")
        )
    ))]
    async fn test_expired(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        // expires yesterday

        let res = UserVerification::find(USERID, EXPIRED_KEY, &db).await;
        let err = res.expect_err("Should have failed");
        assert!(matches!(err, VerificationError::Expired));
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "user-verifications")
        )
    ))]
    async fn test_user_verify(pool: Pool<Sqlite>) {
        let db = Database::from(pool);

        // make sure that the user is unverified before this
        let user = User::load(USERID, &db).await.expect("Failed to load user");
        assert_eq!(UserStatus::Unverified, user.status);
        let mut uv = UserVerification::find(USERID, KEY, &db)
            .await
            .expect("Failed to find user verification request");
        assert!(!uv.confirmed);
        let uvid = uv.id;
        uv.verify(&db).await.expect("Failed to verify request");

        // re-load from db
        let uv = UserVerification::load(uvid, &db)
            .await
            .expect("Failed to find user verification request");
        // make sure that the verification request is marked as confirmed
        assert_eq!(2, uv.id);
        assert_eq!(KEY, uv.key);
        assert!(uv.confirmed);
    }
}
