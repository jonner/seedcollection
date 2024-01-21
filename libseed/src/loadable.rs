use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};

#[async_trait]
pub trait Loadable {
    type Id: Clone;

    fn id(&self) -> Self::Id;
    async fn load(id: Self::Id, pool: &Pool<Sqlite>) -> Result<Self>
    where
        Self: Sized;
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
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
            _ => Err(Error::InvalidData("Object is not loaded".to_string())),
        }
    }

    pub fn object_mut(&mut self) -> Result<&mut T> {
        match self {
            Self::Object(obj) => Ok(obj),
            _ => Err(Error::InvalidData("Object is not loaded".to_string())),
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
}
