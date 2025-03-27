use crate::{
    Error, Result,
    taxonomy::Taxon,
    user::{User, UserStatus},
};

use sqlparser::{dialect::SQLiteDialect, parser::Parser};
use sqlx::{Connection, FromRow, Pool, Row, Sqlite, SqlitePool, sqlite::SqliteConnectOptions};
use std::path::{Path, PathBuf};
use tracing::{debug, trace, warn};

/// Representation of changes to a taxon after a database upgrade
#[derive(Debug)]
pub struct TaxonChange {
    /// The taxon that will be affected
    pub taxon: Taxon,
    /// The properties that will change after the database upgrade
    pub changes: Vec<PropertyChange>,
}

/// A Representation of a change to a single property of a taxon
#[derive(Debug)]
pub struct PropertyChange {
    /// The name of the property that will change after a database upgrade
    pub property_name: String,
    /// The value of the property in the old taxonomy database
    pub old_value: String,
    /// The value of the property in the new taxonomy database
    pub new_value: String,
}

/// A representation of a taxon replacement in a database upgrade
#[derive(Debug)]
pub struct TaxonReplacement {
    /// The taxon in the old database that will be replaced
    pub old: Taxon,
    /// The taxon in the new database that will be used after upgrading
    pub new: Taxon,
}

/// A Summary of changes between different versions of the taxonomy database
#[derive(Debug, Default)]
pub struct DatabaseUpgradeSummary {
    /// A list of taxa which contain property changes between the old and new taxonomy databases
    pub changes: Vec<TaxonChange>,
    /// A list of taxa which were renamed or moved to a new taxon between the
    /// old and new taxonomy databases
    pub replacements: Vec<TaxonReplacement>,
    /// A list of taxa which are considered obsolete in the new database and for
    /// which no replacement taxon could be determined
    pub invalid: Vec<Taxon>,
}

impl DatabaseUpgradeSummary {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.replacements.is_empty() && self.invalid.is_empty()
    }
}

/// An enumeration for specifying which action to take for a database upgrade
#[derive(Debug, PartialEq)]
pub enum UpgradeAction {
    /// Abort the database upgrade
    Abort,
    /// Go ahead with the database upgrade
    Proceed,
}

/// An object that represents a connection to the seed collection database
#[derive(Clone, Debug)]
pub struct Database(Pool<Sqlite>);

impl From<Pool<Sqlite>> for Database {
    /// **WARNING**: This is primarily intended for tests. You should probably
    /// use [Database::open()] instead of creating the pool yourself, since
    /// [Database::open()] will perform database schema migration automatically.
    fn from(value: Pool<Sqlite>) -> Self {
        Self(value)
    }
}

