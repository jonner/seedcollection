use std::convert;

use inquire::{autocompletion::Autocomplete, CustomUserError};
use libseed::{
    filter::{Cmp, CompoundFilter, Op},
    source::{self, Source},
    taxonomy::{quickfind, Taxon},
};
use sqlx::{Pool, Sqlite};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Internal Error: completion format was incorrect for '{0}'")]
    CompletionIdFormatMissingDot(String),
    #[error("Internal Error: unable to parse an integer from '{0}'")]
    CompletionIdFormatParseFailure(String),
    #[error(transparent)]
    Prompt(#[from] inquire::InquireError),
}

pub struct TaxonIdPrompt<'a> {
    text: inquire::Text<'a>,
}

impl<'a> TaxonIdPrompt<'a> {
    pub fn new(message: &'a str, dbpool: &Pool<Sqlite>) -> Self {
        Self {
            text: inquire::Text::new(message).with_autocomplete(TaxonCompleter {
                dbpool: dbpool.clone(),
            }),
        }
    }

    pub fn prompt(self) -> Result<i64, Error> {
        let res = self.text.prompt()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Genus Species
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        extract_dbid(&res)
    }

    pub fn prompt_skippable(self) -> Option<i64> {
        let res = self.text.prompt_skippable().ok()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Genus Species
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        res.and_then(|val| extract_dbid(&val).ok())
    }
}

#[derive(Clone)]
struct TaxonCompleter {
    dbpool: Pool<Sqlite>,
}

impl Autocomplete for TaxonCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut taxa = Ok(vec![]);
        if input.len() > 2 {
            taxa = futures::executor::block_on(Taxon::load_all(
                quickfind(input.to_string()),
                None,
                &self.dbpool,
            ));
        }
        taxa.map(|taxa| {
            taxa.iter()
                .map(|t| {
                    let mut cnames = t.vernaculars.join(", ");
                    if !cnames.is_empty() {
                        cnames = format!(" - {cnames}");
                    }
                    format!("{:6}. {}{}", t.id, t.complete_name.clone(), cnames)
                })
                .collect::<Vec<String>>()
        })
        .map_err(|e| e.into())
    }

    fn get_completion(
        &mut self,
        _input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, CustomUserError> {
        {
            Ok(highlighted_suggestion)
        }
    }
}

pub struct SourceIdPrompt<'a> {
    text: inquire::Text<'a>,
}

impl<'a> SourceIdPrompt<'a> {
    pub fn new(message: &'a str, userid: i64, dbpool: &Pool<Sqlite>) -> Self {
        Self {
            text: inquire::Text::new(message).with_autocomplete(SourceCompleter {
                dbpool: dbpool.clone(),
                userid,
            }),
        }
    }

    pub fn prompt(self) -> Result<i64, Error> {
        let res = self.text.prompt()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Source name
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        extract_dbid(&res)
    }

    pub fn prompt_skippable(self) -> Option<i64> {
        let res = self.text.prompt_skippable().ok()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Source name
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        res.and_then(|val| extract_dbid(&val).ok())
    }
}

#[derive(Clone)]
struct SourceCompleter {
    dbpool: Pool<Sqlite>,
    userid: i64,
}

impl Autocomplete for SourceCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let fbuilder = CompoundFilter::builder(Op::And)
            .push(source::Filter::UserId(self.userid))
            .push(source::Filter::Name(Cmp::Like, input.to_string()));
        let mut sources = Ok(vec![]);
        if input.len() > 2 {
            sources =
                futures::executor::block_on(Source::load_all(Some(fbuilder.build()), &self.dbpool));
        }
        sources
            .map(|taxa| {
                taxa.iter()
                    .map(|src| format!("{}. {}", src.id, src.name))
                    .collect::<Vec<String>>()
            })
            .map_err(|e| e.into())
    }

    fn get_completion(
        &mut self,
        _input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, CustomUserError> {
        {
            Ok(highlighted_suggestion)
        }
    }
}

// This is a hack that relies on the conventions above for autocompleting items from the database.
// The autocompletion suggestions have the form "$DBID. $DESCRIPTION"
// This code simply splits the string at the first '.' character and returns the ID before that.
fn extract_dbid(s: &str) -> Result<i64, Error> {
    s.split('.')
        .next()
        .map(|ns| {
            ns.trim()
                .parse::<i64>()
                .map_err(|_| Error::CompletionIdFormatParseFailure(ns.to_owned()))
        })
        .ok_or_else(|| Error::CompletionIdFormatMissingDot(s.to_owned()))
        // flatten from Result<Result<T>> to Result<T>
        .and_then(convert::identity)
}
