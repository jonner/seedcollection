//! Functions related to prompting the user for input data
use inquire::{CustomUserError, autocompletion::Autocomplete, validator::Validation};
use libseed::{
    core::{
        database::Database,
        loadable::Loadable,
        query::filter::{Cmp, and},
    },
    source::{self, Source},
    taxonomy::{Taxon, quickfind},
};
use std::convert;

/// Errors that may occur when prompting a user for input
#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Internal Error: completion format was incorrect for '{0}'")]
    CompletionIdFormatMissingDot(String),
    #[error("Internal Error: unable to parse an integer from '{0}'")]
    CompletionIdFormatParseFailure(String),
    #[error(transparent)]
    Prompt(#[from] inquire::InquireError),
}

/// An object representing a prompt for a [Taxon] id
pub(crate) struct TaxonIdPrompt<'a> {
    text: inquire::Text<'a>,
}

impl<'a> TaxonIdPrompt<'a> {
    /// Create a new [TaxonIdPrompt] object
    pub(crate) fn new(message: &'a str, db: &Database) -> Self {
        Self {
            text: inquire::Text::new(message).with_autocomplete(TaxonCompleter { db: db.clone() }),
        }
    }

    /// Prompt the user for input and return a result
    pub(crate) fn prompt(self) -> Result<i64, Error> {
        let res = self.text.prompt()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Genus Species
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        extract_dbid(&res)
    }

    /// Prompt the user for input and return a result, but allow the user to
    /// press `<esc>` to skip giving input
    pub(crate) fn prompt_skippable(self) -> Option<i64> {
        let res = self.text.prompt_skippable().ok()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Genus Species
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        res.and_then(|val| extract_dbid(&val).ok())
    }
}

/// An object that assists in providing completion options when the user starts
/// typing part of a [Taxon] name.
#[derive(Clone)]
struct TaxonCompleter {
    db: Database,
}

impl Autocomplete for TaxonCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut taxa = Ok(vec![]);
        if input.len() > 2 {
            taxa = futures::executor::block_on(Taxon::load_all(
                quickfind(input),
                None,
                None,
                &self.db,
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
        Ok(highlighted_suggestion)
    }
}

/// An object representing a prompt for a [Source] id
pub(crate) struct SourceIdPrompt<'a> {
    text: inquire::Text<'a>,
}

impl<'a> SourceIdPrompt<'a> {
    /// Create a new [SourceIdPrompt] object
    pub(crate) fn new(message: &'a str, userid: i64, db: &Database) -> Self {
        Self {
            text: inquire::Text::new(message).with_autocomplete(SourceCompleter {
                db: db.clone(),
                userid,
            }),
        }
    }

    /// Prompt the user for input and return the result
    pub(crate) fn prompt(self) -> Result<i64, Error> {
        let res = self.text.prompt()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Source name
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        extract_dbid(&res)
    }

    /// Prompt the user for input and return the result, but allow the user to
    /// press `<esc>` to skip giving a response
    pub(crate) fn prompt_skippable(self) -> Option<i64> {
        let res = self.text.prompt_skippable().ok()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Source name
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        res.and_then(|val| extract_dbid(&val).ok())
    }
}

/// An object that assists in providing completion options when the user starts
/// typing part of a [Source] name.
#[derive(Clone)]
struct SourceCompleter {
    db: Database,
    userid: i64,
}

impl Autocomplete for SourceCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let fbuilder = and()
            .push(source::Filter::UserId(self.userid))
            .push(source::Filter::Name(Cmp::Like, input.to_string()));
        let mut sources = Ok(vec![]);
        if input.len() > 2 {
            sources = futures::executor::block_on(Source::load_all(
                Some(fbuilder.build()),
                None,
                None,
                &self.db,
            ));
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
        Ok(highlighted_suggestion)
    }
}

#[doc(hidden)]
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

pub(crate) fn prompt_source(userid: i64) -> Result<Source, Error> {
    let name = inquire::Text::new("Source Name:").prompt()?;
    let description = inquire::Text::new("Source Description:").prompt_skippable()?;
    let latitude = inquire::CustomType::<f64>::new("Latitude:")
        .with_validator(|val: &f64| {
            if *val < -90.0 || *val > 90.0 {
                return Ok(Validation::Invalid(
                    "Value must be between -90 and 90".into(),
                ));
            }
            Ok(Validation::Valid)
        })
        .prompt_skippable()?;
    let longitude = inquire::CustomType::<f64>::new("Longitude:")
        .with_validator(|val: &f64| {
            if *val < -180.0 || *val > 180.0 {
                return Ok(Validation::Invalid(
                    "Value must be betwen -180 and 180".into(),
                ));
            }
            Ok(Validation::Valid)
        })
        .prompt_skippable()?;
    if !inquire::Confirm::new("Save to database?")
        .with_default(false)
        .prompt()?
    {
        return Err(inquire::InquireError::OperationCanceled.into());
    }
    Ok(Source::new(name, description, latitude, longitude, userid))
}
