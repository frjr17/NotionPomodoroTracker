//! Markdown ↔ Notion block conversion for the task page body. Pure (no HTTP)
//! so it can be tested against canned block JSON.
//!
//! ponytail: bounded block subset — paragraph, heading 1-3, bulleted/numbered
//! list, to-do, quote, code, divider, plus inline bold/italic/strike/code/link.
//! Anything else degrades to plain text; nested block trees are flattened. Good
//! enough for task notes; a full Notion block tree would need recursion.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Notion blocks -> Markdown
// ---------------------------------------------------------------------------

/// Render a page's block children (the `results` array from
/// `GET /blocks/{id}/children`) as Markdown.
pub fn blocks_to_markdown(blocks: &[Value]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut prev_list = false;
    for block in blocks {
        let ty = block.get("type").and_then(Value::as_str).unwrap_or("");
        let body = block.get(ty).unwrap_or(&Value::Null);
        let rich = |b: &Value| rich_text_to_md(b.get("rich_text").unwrap_or(&Value::Null));

        let (line, is_list) = match ty {
            "paragraph" => (rich(body), false),
            "heading_1" => (format!("# {}", rich(body)), false),
            "heading_2" => (format!("## {}", rich(body)), false),
            "heading_3" => (format!("### {}", rich(body)), false),
            "bulleted_list_item" => (format!("- {}", rich(body)), true),
            "numbered_list_item" => (format!("1. {}", rich(body)), true),
            "to_do" => {
                let checked = body.get("checked").and_then(Value::as_bool) == Some(true);
                let mark = if checked { "x" } else { " " };
                (format!("- [{mark}] {}", rich(body)), true)
            }
            "quote" => (format!("> {}", rich(body)), false),
            "code" => {
                let lang = body.get("language").and_then(Value::as_str).unwrap_or("");
                (format!("```{lang}\n{}\n```", rich(body)), false)
            }
            "divider" => ("---".to_string(), false),
            // Unknown block: keep any text it carries rather than dropping it.
            _ => (rich(body), false),
        };

        // Keep consecutive list items tight; separate other blocks by a blank
        // line.
        if !out.is_empty() {
            out.push(if is_list && prev_list {
                "\n".to_string()
            } else {
                "\n\n".to_string()
            });
        }
        out.push(line);
        prev_list = is_list;
    }
    out.concat()
}

fn rich_text_to_md(rich: &Value) -> String {
    let Some(items) = rich.as_array() else {
        return String::new();
    };
    let mut s = String::new();
    for item in items {
        let text = item
            .get("plain_text")
            .and_then(Value::as_str)
            .or_else(|| {
                item.get("text")
                    .and_then(|t| t.get("content"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("");
        if text.is_empty() {
            continue;
        }
        let ann = item.get("annotations").unwrap_or(&Value::Null);
        let has = |k: &str| ann.get(k).and_then(Value::as_bool) == Some(true);
        let href = item.get("href").and_then(Value::as_str).or_else(|| {
            item.get("text")
                .and_then(|t| t.get("link"))
                .and_then(|l| l.get("url"))
                .and_then(Value::as_str)
        });

        // Code spans can't nest other markdown, so treat them as terminal.
        let mut wrapped = if has("code") {
            format!("`{text}`")
        } else {
            let mut t = text.to_string();
            if has("strikethrough") {
                t = format!("~~{t}~~");
            }
            if has("bold") {
                t = format!("**{t}**");
            }
            if has("italic") {
                t = format!("*{t}*");
            }
            t
        };
        if let Some(url) = href {
            wrapped = format!("[{wrapped}]({url})");
        }
        s.push_str(&wrapped);
    }
    s
}

// ---------------------------------------------------------------------------
// Markdown -> Notion blocks (for PATCH /blocks/{id}/children)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Inline {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
}

/// Build the Notion block objects for a markdown string.
pub fn markdown_to_blocks(md: &str) -> Vec<Value> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let mut blocks: Vec<Value> = Vec::new();
    // Current block being assembled.
    let mut block_type: Option<&'static str> = None;
    let mut spans: Vec<Value> = Vec::new();
    let mut checked = false;
    let mut code_lang = String::new();
    let mut inline = Inline::default();
    // Stack of enclosing list markers: Some = ordered, None = bulleted.
    let mut lists: Vec<Option<u64>> = Vec::new();

    // `checked` / `code_lang` are only read by the block type that sets them
    // (to_do / code), and every such block sets them before its text, so they
    // need no reset between blocks.
    macro_rules! flush {
        () => {
            if let Some(bt) = block_type.take() {
                blocks.push(finish_block(
                    bt,
                    std::mem::take(&mut spans),
                    checked,
                    &code_lang,
                ));
            }
        };
    }

    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::Paragraph) => {
                if block_type.is_none() {
                    block_type = Some("paragraph");
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                flush!();
                block_type = Some(match level {
                    HeadingLevel::H1 => "heading_1",
                    HeadingLevel::H2 => "heading_2",
                    _ => "heading_3",
                });
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush!();
                block_type = Some("quote");
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush!();
                block_type = Some("code");
                code_lang = match kind {
                    CodeBlockKind::Fenced(l) if !l.is_empty() => l.to_string(),
                    _ => "plain text".to_string(),
                };
            }
            Event::Start(Tag::List(start)) => lists.push(start),
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                flush!();
                block_type = Some(match lists.last() {
                    Some(Some(_)) => "numbered_list_item",
                    _ => "bulleted_list_item",
                });
            }
            Event::TaskListMarker(done) => {
                block_type = Some("to_do");
                checked = done;
            }
            Event::Start(Tag::Emphasis) => inline.italic = true,
            Event::End(TagEnd::Emphasis) => inline.italic = false,
            Event::Start(Tag::Strong) => inline.bold = true,
            Event::End(TagEnd::Strong) => inline.bold = false,
            Event::Start(Tag::Strikethrough) => inline.strike = true,
            Event::End(TagEnd::Strikethrough) => inline.strike = false,
            Event::Start(Tag::Link { dest_url, .. }) => inline.link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => inline.link = None,
            Event::Text(t) => {
                if block_type.is_none() {
                    block_type = Some("paragraph");
                }
                spans.push(span(&t, &inline));
            }
            Event::Code(t) => {
                if block_type.is_none() {
                    block_type = Some("paragraph");
                }
                let mut code_inline = Inline {
                    code: true,
                    ..Default::default()
                };
                code_inline.link = inline.link.clone();
                spans.push(span(&t, &code_inline));
            }
            Event::SoftBreak | Event::HardBreak => {
                if !spans.is_empty() {
                    spans.push(span("\n", &Inline::default()));
                }
            }
            Event::Rule => {
                flush!();
                blocks.push(json!({ "type": "divider", "divider": {} }));
            }
            Event::End(TagEnd::Paragraph)
            | Event::End(TagEnd::Heading(_))
            | Event::End(TagEnd::BlockQuote(_))
            | Event::End(TagEnd::CodeBlock)
            | Event::End(TagEnd::Item) => flush!(),
            _ => {}
        }
    }
    flush!();
    blocks
}

