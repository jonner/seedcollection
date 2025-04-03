//! Objects representing a user of the application
use crate::core::{
    database::Database,
    error::{Error, Result},
    loadable::{ExternalRef, Indexable, Loadable},
    query::{DynFilterPart, FilterPart},
};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use async_trait::async_trait;
use password_hash::{PasswordHash, SaltString, rand_core::OsRng};
use serde::{Deserialize, Serialize};
use sqlx::{
    QueryBuilder, Sqlite,
    prelude::*,
    sqlite::{SqliteQueryResult, SqliteRow},
};
use time::OffsetDateTime;
use tracing::debug;
use verification::UserVerification;

pub mod verification;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[repr(i64)]
pub enum UserStatus {
    Unverified = 0,
    Verified = 1,
}

enum Filter {
    Id(i64),
    Username(String),
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut QueryBuilder<Sqlite>) {
        match self {
            Filter::Id(id) => builder.push(" userid=").push_bind(*id),
            Filter::Username(name) => builder.push(" username=").push_bind(name.clone()),
        };
    }
}

/// A website user that is stored in the database. Each object in the database is associated with a
/// particular user.
#[derive(FromRow, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    /// the database ID for this user
    #[sqlx(rename = "userid")]
    pub id: i64,

    /// the username for this user
    pub username: String,

    /// the email address for this user
    #[sqlx(rename = "useremail")]
    pub email: String,

    /// The status of this user
    #[sqlx(rename = "userstatus")]
    pub status: UserStatus,

    /// The date that the user registered their account
    #[sqlx(rename = "usersince")]
    pub register_date: Option<OffsetDateTime>,

    /// A display name for the user
    #[sqlx(rename = "userdisplayname")]
    pub display_name: Option<String>,

    /// Some text describing a bit about the user, written by the user themselves
    #[sqlx(rename = "userprofile", default)]
    pub profile: Option<String>,

    #[serde(skip_serializing)]
    /// a hashed password for use when authenticating a user
    pub pwhash: String,
}

#[async_trait]
impl Loadable for User {
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
            .map_err(|e| e.into())
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<SqliteQueryResult> {
        sqlx::query("DELETE FROM sc_users WHERE userid=?")
            .bind(id)
            .execute(db.pool())
            .await
            .map_err(|e| e.into())
    }
}

