use crate::{APP_PREFIX, Error};
use axum::http::Uri;
use libseed::{
    Database,
    core::{loadable::Loadable, query::LimitSpec},
    project::Project,
    sample::Sample,
    source::Source,
    user::User,
};
use minijinja::ErrorKind;
use pulldown_cmark::{BrokenLink, BrokenLinkCallback};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, num::NonZero};

pub(crate) fn app_url(value: &str) -> String {
    [APP_PREFIX, value.trim_start_matches('/')].join("")
}

pub const PAGE_SIZE: NonZero<u32> = NonZero::new(50).unwrap();

/// A structure that can be used to presents a summary of results in a web page
#[derive(Debug, Serialize, Deserialize)]
pub struct Paginator {
    total_items: u32,
    npages: u32,
    pagesize: NonZero<u32>,
    page: NonZero<u32>,
}

impl Paginator {
    pub fn new(total_items: u32, pagesize: Option<NonZero<u32>>, page: Option<u32>) -> Self {
        let pagesize = pagesize.unwrap_or(PAGE_SIZE);
        let npages = total_items.div_ceil(pagesize.get());
        Self {
            total_items,
            npages,
            pagesize,
            page: page
                .and_then(|p| NonZero::new(p.min(npages)))
                .unwrap_or(unsafe { NonZero::new_unchecked(1) }),
        }
    }

    pub fn limits(&self) -> LimitSpec {
        LimitSpec {
            count: self.pagesize.get() as i32,
            offset: Some(((self.page.get() - 1) * self.pagesize.get()) as i32),
        }
    }
}

#[cfg(test)]
mod test_paginator {
    use crate::util::Paginator;
    use libseed::core::query::LimitSpec;

    #[test]
    fn test_paginator() {
        let p = Paginator::new(100, Some(20.try_into().unwrap()), None);
        assert_eq!(p.npages, 5);
        assert_eq!(p.page.get(), 1);
        assert_eq!(
            p.limits(),
            LimitSpec {
                count: 20,
                offset: Some(0)
            }
        );

        // page 0 gets clamped to 1
        let p = Paginator::new(100, Some(20.try_into().unwrap()), Some(0));
        assert_eq!(p.npages, 5);
        assert_eq!(p.page.get(), 1);
        assert_eq!(
            p.limits(),
            LimitSpec {
                count: 20,
                offset: Some(0)
            }
        );

        let p = Paginator::new(101, Some(20.try_into().unwrap()), Some(5));
        assert_eq!(p.npages, 6);
        assert_eq!(p.page.get(), 5);
        assert_eq!(
            p.limits(),
            LimitSpec {
                count: 20,
                offset: Some(80)
            }
        );

        // specifying a page beyond the max will clamp to the max
        let p = Paginator::new(101, Some(20.try_into().unwrap()), Some(7));
        assert_eq!(p.npages, 6);
        assert_eq!(p.page.get(), 6);
        assert_eq!(
            p.limits(),
            LimitSpec {
                count: 20,
                offset: Some(100)
            }
        );
    }
}

/// A minijinja template filter for appending (or replacing) a given query param
/// to a url.
pub(crate) fn append_query_param(
    uristr: &str,
    key: &str,
    value: &str,
) -> Result<String, minijinja::Error> {
    let uri = uristr.parse::<Uri>().map_err(|e| {
        minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to parse uri string")
            .with_source(e)
    })?;
    let mut query: BTreeMap<_, _> = match uri.query() {
        Some(q) => serde_urlencoded::from_str(q).map_err(|e| {
            minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to decode query params")
                .with_source(e)
        })?,
        None => BTreeMap::new(),
    };
    query.insert(key, value);
    let querystring = serde_urlencoded::to_string(query).map_err(|e| {
        minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to encode query params")
            .with_source(e)
    })?;

    Ok(format!("{path}?{querystring}", path = uri.path()))
}