fn span(text: &str, inline: &Inline) -> Value {
    let mut annotations = serde_json::Map::new();
    if inline.bold {
        annotations.insert("bold".into(), json!(true));
    }
    if inline.italic {
        annotations.insert("italic".into(), json!(true));
    }
    if inline.strike {
        annotations.insert("strikethrough".into(), json!(true));
    }
    if inline.code {
        annotations.insert("code".into(), json!(true));
    }
    let link = inline.link.as_ref().map(|url| json!({ "url": url }));
    let mut span = json!({
        "type": "text",
        "text": { "content": text, "link": link },
    });
    if !annotations.is_empty() {
        span["annotations"] = Value::Object(annotations);
    }
    span
}

fn finish_block(block_type: &str, rich_text: Vec<Value>, checked: bool, code_lang: &str) -> Value {
    let mut body = match block_type {
        "code" => json!({ "rich_text": rich_text, "language": code_lang }),
        "to_do" => json!({ "rich_text": rich_text, "checked": checked }),
        _ => json!({ "rich_text": rich_text }),
    };
    // Code blocks carry newlines as one span; strip the trailing one markdown adds.
    if block_type == "code"
        && let Some(arr) = body.get_mut("rich_text").and_then(Value::as_array_mut)
        && let Some(last) = arr.last_mut()
        && let Some(content) = last
            .pointer_mut("/text/content")
            .and_then(|c| c.as_str().map(str::to_string))
    {
        last["text"]["content"] = json!(content.trim_end_matches('\n'));
    }
    json!({ "type": block_type, block_type: body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_to_markdown_covers_common_types() {
        let blocks = vec![
            json!({ "type": "heading_1", "heading_1": { "rich_text": [text("Goals")] } }),
            json!({ "type": "paragraph", "paragraph": { "rich_text": [
                text("Ship the "), bold("PWA"), text(" work")
            ] } }),
            json!({ "type": "bulleted_list_item", "bulleted_list_item": { "rich_text": [text("research")] } }),
            json!({ "type": "bulleted_list_item", "bulleted_list_item": { "rich_text": [text("test cache")] } }),
        ];
        let md = blocks_to_markdown(&blocks);
        assert_eq!(
            md,
            "# Goals\n\nShip the **PWA** work\n\n- research\n- test cache"
        );
    }

    #[test]
    fn markdown_round_trips_through_blocks() {
        let md = "# Goals\n\nShip the **PWA** work\n\n- research\n- test cache";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks_to_markdown(&blocks), md);
    }

    #[test]
    fn todo_and_code_survive_round_trip() {
        let md = "- [x] done item\n- [ ] open item";
        let blocks = markdown_to_blocks(md);
        assert_eq!(blocks_to_markdown(&blocks), md);

        let code = "```rust\nlet x = 1;\n```";
        let blocks = markdown_to_blocks(code);
        assert_eq!(blocks[0]["type"], "code");
        assert_eq!(blocks_to_markdown(&blocks), code);
    }

    // Test helpers building minimal Notion rich_text items.
    fn text(s: &str) -> Value {
        json!({ "type": "text", "plain_text": s, "annotations": {}, "href": null })
    }
    fn bold(s: &str) -> Value {
        json!({ "type": "text", "plain_text": s, "annotations": { "bold": true }, "href": null })
    }
}
