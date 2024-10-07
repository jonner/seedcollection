use pulldown_cmark::Event;
use regex::Regex;

use crate::app_url;

pub fn markdown(value: Option<&str>) -> minijinja::Value {
    let value = value.unwrap_or("");
    let re = Regex::new(r"(?<tag>[SLP])(?<id>[0-9]{4,})").expect("Unable to create regex");
    let parser = pulldown_cmark::Parser::new(value).flat_map(|event| {
        match event {
            Event::Text(ref txt) => (|| {
                let mut events: Vec<Event> = Vec::new();
                let captures = re.captures_iter(txt);
                let mut prev: Option<regex::Captures> = None;
                for capture in captures {
                    let start = match prev {
                        None => 0,
                        Some(prevcapture) => prevcapture.get(0)?.end(),
                    };
                    // if there was a plain-text segment between the end
                    // of the last match and the start of this match, emit that first
                    let overall = capture.get(0)?;
                    if start < overall.start() {
                        let substr = &txt[start..overall.start()];
                        events.push(Event::Text(substr.to_string().into()));
                    }

                    // now output a link to the object
                    let url = (|| {
                        let tag = capture.name("tag")?.as_str();
                        let slug = match tag {
                            "S" => Some("sample"),
                            "L" => Some("source"),
                            "P" => Some("project"),
                            _ => None,
                        }?;
                        let id: i64 = capture.name("id")?.as_str().parse().ok()?;
                        Some(app_url(&format!("/{slug}/{id}")))
                    })()?;

                    events.push(Event::Start(pulldown_cmark::Tag::Link {
                        link_type: pulldown_cmark::LinkType::Inline,
                        dest_url: url.into(),
                        id: "".into(),
                        title: "".into(),
                    }));
                    events.push(Event::Text(overall.as_str().to_string().into()));
                    events.push(Event::End(pulldown_cmark::TagEnd::Link));
                    prev = Some(capture);
                }
                let end = match prev {
                    Some(capture) => capture.get(0)?.end(),
                    // we never captured anything, so the whole string is just text
                    None => 0,
                };
                if end < txt.len() {
                    // one last text event
                    let substr = &txt[end..];
                    events.push(Event::Text(substr.to_string().into()))
                }
                Some(events)
            })(),
            _ => None,
        }
        .unwrap_or(vec![event])
        .into_iter()
    });
    let mut output = String::new();
    pulldown_cmark::html::push_html(&mut output, parser);
    minijinja::Value::from_safe_string(output)
}

#[test]
fn test_markdown_ids() {
    assert_eq!(
        markdown(Some("S0006")).as_str(),
        Some(format!("<p><a href=\"{}\">S0006</a></p>\n", app_url("/sample/6")).as_str())
    );
    assert_eq!(
        markdown(Some("L0006 and S0123 and P9999")).as_str(),
        Some(
            format!(
                "<p><a href=\"{}\">L0006</a> and <a href=\"{}\">S0123</a> and <a href=\"{}\">P9999</a></p>\n",
                app_url("/source/6"),
                app_url("/sample/123"),
                app_url("/project/9999"),
            )
            .as_str()
        )
    );
    assert_eq!(
        markdown(Some("L0006")).as_str(),
        Some(format!("<p><a href=\"{}\">L0006</a></p>\n", app_url("/source/6")).as_str())
    );
    assert_eq!(
        markdown(Some("P0006")).as_str(),
        Some(format!("<p><a href=\"{}\">P0006</a></p>\n", app_url("/project/6")).as_str())
    );
    assert_eq!(markdown(Some("X0006")).as_str(), Some("<p>X0006</p>\n"));
    assert_eq!(markdown(Some("S006")).as_str(), Some("<p>S006</p>\n"));
    assert_eq!(
        markdown(Some("This is just some text")).as_str(),
        Some("<p>This is just some text</p>\n")
    );
}
