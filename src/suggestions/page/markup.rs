const MAX_MARKUP_STARTS: usize = 4096;
const MAX_TAG_ATTRIBUTES: usize = 128;
const MAX_FORMATTING_STARTS: usize = 512;
const MAX_FORMATTING_WORK: usize = 2 * 1024 * 1024;

// Keep in sync with html5ever's InBody rules that call create_formatting_element_for.
const FORMATTING: [&[u8]; 14] = [
    b"a", b"b", b"big", b"code", b"em", b"font", b"i", b"nobr", b"s", b"small", b"strike",
    b"strong", b"tt", b"u",
];

pub(super) fn check(html: &str) -> Result<(), ()> {
    // Check every plausible tag start, even inside comments or raw text. That
    // avoids having to reproduce the tree builder's tokenizer mode changes.
    // Charge overlapping lookahead so repeated starts cannot make this pass
    // quadratic. Some unusually complex input is intentionally rejected.
    let mut remaining = 4 * html.len();
    let mut formatting = FormattingBudget::default();
    for (count, (offset, _)) in html.match_indices('<').enumerate() {
        if count == MAX_MARKUP_STARTS {
            return Err(());
        }
        let rest = &html.as_bytes()[offset + 1..];
        let tag = rest.strip_prefix(b"/").unwrap_or(rest);
        if tag.first().is_some_and(u8::is_ascii_alphabetic) {
            let scan = scan_tag(tag, remaining)?;
            remaining -= scan.bytes;
            if rest.len() == tag.len() {
                formatting.check(scan)?;
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct FormattingBudget {
    starts: usize,
    work: usize,
    by_name: [(usize, usize); FORMATTING.len()],
}

impl FormattingBudget {
    fn check(&mut self, tag: TagScan<'_>) -> Result<(), ()> {
        let Some(index) = FORMATTING
            .iter()
            .position(|name| tag.name.eq_ignore_ascii_case(name))
        else {
            return Ok(());
        };
        self.starts += 1;
        if self.starts > MAX_FORMATTING_STARTS {
            return Err(());
        }
        let (count, prior_weight) = &mut self.by_name[index];
        // The parser can compare a new entry to any earlier entry. Different
        // names exit before cloning; matching names can clone and sort both
        // attribute lists. Charge both sides, weighted by input bytes and sort
        // depth. Ignore closing tags here rather than simulating list eviction.
        let weight = if tag.attributes == 0 {
            0
        } else {
            tag.bytes * (2 + tag.attributes.ilog2() as usize)
        };
        let work = count
            .checked_mul(weight)
            .and_then(|current| current.checked_add(*prior_weight))
            .ok_or(())?;
        if work > MAX_FORMATTING_WORK - self.work {
            return Err(());
        }
        self.work += work;
        *count += 1;
        *prior_weight += weight;
        Ok(())
    }
}

struct TagScan<'a> {
    name: &'a [u8],
    bytes: usize,
    attributes: usize,
}

fn scan_tag(tag: &[u8], budget: usize) -> Result<TagScan<'_>, ()> {
    let window = &tag[..tag.len().min(budget)];
    let mut rest = skip_while(window, |byte| {
        !byte.is_ascii_whitespace() && !matches!(byte, b'/' | b'>')
    });
    let name = &window[..window.len() - rest.len()];
    let mut attributes = 0;
    let bytes = loop {
        rest = skip_while(rest, |byte| byte.is_ascii_whitespace() || byte == b'/');
        match rest.first() {
            Some(b'>') => break window.len() - rest.len() + 1,
            None if window.len() == tag.len() => break window.len(),
            None => return Err(()),
            Some(_) => {}
        }

        attributes += 1;
        if attributes > MAX_TAG_ATTRIBUTES {
            return Err(());
        }
        // Even '=' can start an attribute name after parser error recovery.
        rest = skip_while(&rest[1..], |byte| {
            !byte.is_ascii_whitespace() && !matches!(byte, b'/' | b'>' | b'=')
        })
        .trim_ascii_start();
        if let Some(value) = rest.strip_prefix(b"=") {
            rest = after_attribute_value(value.trim_ascii_start());
        }
    };
    Ok(TagScan {
        name,
        bytes,
        attributes,
    })
}

fn after_attribute_value(value: &[u8]) -> &[u8] {
    if let [quote @ (b'\'' | b'"'), text @ ..] = value {
        let rest = skip_while(text, |byte| byte != *quote);
        return rest.get(1..).unwrap_or_default();
    }
    skip_while(value, |byte| !byte.is_ascii_whitespace() && byte != b'>')
}

fn skip_while(input: &[u8], keep_skipping: impl Fn(u8) -> bool) -> &[u8] {
    let end = input
        .iter()
        .position(|&byte| !keep_skipping(byte))
        .unwrap_or(input.len());
    &input[end..]
}