impl User {
    fn query_builder(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder = QueryBuilder::new(
            r#"SELECT
                userid,
                username,
                useremail,
                pwhash,
                userstatus,
                usersince,
                userdisplayname,
                userprofile
            FROM
                sc_users"#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" ORDER BY username ASC");
        builder
    }

    /// Fetch all users from the database
    pub async fn load_all(db: &Database) -> Result<Vec<User>> {
        Self::query_builder(None)
            .build_query_as()
            .fetch_all(db.pool())
            .await
            .map_err(|e| e.into())
    }

    /// Fetch the user with the given username from the database
    pub async fn load_by_username(username: &str, db: &Database) -> sqlx::Result<Option<User>> {
        Self::query_builder(Some(Filter::Username(username.to_string()).into()))
            .build_query_as()
            .fetch_optional(db.pool())
            .await
    }

    /// Update the database to match the values currently stored in the object
    pub async fn update(&self, db: &Database) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidUpdateObjectNotFound);
        }

        debug!(?self, "Updating user in database");
        sqlx::query(
            "UPDATE
                        sc_users
                    SET
                        username=?,
                        useremail=?,
                        userstatus=?,
                        userdisplayname=?,
                        userprofile=?,
                        pwhash=?
                    WHERE
                        userid=?",
        )
        .bind(&self.username)
        .bind(&self.email)
        .bind(&self.status)
        .bind(&self.display_name)
        .bind(&self.profile)
        .bind(&self.pwhash)
        .bind(self.id)
        .execute(db.pool())
        .await
        .map_err(|e| e.into())
    }

    /// A helper function to hash a password with a randomly generated salt using the Argon2 hasher
    pub fn hash_password(pw: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hasher = Argon2::default();
        Ok(hasher.hash_password(pw.as_bytes(), &salt)?.to_string())
    }

    /// Use the provided parameters from this user's password hash to hash the supplied password
    /// and compare them to see whether this is the correct password.
    pub fn verify_password(&self, pw: &str) -> Result<()> {
        let hasher = Argon2::default();
        let expected_hash = PasswordHash::new(&self.pwhash)?;
        hasher
            .verify_password(pw.as_bytes(), &expected_hash)
            .map_err(|e| e.into())
    }

    /// hash the given password with a random salt and store it inside the User object.
    pub fn change_password(&mut self, pw: &str) -> Result<()> {
        self.pwhash = Self::hash_password(pw)?;
        Ok(())
    }

    /// create a new object with the given values
    pub fn new(
        username: String,
        email: String,
        pwhash: String,
        status: UserStatus,
        register_date: Option<OffsetDateTime>,
        display_name: Option<String>,
        profile: Option<String>,
    ) -> Self {
        Self {
            id: Self::invalid_id(),
            username,
            email,
            pwhash,
            status,
            register_date,
            display_name,
            profile,
        }
    }

    /// Insert a new row into the database with the values stored in this object
    pub async fn insert(&mut self, db: &Database) -> Result<()> {
        if self.username.trim().is_empty() {
            return Err(Error::InvalidStateMissingAttribute("username".to_string()));
        }
        if self.email.trim().is_empty() {
            return Err(Error::InvalidStateMissingAttribute("email".to_string()));
        }
        debug!(?self, "Inserting user into database");
        // Don't insert the register_date, the database will set it to the current timestamp
        let user = sqlx::query_as(
            r#"INSERT INTO
                sc_users
                (
                    username,
                    useremail,
                    pwhash,
                    userstatus,
                    userdisplayname,
                    userprofile
                )
                VALUES (?, ?, ?, ?, ?, ?)
                RETURNING *"#,
        )
        .bind(&self.username)
        .bind(&self.email)
        .bind(&self.pwhash)
        .bind(&self.status)
        .bind(&self.display_name)
        .bind(&self.profile)
        .fetch_one(db.pool())
        .await?;
        *self = user;
        Ok(())
    }

    pub fn validate_username(username: &str) -> Result<()> {
        if username.len() < 5 {
            return Err(Error::AuthInvalidUsernameTooShort);
        }

        let mut chars = username.chars();
        match chars.next() {
            Some(first_char) => {
                if !first_char.is_alphanumeric() {
                    return Err(Error::AuthInvalidUsernameFirstCharacter);
                }
            }
            // this should never happen since we checked length above
            None => {
                return Err(Error::AuthInvalidUsernameTooShort);
            }
        }

        let allowed_chars = "@.-_";
        if !chars.all(|c| c.is_alphanumeric() || allowed_chars.contains(c)) {
            return Err(Error::AuthInvalidUsernameInvalidCharacters(format!(
                "Usernames can only contain alphanumeric characters or one of {}",
                allowed_chars
                    .chars()
                    .map(|c| format!("'{c}'"))
                    .collect::<Vec<String>>()
                    .join(", ")
            )));
        }
        Ok(())
    }

    pub async fn verify(&mut self, key: &str, db: &Database) -> Result<()> {
        let mut uv = UserVerification::find(self.id, key, db).await?;
        uv.verify(db).await?;
        self.status = UserStatus::Verified;
        self.update(db).await.map(|_| ())
    }

    pub async fn generate_verification_request(&self, db: &Database) -> Result<UserVerification> {
        if !self.id() == <Self as Loadable>::Id::invalid_value() {
            return Err(Error::InvalidUpdateObjectNotFound);
        }
        let mut uv = UserVerification::new(self.clone().into(), None);
        uv.insert(db).await?;
        Ok(uv)
    }
}

