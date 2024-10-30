use std::collections::HashMap;

use axum::http::Uri;
use minijinja::ErrorKind;
use pulldown_cmark::{BrokenLink, BrokenLinkCallback};

use crate::APP_PREFIX;

pub fn app_url(value: &str) -> String {
    [APP_PREFIX, value.trim_start_matches('/')].join("")
}

pub fn append_query_param(
    uristr: String,
    key: String,
    value: String,
) -> Result<String, minijinja::Error> {
    let uri = uristr.parse::<Uri>().map_err(|e| {
        minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to parse uri string")
            .with_source(e)
    })?;
    let mut query: HashMap<_, _> = match uri.query() {
        Some(q) => serde_urlencoded::from_str(q).map_err(|e| {
            minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to decode query params")
                .with_source(e)
        })?,
        None => HashMap::new(),
    };
    query.insert(key.as_str(), value.as_str());
    let querystring = serde_urlencoded::to_string(query).map_err(|e| {
        minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to encode query params")
            .with_source(e)
    })?;

    Ok(format!("?{querystring}"))
}

pub fn truncate_text(mut s: String, chars: Option<usize>) -> String {
    let chars = chars.unwrap_or(100);
    if s.len() > chars {
        s.truncate(chars);
        s + "..."
    } else {
        s
    }
}

pub fn format_id_number(id: i64, prefix: Option<&str>, width: Option<usize>) -> String {
    let width = width.unwrap_or(4);
    let prefix = prefix.unwrap_or("");
    format!("{}{:0>width$}", prefix, id, width = width)
}

pub fn format_quantity(qty: f64) -> String {
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

pub fn markdown(value: Option<&str>) -> minijinja::Value {
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
