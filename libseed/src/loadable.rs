//! Utilities that provide an abstraction that allow one ofject to store a reference to another
//! object, which could be either a stub or a full object. This allows you to sometimes query with
//! joins to other tables and sometimes just query the single table while still making it easy to
//! fetch the referenced object later.

use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Pool, Sqlite};

/// A trait that the [Loadable::Id] type must implement to be used with Loadable.
pub trait Indexable {
    /// provide a value for this type that will be treated as an invalid or
    /// uninintialized value. This value should never be present in the database.
    fn invalid_value() -> Self;
}

impl Indexable for i64 {
    fn invalid_value() -> Self {
        -1
    }
}

#[async_trait]
pub trait Loadable {
    /// The type of the ID for this object in the database
    type Id: Clone + Send + Indexable;

    /// return the ID associated with this particular object
    fn id(&self) -> Self::Id;
    /// Set the ID of this particular object to `id`
    fn set_id(&mut self, id: Self::Id);
    /// Load the object with the given `id` from the database
    async fn load(id: Self::Id, pool: &Pool<Sqlite>) -> Result<Self>
    where
        Self: Sized;

    /// Convenience function to delete the object with the id `self.id()` from
    /// the database
    async fn delete(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        Self::delete_id(&self.id(), pool)
            .await
            .inspect(|_| self.set_id(Self::Id::invalid_value()))
    }

    /// Delete the object with the id `id` from the database
    async fn delete_id(id: &Self::Id, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult>;

    fn invalid_id() -> Self::Id {
        Self::Id::invalid_value()
    }
}

/// An object that represents a reference to a different database object of type T
#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum ExternalRef<T: Loadable + Sync + Send> {
    /// If the object has not been loaded from the database, it is just
    /// represented as a 'stub' object that holds an id
    Stub(T::Id),
    /// If the object has been loaded from the database, it is represented as an
    /// object of type T.
    Object(T),
}

impl<T: Loadable + Sync + Send> ExternalRef<T> {
    /// whether the Loadable variable is a stub object
    pub fn is_stub(&self) -> bool {
        matches!(*self, Self::Stub(_))
    }

    /// whether the Loadable variable is a full object
    pub fn is_object(&self) -> bool {
        matches!(*self, Self::Object(_))
    }

    /// Return the object referenced by `self`, or error if the object is not
    /// yet loaded
    pub fn object(&self) -> Result<&T> {
        match self {
            Self::Object(obj) => Ok(obj),
            _ => Err(Error::InvalidStateNotLoaded),
        }
    }

    /// Return a mutable object that can be used for updating the object owned by `self`.
    /// See also [ExternalRef::object()]
    pub fn object_mut(&mut self) -> Result<&mut T> {
        match self {
            Self::Object(obj) => Ok(obj),
            _ => Err(Error::InvalidStateNotLoaded),
        }
    }

    /// Return the id of this ExternalRef object
    pub fn id(&self) -> T::Id {
        match *self {
            Self::Stub(ref id) => id.clone(),
            Self::Object(ref obj) => obj.id(),
        }
    }

    /// Load the object from the database and update the object to contain the
    /// newly-loaded referenced object
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

    /// Same as [ExternalRef::load()], but returns a mutable object
    pub async fn load_mut(&mut self, pool: &Pool<Sqlite>) -> Result<&mut T> {
        match self {
            Self::Stub(id) => {
                let obj = T::load(id.clone(), pool).await?;
                *self = Self::Object(obj);
                self.object_mut()
            }
            Self::Object(ref mut obj) => Ok(obj),
        }
    }

    /// Delete the referenced object from the database
    pub async fn delete(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        match self {
            Self::Stub(id) => T::delete_id(id, pool).await,
            Self::Object(obj) => obj.delete(pool).await,
        }
    }
}
