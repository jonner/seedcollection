use crate::{
    error::{Error, Result},
    filter::{DynFilterPart, FilterPart},
    loadable::Loadable,
};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use async_trait::async_trait;
use password_hash::{rand_core::OsRng, PasswordHash, SaltString};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Pool, QueryBuilder, Sqlite};
use std::sync::Arc;


/// A website user that is stored in the database
#[derive(sqlx::FromRow, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    /// the database ID for this user
    #[sqlx(rename = "userid")]
    pub id: i64,
    /// the username for this user
    pub username: String,
    #[serde(skip_serializing)]
    #[sqlx(default)]
    /// a hashed password for use when authenticating a user
    pub pwhash: String,
}

#[async_trait]
impl Loadable for User {
    type Id = i64;

    fn is_loadable(&self) -> bool {
        self.id > 0
    }

    async fn do_load(&mut self, pool: &Pool<Sqlite>) -> Result<Self> {
        User::fetch(self.id, pool).await
    }

    fn new_loadable(id: Self::Id) -> Self {
        User {
            id,
            username: Default::default(),
            pwhash: Default::default(),
        }
    }
}

enum UserFilter {
    Id(i64),
    Username(String),
}

impl FilterPart for UserFilter {
    fn add_to_query(&self, builder: &mut QueryBuilder<Sqlite>) {
        match self {
            UserFilter::Id(id) => builder.push(" userid=").push_bind(*id),
            UserFilter::Username(name) => builder.push(" username=").push_bind(name.clone()),
        };
    }
}

impl User {
    fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder =
            QueryBuilder::new("SELECT userid, username, pwhash FROM sc_users");
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" ORDER BY username ASC");
        builder
    }

    /// Fetch all users from the database
    pub async fn fetch_all(pool: &Pool<Sqlite>) -> Result<Vec<User>> {
        Self::build_query(None)
            .build_query_as()
            .fetch_all(pool)
            .await
            .map_err(|e| e.into())
    }

    /// Fetch the user with the given id from the database
    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<User> {
        Self::build_query(Some(Arc::new(UserFilter::Id(id))))
            .build_query_as()
            .fetch_one(pool)
            .await
            .map_err(|e| e.into())
    }

    /// Fetch the user with the given username from the database
    pub async fn fetch_by_username(username: &str, pool: &Pool<Sqlite>) -> Result<Option<User>> {
        Self::build_query(Some(Arc::new(UserFilter::Username(username.to_string()))))
            .build_query_as()
            .fetch_optional(pool)
            .await
            .map_err(|e| e.into())
    }

    /// Update the database to match the values currently stored in the object
    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidData("No id set, cannot update".to_string()));
        }

        sqlx::query("UPDATE sc_users SET username=?, pwhash=? WHERE userid=?")
            .bind(&self.username)
            .bind(&self.pwhash)
            .bind(self.id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }

    /// Delete this user from the database
    pub async fn delete(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidData("No id set, cannot delete".to_string()));
        }

        sqlx::query("DELETE FROM sc_users WHERE userid=?")
            .bind(self.id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
            .and_then(|x| {
                self.id = -1;
                Ok(x)
            })
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
    pub fn new(username: String, pwhash: String) -> Self {
        Self {
            id: -1,
            username,
            pwhash,
        }
    }

    /// Insert a new row into the database with the values stored in this object
    pub async fn insert(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        sqlx::query("INSERT INTO sc_users (username, pwhash) VALUES (?, ?)")
            .bind(&self.username)
            .bind(&self.pwhash)
            .execute(pool)
            .await
            .map(|r| {
                self.id = r.last_insert_rowid();
                r
            })
            .map_err(|e| e.into())
    }

    pub fn validate_username(username: &str) -> Result<()> {
        if username.len() < 5 {
            return Err(Error::InvalidData(
                "Usernames must be at least 5 characters long".to_string(),
            ));
        }

        let mut chars = username.chars();
        match chars.next() {
            Some(first_char) => {
                if !first_char.is_alphanumeric() {
                    return Err(Error::InvalidData(
                        "First character of username must be alphanumeric".to_string(),
                    ));
                }
            }
            // this should never happen since we checked length above
            None => {
                return Err(Error::InvalidData(
                    "Username must be at least 5 characters long".to_string(),
                ))
            }
        }

        let allowed_chars = "@.-_";
        if !chars.all(|c| c.is_alphanumeric() || allowed_chars.contains(c)) {
            return Err(Error::InvalidData(format!(
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
}

impl Default for User {
    fn default() -> Self {
        Self {
            id: -1,
            username: Default::default(),
            pwhash: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    #[test(sqlx::test(migrations = "../db/migrations/",))]
    async fn register_user(pool: Pool<Sqlite>) {
        const PASSWORD: &str = "my-super-secret-password";
        let hash = User::hash_password(PASSWORD).expect("Failed to hash password");
        let mut user = User::new("my-user-name".to_string(), hash);
        let res = user.insert(&pool).await.expect("Failed to insert user");
        let userid = res.last_insert_rowid();

        let mut loaded = User::new_loadable(userid);
        loaded.load(&pool).await.expect("Unable to load new user");
        assert_eq!(user, loaded);
        assert!(loaded.verify_password(PASSWORD).is_ok());
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("users"))
    ))]
    async fn modify_user(pool: Pool<Sqlite>) {
        const NEWNAME: &str = "TestUsername84902";
        let mut user = User::fetch(1, &pool)
            .await
            .expect("Failed to fetch user from database");
        user.username = NEWNAME.to_string();
        user.update(&pool).await.expect("Unable to update user");
        assert!(user.insert(&pool).await.is_err());

        let mut loaded = User::new_loadable(1);
        loaded
            .load(&pool)
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
        assert!(User::fetch(1, &pool).await.is_ok());

        let mut user = User::new_loadable(1);
        user.delete(&pool).await.expect("Failed to delete user");
        assert!(User::fetch(1, &pool).await.is_err());
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
}
