use anyhow::anyhow;
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};

#[async_trait]
pub trait Loadable: Default {
    type Id;

    fn new_loadable(id: Self::Id) -> Self;
    fn is_loaded(&self) -> bool;
    fn is_loadable(&self) -> bool;
    async fn do_load(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<Self>;

    async fn load(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<()> {
        if !self.is_loadable() {
            return Err(anyhow!("object cannot be loaded"));
        }
        let x = self.do_load(pool).await?;
        *self = x;
        Ok(())
    }

    /*
    async fn do_delete(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<()>;
    async fn delete(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<()> {
        if !self.is_loadable() {
            return Err(anyhow!("object cannot be deleted"));
        }
        self.do_delete(pool).await
    }
    */
}