#[test]
fn test_append_query_param() {
    let uri = "http://foo.bar/path/file";
    let expected = "/path/file?key=value";
    assert_eq!(
        append_query_param(uri, "key", "value").expect("Failed to append"),
        expected
    );
    let uri = "http://foo.bar/path/file?key=value1";
    let expected = "/path/file?key=value2";
    assert_eq!(
        append_query_param(uri, "key", "value2").expect("Failed to append"),
        expected
    );
    let uri = "http://foo.bar/path/file?key1=value1";
    let expected = "/path/file?key1=value1&key2=value2";
    assert_eq!(
        append_query_param(uri, "key2", "value2").expect("Failed to append"),
        expected
    );
    let uri = "http://foo.bar/path/file?key1=value1&key2=value2";
    let expected = "/path/file?key1=value1&key2=newvalue";
    assert_eq!(
        append_query_param(uri, "key2", "newvalue").expect("Failed to append"),
        expected
    );
    let uri = "http://foo.bar/path/file?key1=value1&key2=value2";
    let expected = "/path/file?key1=newvalue&key2=value2";
    assert_eq!(
        append_query_param(uri, "key1", "newvalue").expect("Failed to append"),
        expected
    );
}

/// A minijinja template filter for formatting an object's id number in a
/// consistent manner and with a specific amount of zero-padding. e.g. `S0001`
pub(crate) fn format_id_number(id: i64, prefix: Option<&str>, width: Option<usize>) -> String {
    let width = width.unwrap_or(4);
    let prefix = prefix.unwrap_or("");
    format!("{}{:0>width$}", prefix, id, width = width)
}

/// A minijinja template filter for formatting a seed quantity in grams into a
/// standard format and calculating the imperial equivalent.
pub(crate) fn format_quantity(qty: f64) -> String {
    let mut metric_qty = qty;
    let mut metric_label = "grams";
    let imperial_qty = metric_qty * 0.03527396195;

    if metric_qty > 1000.0 {
        metric_label = "kilograms";
        metric_qty /= 1000.0;
    }
    let metric = format!("{metric_qty:.2} {metric_label}");

    let imperial = if imperial_qty > 16.0 {
        let lbs = (imperial_qty / 16.0).floor() as i64;
        let oz = imperial_qty % 16.0;
        format!("{lbs} lbs {oz:.2} ounces")
    } else {
        format!("{imperial_qty:.2} ounces")
    };

    format!("{metric} ({imperial})")
}

/// An object for resolving links to various objects in the collection database
/// from within a markdown comment. For example, the text `[S0024]` should be
/// transformed into a link to the details page for the sample object with id of 24.
/// - `[Sxxxx]` -> Samples
/// - `[Lxxxx]` -> Sources
/// - `[Pxxxx]` -> Projects
struct ObjectLinkResolver;

impl<'input> BrokenLinkCallback<'input> for ObjectLinkResolver {
    fn handle_broken_link(
        &mut self,
        link: BrokenLink<'input>,
    ) -> Option<(
        pulldown_cmark::CowStr<'input>,
        pulldown_cmark::CowStr<'input>,
    )> {
        link.reference
            .chars()
            .nth(0)
            .and_then(|ch| match ch {
                'S' => Some("sample"),
                'L' => Some("source"),
                'P' => Some("project"),
                _ => None,
            })
            .and_then(|slug| {
                link.reference
                    .get(1..)
                    .and_then(|s| s.parse::<i64>().ok())
                    .map(|id| {
                        (
                            app_url(&format!("/{slug}/{id}")).into(),
                            format!("{slug} #{id}").into(),
                        )
                    })
            })
    }
}

/// A minijinja template filter to parse and format markdown so that templates
/// can process user-generated markdown for comments, etc.
pub(crate) fn markdown(value: Option<&str>) -> minijinja::Value {
    let value = value.unwrap_or("");
    let parser = pulldown_cmark::Parser::new_with_broken_link_callback(
        value,
        pulldown_cmark::Options::empty(),
        Some(ObjectLinkResolver),
    );
    let mut output = String::new();
    pulldown_cmark::html::push_html(&mut output, parser);
    minijinja::Value::from_safe_string(output)
}