impl Database {
    /// Open a connection to the specified database. This will also perform any
    /// necessary sql migrations to ensure that the database is up to date with the
    /// latest schema changes.
    pub async fn open<P: AsRef<Path>>(db: P) -> Result<Self, sqlx::Error> {
        let dbpool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(db)).await?;
        trace!("Running database migrations");
        sqlx::migrate!("../db/migrations").run(&dbpool).await?;
        Ok(Database(dbpool))
    }

    /// gets a reference to the underlying sqlx connection pool
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.0
    }

    /// Upgrade the database to a new version of the ITIS taxonomy.
    /// `newdbfile` should be a path to the sqlite database downloaded from
    /// the ITIS website. This function will check that the database schema
    /// is compatible between the old and new databases and compiles a
    /// list of changes that will be made to the database. The `resolve_fn`
    /// allows the caller to examine the potential changes to the database
    /// before proceeding with the upgrade. If this function returns
    /// [UpgradeAction::Proceed], the upgrade will continue.
    pub async fn upgrade<F>(&self, newdbfile: PathBuf, resolve_fn: F) -> Result<()>
    where
        F: Fn(&DatabaseUpgradeSummary) -> UpgradeAction,
    {
        // attach the new database to the connection: https://www.sqlite.org/lang_attach.html
        let poolconn = self.pool().acquire().await?;
        let mut conn = poolconn.detach();
        debug!("attaching database {newdbfile:?} to connection");
        let _q = sqlx::query("ATTACH DATABASE ? as 'newdb'")
            .bind(newdbfile.to_string_lossy())
            .execute(&mut conn)
            .await?;

        let tables = sqlx::query("SELECT name FROM newdb.sqlite_master WHERE type='table'")
            .fetch_all(&mut conn)
            .await?
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect::<Vec<_>>();
        for table in tables.iter() {
            check_table(&mut conn, table).await?;
        }

        debug!("All tables have compatible schema, proceeding with upgrade check");
        let mut upgrade_summary = DatabaseUpgradeSummary::default();

        // Look up taxa that we're using in our collection (and supporting
        // tables) that are now marked as "not accepted" in the new database
        let obsolete_taxa: Vec<Taxon> = sqlx::query_as("SELECT * FROM taxonomic_units T WHERE T.name_usage IS NOT 'accepted' AND tsn in (SELECT tsn FROM mntaxa UNION SELECT tsn from sc_samples UNION SELECT tsn FROM sc_taxon_germination)").fetch_all(&mut conn).await?;
        debug!("{obsolete_taxa:?}");

        for obsolete_taxon in obsolete_taxa {
            match sqlx::query_as("SELECT T.* FROM taxonomic_units T INNER JOIN synonym_links S ON T.tsn=S.tsn_accepted WHERE S.tsn=?")
                .bind(obsolete_taxon.id)
                .fetch_one(&mut conn)
                .await
            {
                Ok(replacement) => upgrade_summary.replacements.push(TaxonReplacement {
                    old: obsolete_taxon,
                    new: replacement,
                }),
                Err(_) => upgrade_summary.invalid.push(obsolete_taxon),
            }
        }

        let str_fields = &[
            "unit_name1",
            "unit_name2",
            "unit_name3",
            "name_usage",
            "unaccept_reason",
            "update_date",
            "complete_name",
        ];
        let int_fields = &["parent_tsn", "kingdom_id", "rank_id"];
        let fields_to_compare = str_fields
            .iter()
            .chain(int_fields.iter())
            .collect::<Vec<_>>();
        let querystr = "SELECT T.*, ".to_string()
            + &fields_to_compare
                .into_iter()
                .map(|f| format!("NEWT.{f} as new_{f}"))
                .collect::<Vec<_>>()
                .join(", ")
            + " FROM taxonomic_units T INNER JOIN newdb.taxonomic_units NEWT ON T.tsn=NEWT.tsn WHERE T.tsn in (SELECT DISTINCT tsn from sc_samples) AND T.name_usage IS 'accepted'";

        // Look up all taxa that have changed between the current database and
        // the new database and present differences to the user
        let taxon_compare = sqlx::query(&querystr).fetch_all(&mut conn).await?;
        for taxon_row in taxon_compare {
            let taxon = Taxon::from_row(&taxon_row)?;
            let mut field_changes = Vec::default();
            for field in str_fields {
                let (old, new): (&str, &str) = (
                    taxon_row.try_get(field)?,
                    taxon_row.try_get(format!("new_{field}").as_str())?,
                );
                if old != new {
                    field_changes.push(PropertyChange {
                        property_name: field.to_string(),
                        old_value: old.to_string(),
                        new_value: new.to_string(),
                    });
                }
            }
            for field in int_fields {
                let (old, new): (u64, u64) = (
                    taxon_row.try_get(field)?,
                    taxon_row.try_get(format!("new_{field}").as_str())?,
                );
                if old != new {
                    field_changes.push(PropertyChange {
                        property_name: field.to_string(),
                        old_value: old.to_string(),
                        new_value: new.to_string(),
                    });
                }
            }
            if !field_changes.is_empty() {
                upgrade_summary.changes.push(TaxonChange {
                    taxon,
                    changes: field_changes,
                })
            }
        }

        // Present differences to the user to see if the user would like to
        // proceed with the upgrade
        debug!("Calling resolve_fn with summary={upgrade_summary:?}");
        if resolve_fn(&upgrade_summary) == UpgradeAction::Abort {
            return Err(Error::DatabaseUpgrade("Canceled by user".to_string()));
        };
        // now copy the new table over
        sqlx::query("PRAGMA foreign_keys=off")
            .execute(&mut conn)
            .await?;
        let mut tx = conn.begin().await?;
        let res_upgrade = self
            .do_upgrade(&mut tx, tables, upgrade_summary.replacements)
            .await;
        let res_tx = match res_upgrade {
            Ok(_) => tx
                .commit()
                .await
                .map_err(|e| Error::DatabaseUpgrade(format!("Failed to commit transaction: {e}"))),
            Err(_) => tx.rollback().await.map_err(|e| {
                Error::DatabaseUpgrade(format!("Failed to rollback transaction: {e}"))
            }),
        };
        let res_pragma = sqlx::query("PRAGMA foreign_keys=on")
            .execute(&mut conn)
            .await
            .map_err(|e| Error::DatabaseUpgrade(format!("Failed to re-enable foreign keys: {e}")));
        let res_detach = sqlx::query("DETACH DATABASE 'newdb'")
            .execute(&mut conn)
            .await
            .map(|_| ())
            .map_err(|e| Error::DatabaseUpgrade(format!("Failed to detach new database: {e}")));
        debug!("Removing all non-plant taxa to conserve space");
        let res_clean = self.clean_non_plant_taxa().await;
        // It seems that phylo_sort_seq is always 0 in new DB??? Make sure it's set.
        debug!("Updating taxonomic order...");
        let res_order = self.ensure_taxonomic_order().await;
        res_upgrade
            .and(res_tx)
            .and(res_pragma)
            .and(res_detach)
            .and(res_clean)
            .and(res_order)
    }

    pub async fn ensure_taxonomic_order(&self) -> Result<()> {
        sqlx::query("UPDATE taxonomic_units SET phylo_sort_seq = H.rowid FROM (SELECT ROW_NUMBER() OVER (ORDER BY hierarchy_string) AS rowid, tsn FROM hierarchy) as H WHERE H.tsn=taxonomic_units.tsn")
            .execute(&self.0)
            .await.map(|_| ()).map_err(Into::into)
    }

    pub async fn clean_non_plant_taxa(&self) -> Result<()> {
        let mut connection = self.0.acquire().await?;
        sqlx::query(
            "CREATE TEMP TABLE plantids AS
            SELECT T.tsn FROM taxonomic_units T WHERE T.kingdom_id IS ?",
        )
        .bind(crate::taxonomy::KINGDOM_PLANTAE)
        .execute(connection.as_mut())
        .await?;

        // hierarchy
        sqlx::query("DELETE FROM hierarchy WHERE TSN NOT IN (SELECT tsn FROM plantids)")
            .execute(connection.as_mut())
            .await?;
        // jurisdiction
        sqlx::query("DELETE FROM jurisdiction")
            .execute(connection.as_mut())
            .await?;
        // longnames
        sqlx::query("DELETE FROM longnames WHERE tsn NOT IN (SELECT tsn FROM plantids)")
            .execute(connection.as_mut())
            .await?;
        // nodc_ids
        sqlx::query("DELETE FROM nodc_ids")
            .execute(connection.as_mut())
            .await?;
        // reference_links
        sqlx::query("DELETE FROM reference_links")
            .execute(connection.as_mut())
            .await?;
        // synonym_links
        sqlx::query("DELETE FROM synonym_links WHERE tsn NOT IN (SELECT tsn FROM plantids)")
            .execute(connection.as_mut())
            .await?;
        // tu_comments_links (delete comments from here as well?)
        sqlx::query("DELETE FROM comments")
            .execute(connection.as_mut())
            .await?;
        sqlx::query("DELETE FROM tu_comments_links")
            .execute(connection.as_mut())
            .await?;
        // vern_ref_links
        sqlx::query("DELETE FROM vern_ref_links")
            .execute(connection.as_mut())
            .await?;
        // vernaculars
        sqlx::query("DELETE FROM vernaculars WHERE tsn NOT IN (SELECT tsn FROM plantids)")
            .execute(connection.as_mut())
            .await?;
        // taxonomic_units
        sqlx::query("DELETE FROM taxonomic_units WHERE tsn NOT IN (SELECT tsn FROM plantids)")
            .execute(connection.as_mut())
            .await?;
        sqlx::query("DROP TABLE plantids")
            .execute(connection.as_mut())
            .await?;
        sqlx::query("VACUUM").execute(connection.as_mut()).await?;
        Ok(())
    }

    pub async fn init(
        &mut self,
        admin_user: String,
        admin_email: String,
        admin_password: String,
    ) -> Result<User> {
        debug!("Removing all non-plant taxa to conserve space");
        self.clean_non_plant_taxa().await?;
        debug!("ensuring taxonomic order is set");
        self.ensure_taxonomic_order().await?;
        debug!("Initializing database with a new admin user {admin_user} ({admin_email})");
        // hash the password
        let pwhash = User::hash_password(&admin_password)?;

        let mut user = User::new(
            admin_user,
            admin_email,
            pwhash,
            UserStatus::Unverified,
            None,
            None,
            None,
        );
        user.insert(self).await?;
        Ok(user)
    }

    async fn do_upgrade<I: IntoIterator<Item = String>>(
        &self,
        tx: &mut sqlx::SqliteTransaction<'_>,
        tables: I,
        replacements: Vec<TaxonReplacement>,
    ) -> Result<(), Error> {
        println!("Performing upgrade...");
        for table in tables {
            println!(" - migrating database table '{table}'...");
            let create_sql: String = sqlx::query(
                "SELECT sql FROM newdb.sqlite_schema WHERE tbl_name=? AND type='table'",
            )
            .bind(&table)
            .fetch_one(tx.as_mut())
            .await?
            .try_get("sql")?;

            // First drop the old taxonomy table
            sqlx::query(&format!("DROP TABLE {table}"))
                .execute(tx.as_mut())
                .await?;

            // Create a new table from the schema in the new database
            sqlx::query(&create_sql).execute(tx.as_mut()).await?;

            // copy the data over to the new table
            sqlx::query(&format!("INSERT INTO {table} SELECT * FROM newdb.{table}"))
                .execute(tx.as_mut())
                .await?;
        }

        // Now update all of the changed taxa in the collection
        for replacement in replacements {
            println!("Fixup up {}...", replacement.new.complete_name);
            sqlx::query("UPDATE sc_samples SET tsn=? WHERE tsn=?")
                .bind(replacement.new.id)
                .bind(replacement.old.id)
                .execute(tx.as_mut())
                .await?;
            sqlx::query("UPDATE sc_taxon_germination SET tsn=? WHERE tsn=?")
                .bind(replacement.new.id)
                .bind(replacement.old.id)
                .execute(tx.as_mut())
                .await?;
            sqlx::query("UPDATE mntaxa SET tsn=? WHERE tsn=?")
                .bind(replacement.new.id)
                .bind(replacement.old.id)
                .execute(tx.as_mut())
                .await?;
        }
        sqlx::query("PRAGMA foreign_key_check")
            .execute(tx.as_mut())
            .await?;
        Ok(())
    }
}

