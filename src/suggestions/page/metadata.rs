use std::{cell::RefCell, time::Instant};

use html5ever::tokenizer::{
    BufferQueue, EndTag, StartTag, Tag, Token, TokenSink, TokenSinkResult, Tokenizer,
    states::RawKind::{Rawtext, Rcdata, ScriptData},
};

use super::{MAX_TITLE, Page, Text, markup};
use crate::error::PageReadError;

const PREFIX_BYTES: usize = 16 * 1024;
const MAX_DESCRIPTION: usize = 5000;
const MAX_MARKUP_STARTS: usize = 128;

pub(super) fn recover(html: &str, deadline: Instant) -> Result<Option<Page>, PageReadError> {
    let prefix = &html[..html.floor_char_boundary(PREFIX_BYTES)];
    let prefix = match markup::check(prefix) {
        Ok(()) => prefix,
        Err(stop) => &prefix[..stop],
    };
    let tokenizer = Tokenizer::new(Sink::default(), Default::default());
    let input = BufferQueue::default();
    let mut start = 0;
    // Feed through the tokenizer, not a split-on-tags parser: quoted
    // attributes and raw text can span input chunks.
    for end in prefix
        .match_indices('<')
        .map(|(offset, _)| offset)
        .filter(|&offset| offset != 0)
        .chain(std::iter::once(prefix.len()))
        .take(MAX_MARKUP_STARTS)
    {
        if Instant::now() >= deadline {
            return Err(PageReadError::Timeout);
        }
        input.push_back(prefix[start..end].into());
        let _ = tokenizer.feed(&input);
        let head = tokenizer.sink.0.borrow();
        if head.done || (head.title.is_some() && head.description.is_some()) {
            break;
        }
        start = end;
    }
    if Instant::now() >= deadline {
        return Err(PageReadError::Timeout);
    }
    // Do not send EOF. Only a real closing title token commits the title,
    // and a partial meta tag never reaches the sink.
    let Head {
        title, description, ..
    } = tokenizer.sink.0.into_inner();
    Ok((title.is_some() || description.is_some()).then_some(Page::Metadata { title, description }))
}

#[derive(Default)]
struct Head {
    title: Option<String>,
    description: Option<String>,
    pending_title: Option<Text>,
    raw: bool,
    templates: usize,
    done: bool,
}

#[derive(Default)]
struct Sink(RefCell<Head>);

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut head = self.0.borrow_mut();
        if head.done {
            return TokenSinkResult::Continue;
        }
        match token {
            Token::TagToken(tag) => return head.tag(tag),
            Token::CharacterTokens(text) if head.raw => {
                if let Some(title) = &mut head.pending_title
                    && !title.append(&text)
                {
                    head.pending_title = None;
                }
            }
            Token::CharacterTokens(text) if head.templates == 0 => {
                // Only HTML space characters leave the insertion mode in head.
                head.done = !text.bytes().all(|b| b.is_ascii_whitespace());
            }
            Token::NullCharacterToken if !head.raw && head.templates == 0 => head.done = true,
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

impl Head {
    fn tag(&mut self, tag: Tag) -> TokenSinkResult<()> {
        use TokenSinkResult::{Continue, RawData};

        let name = &*tag.name;
        if self.raw {
            // In RCDATA, RAWTEXT and script data, only the appropriate end tag
            // can be emitted. A literal tag in the text cannot change scope.
            if tag.kind == EndTag {
                if let Some(title) = self.pending_title.take().filter(|t| !t.value.is_empty()) {
                    self.title = Some(title.value);
                }
                self.raw = false;
            }
            return Continue;
        }
        match (tag.kind, name) {
            (StartTag, "template") => self.templates += 1,
            (EndTag, "template") if self.templates != 0 => self.templates -= 1,
            (StartTag, "title" | "style" | "script" | "noscript" | "noframes") => {
                self.raw = true;
                return RawData(match name {
                    "title" => {
                        if self.templates == 0 && self.title.is_none() {
                            self.pending_title = Some(Text::new(MAX_TITLE));
                        }
                        Rcdata
                    }
                    "script" => ScriptData,
                    _ => Rawtext,
                });
            }
            (StartTag, "meta") if self.templates == 0 && self.description.is_none() => {
                let attr = |name: &str| tag.attrs.iter().find(|a| &*a.name.local == name);
                if attr("name").is_some_and(|a| a.value.eq_ignore_ascii_case("description"))
                    && let Some(content) = attr("content")
                {
                    let mut text = Text::new(MAX_DESCRIPTION);
                    if text.append(&content.value) && !text.value.is_empty() {
                        self.description = Some(text.value);
                    }
                }
            }
            (StartTag, "html" | "head" | "base" | "basefont" | "bgsound" | "link" | "meta") => {}
            // Within an inert template we can skip ordinary containers. Stop
            // rather than guess if foreign content, special insertion modes,
            // or other unexpected markup would require a tree builder.
            (_, "div" | "span" | "p" | "section" | "article") if self.templates != 0 => {}
            _ => self.done = true,
        }
        Continue
    }
}