#[test]
fn test_markdown_ids() {
    assert_eq!(
        markdown(Some("[S0006]")).as_str(),
        Some(
            format!(
                "<p><a href=\"{}\" title=\"sample #6\">S0006</a></p>\n",
                app_url("/sample/6")
            )
            .as_str()
        )
    );
    assert_eq!(markdown(Some("S0006")).as_str(), Some("<p>S0006</p>\n"));
    assert_eq!(
        markdown(Some("[L0006] and [S0123] and [P9999]")).as_str(),
        Some(
            format!(
                "<p><a href=\"{}\" title=\"source #6\">L0006</a> and <a href=\"{}\" title=\"sample #123\">S0123</a> and <a href=\"{}\" title=\"project #9999\">P9999</a></p>\n",
                app_url("/source/6"),
                app_url("/sample/123"),
                app_url("/project/9999"),
            )
            .as_str()
        )
    );
    assert_eq!(
        markdown(Some("L0006 and [S0123] and [B9999]")).as_str(),
        Some(
            format!(
                "<p>L0006 and <a href=\"{}\" title=\"sample #123\">S0123</a> and [B9999]</p>\n",
                app_url("/sample/123"),
            )
            .as_str()
        )
    );
    assert_eq!(
        markdown(Some("[L0006]")).as_str(),
        Some(
            format!(
                "<p><a href=\"{}\" title=\"source #6\">L0006</a></p>\n",
                app_url("/source/6")
            )
            .as_str()
        )
    );
    assert_eq!(
        markdown(Some("[P0006]")).as_str(),
        Some(
            format!(
                "<p><a href=\"{}\" title=\"project #6\">P0006</a></p>\n",
                app_url("/project/6")
            )
            .as_str()
        )
    );
    assert_eq!(markdown(Some("[X0006]")).as_str(), Some("<p>[X0006]</p>\n"));
    assert_eq!(
        markdown(Some("[S006]")).as_str(),
        Some(
            format!(
                "<p><a href=\"{}\" title=\"sample #6\">S006</a></p>\n",
                app_url("/sample/6")
            )
            .as_str()
        )
    );
    assert_eq!(
        markdown(Some("This is just some text")).as_str(),
        Some("<p>This is just some text</p>\n")
    );
}

#[derive(Serialize)]
pub(crate) enum FlashMessageKind {
    Success,
    Warning,
    Info,
    Error,
}

#[derive(Serialize)]
pub(crate) struct FlashMessage {
    pub kind: FlashMessageKind,
    pub msg: String,
}

pub trait AccessControlled: Loadable {
    fn load_for_user(
        id: <Self as Loadable>::Id,
        user: &User,
        db: &Database,
    ) -> impl Future<Output = Result<Self, Error>>
    where
        Self: Sized;
}

impl AccessControlled for Sample {
    async fn load_for_user(
        id: <Self as Loadable>::Id,
        user: &User,
        db: &Database,
    ) -> Result<Self, Error> {
        let sample = Self::load(id, db).await.map_err(|e| match e {
            libseed::Error::DatabaseError(sqlx::Error::RowNotFound) => {
                Error::NotFound(format!("Unable to find sample '{id}'"))
            }
            _ => e.into(),
        })?;
        if sample.user.id() != user.id {
            return Err(Error::Unauthorized(format!(
                "User does not have permission to access sample '{id}'"
            )));
        };
        Ok(sample)
    }
}

impl AccessControlled for Project {
    async fn load_for_user(
        id: <Self as Loadable>::Id,
        user: &User,
        db: &Database,
    ) -> Result<Self, Error> {
        let obj = Self::load(id, db).await.map_err(|e| match e {
            libseed::Error::DatabaseError(sqlx::Error::RowNotFound) => {
                Error::NotFound(format!("Unable to find sample '{id}'"))
            }
            _ => e.into(),
        })?;
        if obj.userid != user.id {
            return Err(Error::Unauthorized(format!(
                "User does not have permission to access project '{id}'"
            )));
        };
        Ok(obj)
    }
}

impl AccessControlled for Source {
    async fn load_for_user(
        id: <Self as Loadable>::Id,
        user: &User,
        db: &Database,
    ) -> Result<Self, Error> {
        let obj = Self::load(id, db).await.map_err(|e| match e {
            libseed::Error::DatabaseError(sqlx::Error::RowNotFound) => {
                Error::NotFound(format!("Unable to find source '{id}'"))
            }
            _ => e.into(),
        })?;
        if obj.userid != user.id {
            return Err(Error::Unauthorized(format!(
                "User does not have permission to access source '{id}'"
            )));
        };
        Ok(obj)
    }
}
