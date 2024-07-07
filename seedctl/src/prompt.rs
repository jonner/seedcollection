use anyhow::{anyhow, Result};
use inquire::{autocompletion::Autocomplete, validator::Validation, CustomUserError};
use libseed::{
    filter::{Cmp, FilterBuilder, FilterOp},
    source::{self, Source},
    taxonomy::{quickfind, Taxon},
};
use sqlx::{Pool, Sqlite};
use std::sync::Arc;

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

    pub fn prompt(self) -> Result<i64> {
        let res = self.text.prompt()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Genus Species
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        extract_dbid(&res)
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
            taxa = futures::executor::block_on(Taxon::fetch_all(
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

    pub fn prompt(self) -> Result<i64> {
        let res = self.text.prompt()?;
        // HACK -- the completer generates a string with the following format:
        // $DBID. Source name
        // Since we want to be able to choose species by name, but we want to
        // end up with the database id, just parse the database id from the string.
        extract_dbid(&res)
    }
}

#[derive(Clone)]
struct SourceCompleter {
    dbpool: Pool<Sqlite>,
    userid: i64,
}

impl Autocomplete for SourceCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut fbuilder = FilterBuilder::new(FilterOp::And);
        fbuilder = fbuilder.push(Arc::new(source::Filter::UserId(self.userid)));
        fbuilder = fbuilder.push(Arc::new(source::Filter::Name(Cmp::Like, input.to_string())));
        let mut sources = Ok(vec![]);
        if input.len() > 2 {
            sources = futures::executor::block_on(Source::fetch_all(
                Some(fbuilder.build()),
                &self.dbpool,
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
        {
            Ok(highlighted_suggestion)
        }
    }
}

// This is a hack that relies on the conventions above for autocompleting items from the database.
// The autocompletion suggestions have the form "$DBID. $DESCRIPTION"
// This code simply splits the string at the first '.' character and returns the ID before that.
fn extract_dbid(s: &str) -> Result<i64> {
    s.split('.')
        .next()
        .map(|s| s.trim().parse::<i64>())
        .ok_or_else(|| anyhow!("Internal Error: Couldn't extract database ID"))?
        .map_err(|e| e.into())
}

pub fn u32_prompt(message: &str, min: u32, max: u32) -> Result<Option<u32>> {
    inquire::Text::new(message)
        .with_validator(move |input: &str| match input.parse::<u32>() {
            Ok(n) if n >= min && n <= max => Ok(Validation::Valid),
            Ok(_) => Ok(Validation::Invalid("Invalid value for integer".into())),
            Err(_) => Ok(Validation::Invalid("Input should be an integer".into())),
        })
        .prompt_skippable()
        .map(|o| o.map(|s| s.parse::<u32>().unwrap()))
        .map_err(|e| e.into())
}

pub fn i64_prompt(message: &str, min: i64, max: i64) -> Result<Option<i64>> {
    inquire::Text::new(message)
        .with_validator(move |input: &str| match input.parse::<i64>() {
            Ok(n) if n >= min && n <= max => Ok(Validation::Valid),
            Ok(_) => Ok(Validation::Invalid("Invalid value for integer".into())),
            Err(_) => Ok(Validation::Invalid("Input should be an integer".into())),
        })
        .prompt_skippable()
        .map(|o| o.map(|s| s.parse::<i64>().unwrap()))
        .map_err(|e| e.into())
}

pub fn f64_prompt(message: &str, min: f64, max: f64) -> Result<Option<f64>> {
    inquire::Text::new(message)
        .with_validator(move |input: &str| match input.parse::<f64>() {
            Ok(n) if n >= min && n <= max => Ok(Validation::Valid),
            Ok(_) => Ok(Validation::Invalid("Invalid value for number".into())),
            Err(_) => Ok(Validation::Invalid("Input should be an number".into())),
        })
        .prompt_skippable()
        .map(|o| o.map(|s| s.parse::<f64>().unwrap()))
        .map_err(|e| e.into())
}