pub(crate) async fn check_table(
    conn: &mut sqlx::SqliteConnection,
    table_name: &str,
) -> Result<(), Error> {
    let create_old: String =
        sqlx::query("SELECT sql FROM sqlite_schema WHERE tbl_name=? AND type='table'")
            .bind(table_name)
            .fetch_one(&mut *conn)
            .await?
            .try_get("sql")?;
    let create_new: String =
        sqlx::query("SELECT sql FROM newdb.sqlite_schema WHERE tbl_name=? AND type='table'")
            .bind(table_name)
            .fetch_one(&mut *conn)
            .await?
            .try_get("sql")?;
    let dialect = SQLiteDialect {};
    let old_sql = Parser::parse_sql(&dialect, &create_old)
        .map_err(|e| Error::DatabaseUpgrade(format!("Unable to determine new table sql: {e}")))?;
    let new_sql = Parser::parse_sql(&dialect, &create_new)
        .map_err(|e| Error::DatabaseUpgrade(format!("Unable to determine new table sql: {e}")))?;
    if old_sql != new_sql {
        warn!("database schema mismatch\n  old: <<{create_old}>>\n  new: <<{create_new}>>");
        return Err(Error::DatabaseUpgrade(format!(
            "Database schema mismatch for table '{table_name}'"
        )));
    }
    Ok(())
}
