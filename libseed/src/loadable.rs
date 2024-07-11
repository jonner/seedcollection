//! Utilities that provide an abstraction that allow one ofject to store a reference to another
//! object, which could be either a stub or a full object. This allows you to sometimes query with
//! joins to other tables and sometimes just query the single table while still making it easy to
//! fetch the referenced object later.

use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Pool, Sqlite};

#[async_trait]
pub trait Loadable {
    type Id: Clone + Send;

    fn invalid_id() -> Self::Id;
    fn id(&self) -> Self::Id;
    fn set_id(&mut self, id: Self::Id);
    async fn load(id: Self::Id, pool: &Pool<Sqlite>) -> Result<Self>
    where
        Self: Sized;

    async fn delete(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        Self::delete_id(&self.id(), pool).await.map(|r| {
            self.set_id(Self::invalid_id());
            r
        })
    }

    async fn delete_id(id: &Self::Id, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult>;
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum ExternalRef<T: Loadable + Sync + Send> {
    Stub(T::Id),
    Object(T),
}

impl<T: Loadable + Sync + Send> ExternalRef<T> {
    pub fn is_stub(&self) -> bool {
        matches!(*self, Self::Stub(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(*self, Self::Object(_))
    }

    pub fn object(&self) -> Result<&T> {
        match self {
            Self::Object(obj) => Ok(obj),
            _ => Err(Error::InvalidStateNotLoaded),
        }
    }

    pub fn object_mut(&mut self) -> Result<&mut T> {
        match self {
            Self::Object(obj) => Ok(obj),
            _ => Err(Error::InvalidStateNotLoaded),
        }
    }

    pub fn id(&self) -> T::Id {
        match *self {
            Self::Stub(ref id) => id.clone(),
            Self::Object(ref obj) => obj.id(),
        }
    }

    pub async fn load(&mut self, pool: &Pool<Sqlite>) -> Result<&T> {
        match self {
            Self::Stub(id) => {
                let obj = T::load(id.clone(), pool).await?;
                *self = Self::Object(obj);
                self.object()
            }
            Self::Object(ref obj) => Ok(obj),
        }
    }

    pub async fn delete(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        match self {
            Self::Stub(id) => T::delete_id(id, pool).await,
            Self::Object(obj) => obj.delete(pool).await,
        }
    }
}