impl FromRow<'_, SqliteRow> for ExternalRef<User> {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(match User::from_row(row) {
            Ok(user) => ExternalRef::Object(user),
            Err(_) => {
                let id = row.try_get("userid")?;
                ExternalRef::Stub(id)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Pool;
    use test_log::test;

    #[test(sqlx::test(migrations = "../db/migrations/",))]
    async fn register_user(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        const PASSWORD: &str = "my-super-secret-password";
        let hash = User::hash_password(PASSWORD).expect("Failed to hash password");
        let mut user = User::new(
            "my-user-name".to_string(),
            "my-address@domain.co.uk".to_string(),
            hash,
            UserStatus::Unverified,
            None,
            None,
            None,
        );
        user.insert(&db).await.expect("Failed to insert user");

        let loaded = User::load(user.id, &db)
            .await
            .expect("Unable to load new user");
        assert_eq!(user.id, loaded.id);
        assert_eq!(user.username, loaded.username);
        assert_eq!(user.email, loaded.email);
        assert_eq!(user.pwhash, loaded.pwhash);
        assert_eq!(user.status, loaded.status);
        assert_eq!(user.register_date, loaded.register_date);
        assert_eq!(user.display_name, loaded.display_name);
        assert_eq!(user.profile, loaded.profile);
        assert!(loaded.verify_password(PASSWORD).is_ok());
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("users"))
    ))]
    async fn modify_user(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        const NEWNAME: &str = "TestUsername84902";
        let mut user = User::load(1, &db)
            .await
            .expect("Failed to fetch user from database");
        user.username = NEWNAME.to_string();
        user.update(&db).await.expect("Unable to update user");
        assert!(user.insert(&db).await.is_err());

        let loaded = User::load(1, &db)
            .await
            .expect("Unable to load updated user");
        assert_eq!(user, loaded);
        assert_eq!(&loaded.username, NEWNAME);
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("users"))
    ))]
    async fn delete_user(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        User::delete_id(&1, &db)
            .await
            .expect("failed to delete user");
        assert!(User::load(1, &db).await.is_err());
    }

    #[test]
    fn validate_user() {
        // too short
        assert!(User::validate_username("foo").is_err());
        // starts with a non-alphanumeric character
        assert!(User::validate_username("-foobar").is_err());
        assert!(User::validate_username("_foobar").is_err());
        assert!(User::validate_username(".foobar").is_err());
        assert!(User::validate_username(" foobar").is_err());
        // contains a space
        assert!(User::validate_username("foo bar").is_err());
        // other character
        assert!(User::validate_username("foo%bar.com").is_err());

        assert!(User::validate_username("foobar").is_ok());
        assert!(User::validate_username("foo-bar").is_ok());
        assert!(User::validate_username("foo_bar").is_ok());
        assert!(User::validate_username("foo.bar").is_ok());
        assert!(User::validate_username("7foobar").is_ok());
        assert!(User::validate_username("foo3bar").is_ok());
        // emails should be ok
        assert!(User::validate_username("foo@bar.com").is_ok());
    }

    #[test]
    fn hash_password() {
        let pw = "my-super-secret-password";
        let user = User::new(
            "my-user-name".to_string(),
            "my-address@domain.co.uk".to_string(),
            User::hash_password(pw).expect("Failed to hash password"),
            UserStatus::Unverified,
            None,
            None,
            None,
        );

        assert!(user.verify_password(pw).is_ok());
        assert!(user.verify_password("wrong-password").is_err());
    }

    #[test]
    fn change_password() {
        let pw = "my-super-secret-password";
        let mut user = User::new(
            "my-user-name".to_string(),
            "my-address@domain.co.uk".to_string(),
            User::hash_password(pw).expect("Failed to hash password"),
            UserStatus::Unverified,
            None,
            None,
            None,
        );

        assert!(user.verify_password(pw).is_ok());
        assert!(user.change_password("new-password").is_ok());
        assert!(user.verify_password("new-password").is_ok());
        assert!(user.verify_password(pw).is_err());
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../db/fixtures",
            scripts("users", "sources", "taxa", "user-verifications")
        )
    ))]
    async fn test_user_verify(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        // expires in an hour
        const KEY: &str = "aRbitrarykeyvaluej0asvdo-q134f@#$%@~!3r42i1o";
        const USERID: i64 = 1;

        // make sure that the user is unverified before this
        let mut user = User::load(USERID, &db).await.expect("Failed to load user");
        assert_eq!(user.status, UserStatus::Unverified);
        user.verify(KEY, &db).await.expect("Failed to verify user");
        assert_eq!(user.status, UserStatus::Verified);
        // re-load from db to make sure it's also updated in the db
        let user = User::load(USERID, &db).await.expect("Failed to load user");
        assert_eq!(user.status, UserStatus::Verified);
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../db/fixtures",
            scripts("users", "sources", "taxa", "user-verifications")
        )
    ))]
    async fn test_user_verify_wrong_user(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        // expires in an hour
        const KEY: &str = "aRbitrarykeyvaluej0asvdo-q134f@#$%@~!3r42i1o";
        const WRONG_USERID: i64 = 2;

        // make sure that the user is unverified before this
        let mut user = User::load(WRONG_USERID, &db)
            .await
            .expect("Failed to load user");
        assert_eq!(user.status, UserStatus::Unverified);
        user.verify(KEY, &db).await.expect_err(
            "We were mistakenly able to verify a verification key for a different user",
        );
        assert_eq!(user.status, UserStatus::Unverified);
        // re-load from db to make sure it's also updated in the db
        let user = User::load(WRONG_USERID, &db)
            .await
            .expect("Failed to load user");
        assert_eq!(user.status, UserStatus::Unverified);
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("users", "sources", "taxa"))
    ))]
    async fn test_user_generate_verification(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        let nuvs = UserVerification::load_all(&db)
            .await
            .expect("Failed to get user verifications from db")
            .len();
        let user = User::load(1, &db).await.expect("Failed to get user list");
        let uv = user
            .generate_verification_request(&db)
            .await
            .expect("Failed to generate verification request");
        let nuvs_after = UserVerification::load_all(&db)
            .await
            .expect("Failed to get user list")
            .len();
        assert_eq!(nuvs + 1, nuvs_after);
        let dbuv = UserVerification::find(user.id, &uv.key, &db)
            .await
            .expect("Failed to load userverification from db");
        assert_eq!(uv, dbuv);
    }
}
