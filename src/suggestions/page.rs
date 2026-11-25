mod document;
mod markup;

use std::{
    cell::LazyCell,
    collections::{HashMap, HashSet},
    error::Error,
    net::{IpAddr, SocketAddr},
    sync::{Arc, LazyLock},
    time::Instant,
};

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
use reqwest::{
    Client,
    dns::{Addrs, Name, Resolve, Resolving},
    header::{
        ACCEPT, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_TYPE, LOCATION, TRANSFER_ENCODING,
    },
    redirect::Policy,
};
use scraper::{ElementRef, Html, Selector};
use tokio::sync::Semaphore;
use url::{Host, Url};

use crate::error::PageReadError;

use super::{CONNECT_TIMEOUT, limited_body};

const MAX_REDIRECTS: usize = 4;
const MAX_PAGE_BODY: usize = 512 * 1024;
const HTML_CHARSET_SNIFF_BYTES: usize = 1024;
const MAX_TITLE: usize = 500;
const MAX_EXCERPT: usize = 6000;
const EXCERPT_CUTOFF: &str = "…";

static PAGE_PARSE_SLOTS: Semaphore = Semaphore::const_new(2);

pub(super) struct Page {
    pub title: Option<String>,
    pub excerpt: String,
}

pub(super) fn client() -> reqwest::ClientBuilder {
    Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .dns_resolver(Arc::new(PublicDns))
}

struct PublicDns;

impl Resolve for PublicDns {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses: Vec<SocketAddr> =
                tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            // These exact addresses go to the connector; no second DNS lookup.
            if addresses.is_empty() || addresses.iter().any(|addr| !is_public(addr.ip())) {
                return Err(PageReadError::RestrictedDestination.into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => match ip.octets() {
            [0 | 10 | 127 | 224..=255, ..]
            | [100, 64..=127, ..]
            | [169, 254, ..]
            | [172, 16..=31, ..]
            | [192, 0, 0 | 2, _]
            | [192, 88, 99, _]
            | [192, 168, ..]
            | [198, 18 | 19, ..]
            | [198, 51, 100, _]
            | [203, 0, 113, _] => false,
            _ => true,
        },
        IpAddr::V6(ip) => match ip.segments() {
            // Accept global unicast only, excluding special-purpose, translation,
            // tunneling, and documentation ranges within it.
            [0x2001, 0..=0x1ff | 0xdb8, ..] | [0x2002, ..] | [0x3fff, 0..=0xfff, ..] => false,
            [0x2000..=0x3fff, ..] => true,
            _ => false,
        },
    }
}

fn validate_destination(url: &Url) -> Result<(), PageReadError> {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(PageReadError::InvalidUrl);
    }
    match url.host() {
        Some(Host::Domain(_)) => Ok(()),
        Some(Host::Ipv4(ip)) if is_public(ip.into()) => Ok(()),
        Some(Host::Ipv6(ip)) if is_public(ip.into()) => Ok(()),
        _ => Err(PageReadError::RestrictedDestination),
    }
}

fn request_error(error: reqwest::Error) -> PageReadError {
    if error.is_timeout() {
        return PageReadError::Timeout;
    }
    // The connector wraps our DNS rejection. Never log its error chain or URL.
    let mut cause = error.source();
    while let Some(inner) = cause {
        if let Some(reason) = inner.downcast_ref::<PageReadError>() {
            return *reason;
        }
        cause = inner.source();
    }
    PageReadError::Connection
}

pub(super) async fn fetch(
    client: &Client,
    mut url: Url,
    deadline: Instant,
) -> Result<Page, PageReadError> {
    for hop in 0..=MAX_REDIRECTS {
        // IP literals bypass the connector's DNS resolver.
        validate_destination(&url)?;
        url.set_fragment(None);
        let response = client
            .get(url)
            .header(ACCEPT, "text/html")
            .header(ACCEPT_ENCODING, "identity")
            .send()
            .await
            .map_err(request_error)?;
        if response.status().is_redirection() {
            if hop == MAX_REDIRECTS {
                return Err(PageReadError::Redirect);
            }
            let location = response
                .headers()
                .get(LOCATION)
                .ok_or(PageReadError::Redirect)?
                .to_str()
                .map_err(|_| PageReadError::Redirect)?;
            url = response
                .url()
                .join(location)
                .map_err(|_| PageReadError::Redirect)?;
            continue;
        }
        if !response.status().is_success() {
            return Err(PageReadError::HttpStatus(response.status().as_u16()));
        }
        let mut transfer_codings = response.headers().get_all(TRANSFER_ENCODING).into_iter();
        // Hyper removes chunked framing, but does not decode other transfer
        // codings. Never feed the remaining compressed bytes to the text parser.
        if let Some(coding) = transfer_codings.next()
            && (!coding.as_bytes().eq_ignore_ascii_case(b"chunked")
                || transfer_codings.next().is_some())
        {
            return Err(PageReadError::UnsupportedEncoding);
        }
        if response
            .headers()
            .get_all(CONTENT_ENCODING)
            .iter()
            .any(|value| !value.as_bytes().eq_ignore_ascii_case(b"identity"))
        {
            return Err(PageReadError::UnsupportedEncoding);
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .ok_or(PageReadError::NotHtml)?;
        let transport_encoding = html_transport_encoding(content_type.as_bytes())?;
        let bytes = limited_body(response, MAX_PAGE_BODY).await?;
        // The blocking job uses the same deadline as retrieval and owns its
        // permit until parsing finishes or the budget stops it.
        let permit = PAGE_PARSE_SLOTS
            .acquire()
            .await
            .map_err(|_| PageReadError::ParserUnavailable)?;
        return tokio::task::spawn_blocking(move || {
            let _permit = permit;
            parse(&bytes, transport_encoding, deadline)
        })
        .await
        .map_err(|_| PageReadError::ParserUnavailable)?;
    }
    Err(PageReadError::Redirect)
}

fn html_transport_encoding(value: &[u8]) -> Result<Option<&'static Encoding>, PageReadError> {
    let mut fields = value.splitn(2, |&byte| byte == b';');
    let essence = fields.next().unwrap_or_default().trim_ascii();
    if !essence.eq_ignore_ascii_case(b"text/html") {
        return Err(PageReadError::NotHtml);
    }
    let Some(parameters) = fields.next() else {
        return Ok(None);
    };
    // Only charset affects extraction. Do not validate unrelated parameters,
    // but respect HTTP quoted strings when looking for the next parameter.
    let mut quoted = false;
    let mut escaped = false;
    for parameter in parameters.split(|&byte| {
        if escaped {
            escaped = false;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if !quoted && byte == b';' {
            return true;
        }
        false
    }) {
        let mut pair = parameter.splitn(2, |&byte| byte == b'=');
        if pair
            .next()
            .unwrap_or_default()
            .trim_ascii()
            .eq_ignore_ascii_case(b"charset")
            && let Some(value) = pair.next()
        {
            return Ok(http_charset_label(value.trim_ascii()));
        }
    }
    Ok(None)
}

fn http_charset_label(value: &[u8]) -> Option<&'static Encoding> {
    let Some(quoted) = value.strip_prefix(b"\"") else {
        return Encoding::for_label_no_replacement(value);
    };
    let label = quoted.strip_suffix(b"\"")?;
    if !label.contains(&b'\\') {
        return Encoding::for_label_no_replacement(label);
    }
    let mut unescaped = Vec::with_capacity(label.len());
    let mut bytes = label.iter().copied();
    while let Some(byte) = bytes.next() {
        unescaped.push(if byte == b'\\' { bytes.next()? } else { byte });
    }
    Encoding::for_label_no_replacement(&unescaped)
}

fn parse(
    bytes: &[u8],
    transport_encoding: Option<&'static Encoding>,
    deadline: Instant,
) -> Result<Page, PageReadError> {
    let budget = document::Budget::new(deadline);
    let html = match Encoding::for_bom(bytes)
        .map(|(encoding, _)| encoding)
        .or(transport_encoding)
    {
        Some(encoding) => encoding.decode(bytes).0,
        None => match html_encoding(bytes, &budget)? {
            Some(encoding) => encoding.decode(bytes).0,
            None => match std::str::from_utf8(bytes) {
                Ok(text) => text.into(),
                // Undeclared legacy HTML commonly uses Windows-1252. Do not
                // manufacture replacement characters for its non-UTF-8 bytes.
                Err(_) => WINDOWS_1252.decode_without_bom_handling(bytes).0,
            },
        },
    };
    let document = budget.parse(&html)?;
    Ok(extract(&document))
}

fn html_encoding(
    bytes: &[u8],
    budget: &document::Budget,
) -> Result<Option<&'static Encoding>, PageReadError> {
    let prefix = &bytes[..bytes.len().min(HTML_CHARSET_SNIFF_BYTES)];
    // The HTML prescan recognizes a UTF-16 XML prefix before scanning for
    // ASCII meta tags. An ASCII XML declaration is only a fallback.
    if prefix.starts_with(b"<\0?\0x\0") {
        return Ok(Some(UTF_16LE));
    }
    if prefix.starts_with(b"\0<\0?\0x") {
        return Ok(Some(UTF_16BE));
    }
    Ok(meta_encoding(prefix, budget)?.or_else(|| xml_encoding(prefix)))
}

fn meta_encoding(
    prefix: &[u8],
    budget: &document::Budget,
) -> Result<Option<&'static Encoding>, PageReadError> {
    static META: LazyLock<Selector> =
        LazyLock::new(|| Selector::parse("meta[charset], meta[http-equiv][content]").unwrap());
    if !prefix
        .windows(5)
        .any(|text| text.eq_ignore_ascii_case(b"<meta"))
    {
        return Ok(None);
    }
    // The declaration itself is ASCII. Only inspect the HTML standard's
    // initial sniffing window, not the full body decoded with a guess.
    let document = budget.parse(&String::from_utf8_lossy(prefix))?;
    let encoding = document.select(&META).find_map(|element| {
        if let Some(charset) = element.attr("charset") {
            return Encoding::for_label_no_replacement(charset.as_bytes());
        }
        if !element
            .attr("http-equiv")?
            .eq_ignore_ascii_case("content-type")
        {
            return None;
        }
        pragma_encoding(element.attr("content")?.as_bytes())
    });
    Ok(encoding.map(|encoding| match encoding.name() {
        "UTF-16BE" | "UTF-16LE" => UTF_8,
        "x-user-defined" => WINDOWS_1252,
        _ => encoding,
    }))
}

fn xml_encoding(prefix: &[u8]) -> Option<&'static Encoding> {
    let declaration = prefix.strip_prefix(b"<?xml")?;
    let declaration = &declaration[..declaration.iter().position(|&byte| byte == b'>')?];
    let key = b"encoding";
    let start = declaration.windows(key.len()).position(|s| s == key)?;
    let mut value = declaration[start + key.len()..].iter();
    if *value.find(|&&byte| byte > b' ')? != b'=' {
        return None;
    }
    let quote = *value.find(|&&byte| byte > b' ')?;
    if !matches!(quote, b'\'' | b'"') {
        return None;
    }
    let rest = value.as_slice();
    let label = &rest[..rest.iter().position(|&byte| byte == quote)?];
    if label.iter().any(|&byte| byte <= b' ') {
        return None;
    }
    let encoding = Encoding::for_label(label)?;
    Some(if encoding == UTF_16LE || encoding == UTF_16BE {
        UTF_8
    } else {
        encoding
    })
}

fn pragma_encoding(mut content: &[u8]) -> Option<&'static Encoding> {
    // HTML's legacy meta pragma is not HTTP MIME syntax: it allows ASCII
    // whitespace around '=' and single as well as double quotes.
    while let Some(start) = content
        .windows(7)
        .position(|text| text.eq_ignore_ascii_case(b"charset"))
    {
        content = content[start + 7..].trim_ascii_start();
        let Some(value) = content.strip_prefix(b"=") else {
            continue;
        };
        let value = value.trim_ascii_start();
        let (first, rest) = value.split_first()?;
        let label = match first {
            b'\'' | b'"' => &rest[..rest.iter().position(|byte| byte == first)?],
            _ => value
                .split(|byte| byte.is_ascii_whitespace() || *byte == b';')
                .next()?,
        };
        return Encoding::for_label_no_replacement(label);
    }
    None
}

fn extract(document: &Html) -> Page {
    static TITLE: LazyLock<Selector> = LazyLock::new(|| Selector::parse("title").unwrap());
    static BODY: LazyLock<Selector> = LazyLock::new(|| Selector::parse("body").unwrap());

    let root = document.root_element();
    let title = root
        .select(&TITLE)
        .find(|element| {
            &*element.value().name.ns == "http://www.w3.org/1999/xhtml"
                && !element.ancestors().any(|node| node.value().is_fragment())
        })
        .and_then(|element| {
            let mut title = Text::new(MAX_TITLE);
            (element.text().all(|text| title.append(text)) && !title.value.is_empty())
                .then_some(title.value)
        });
    // Reserve the marker before collecting text; adding it must not alter
    // an expression that already fit in the excerpt.
    let mut excerpt = Text::new(MAX_EXCERPT - EXCERPT_CUTOFF.len());
    if !initially_hidden(root)
        && let Some(body) = root.select(&BODY).next()
    {
        enum Visit<N> {
            Node(N),
            OptionText(N),
            Separate,
            CloseScript,
        }

        let closed_by_group = LazyCell::new(|| implicitly_closed_details(root));
        // Walk iteratively; omitted subtrees never contribute descendant text.
        let mut nodes = vec![Visit::Node(*body)];
        let clipped = loop {
            let Some(visit) = nodes.pop() else {
                break false;
            };
            let node = match visit {
                Visit::Node(node) => node,
                Visit::OptionText(node) => {
                    // Native option labels collect text, not rendered subtrees.
                    if let Some(text) = node.value().as_text() {
                        if !excerpt.append(text) {
                            break true;
                        }
                    } else if let Some(child) = ElementRef::wrap(node)
                        && !matches!(child.value().name(), "script" | "template")
                    {
                        nodes.extend(node.children().rev().map(Visit::OptionText));
                    }
                    continue;
                }
                Visit::Separate => {
                    excerpt.separate();
                    continue;
                }
                Visit::CloseScript => {
                    if !excerpt.end_script() {
                        break true;
                    }
                    continue;
                }
            };
            if let Some(text) = node.value().as_text() {
                if !excerpt.append(text) {
                    break true;
                }
            } else if let Some(element) = ElementRef::wrap(node)
                && !matches!(
                    element.value().name(),
                    "title"
                        | "script"
                        | "style"
                        | "template"
                        | "noscript"
                        | "noembed"
                        | "noframes"
                        | "datalist"
                        | "rp"
                        | "rt"
                        | "rtc"
                )
                && !initially_hidden(element)
            {
                let name = element.value().name();
                if (name == "dialog" && element.attr("open").is_none())
                    || (name == "audio" && element.attr("controls").is_none())
                {
                    continue;
                }
                if name == "details"
                    && (element.attr("open").is_none()
                        || (element.attr("name").is_some_and(|name| !name.is_empty())
                            && closed_by_group.contains(&node.id())))
                {
                    excerpt.separate();
                    if let Some(summary) = element
                        .child_elements()
                        .find(|child| child.value().name() == "summary")
                    {
                        nodes.push(Visit::Node(*summary));
                    }
                    continue;
                }
                if matches!(name, "math" | "svg" | "iframe" | "del" | "s" | "strike")
                    || matches!(
                        name,
                        "audio" | "video" | "canvas" | "meter" | "progress" | "embed"
                    )
                    || (name == "object"
                        && element.attr("data").is_some_and(|data| !data.is_empty()))
                {
                    // Omitted visible content must not join neighboring text.
                    excerpt.separate();
                    continue;
                }
                if name == "img" {
                    if let Some(alt) = element.attr("alt").filter(|alt| !alt.trim().is_empty()) {
                        if !excerpt.append(alt) {
                            break true;
                        }
                    } else {
                        excerpt.separate();
                    }
                    continue;
                }
                if matches!(name, "sup" | "sub") {
                    excerpt.begin_script(if name == "sup" { "^(" } else { "_(" });
                    nodes.push(Visit::CloseScript);
                }
                if name == "input" {
                    let kind = element.attr("type").unwrap_or_default();
                    if kind.eq_ignore_ascii_case("hidden") {
                        continue;
                    }
                    let label = if kind.eq_ignore_ascii_case("image") {
                        element.attr("alt")
                    } else if ["button", "submit", "reset"]
                        .iter()
                        .any(|button| kind.eq_ignore_ascii_case(button))
                    {
                        element.attr("value")
                    } else {
                        None
                    };
                    excerpt.separate();
                    if let Some(label) = label
                        && !excerpt.append(label)
                    {
                        break true;
                    }
                    excerpt.separate();
                    continue;
                }
                if is_block(name) {
                    excerpt.separate();
                    nodes.push(Visit::Separate);
                }
                if let Some(template) = declarative_shadow_root(element) {
                    // The parser stores template contents in a document fragment.
                    // Do not substitute light DOM or slot fallback for projected text.
                    if let Some(fragment) = template.first_child() {
                        if has_shadow_slot(template) {
                            excerpt.separate();
                        } else {
                            nodes.extend(fragment.children().rev().map(Visit::Node));
                        }
                    }
                    continue;
                }
                if name == "select" && is_listbox(element) {
                    // Only assigned elements render in the native listbox;
                    // direct text is not a row. Keep option wrappers intact.
                    nodes.extend(element.children().rev().filter_map(|node| {
                        let child = node.value().as_element()?;
                        matches!(child.name(), "option" | "optgroup" | "hr" | "div" | "span")
                            .then_some(Visit::Node(node))
                    }));
                    continue;
                }
                // A collapsed select displays its selected label even if the
                // option or optgroup is hidden in the picker.
                let content = if name == "select" {
                    let Some(option) = selected_option(element) else {
                        continue;
                    };
                    option
                } else {
                    element
                };
                let name = content.value().name();
                // A listbox renders its first legend as markup in place of
                // the attribute label. Options display computed label text.
                let renders_legend = name == "optgroup"
                    && content
                        .child_elements()
                        .any(|child| child.value().name() == "legend");
                if matches!(name, "option" | "optgroup")
                    && !renders_legend
                    && let Some(label) = content.attr("label").filter(|label| !label.is_empty())
                {
                    if !excerpt.append(label) {
                        break true;
                    }
                    if name == "option" {
                        continue;
                    }
                    excerpt.separate();
                }
                if name == "option" {
                    nodes.extend(content.children().rev().map(Visit::OptionText));
                    continue;
                }
                nodes.extend(content.children().rev().map(Visit::Node));
            }
        };
        if clipped {
            if excerpt.opened_scripts != 0 {
                excerpt.value.truncate(excerpt.expression_start);
            }
            if !excerpt.value.is_empty() {
                excerpt.value.push_str(EXCERPT_CUTOFF);
            }
        }
    }
    Page {
        title,
        excerpt: excerpt.value,
    }
}

// Hidden members still participate. Each template fragment (including a
// declarative shadow root) is a separate tree with its own name groups.
fn implicitly_closed_details(root: ElementRef<'_>) -> HashSet<document::Node> {
    let mut opened = HashMap::new();
    let mut closed = HashSet::new();
    let mut nodes = vec![(*root, root.id())];
    while let Some((node, tree)) = nodes.pop() {
        let tree = if node.value().is_fragment() {
            node.id()
        } else {
            tree
        };
        if let Some(element) = ElementRef::wrap(node)
            && element.value().name() == "details"
            && &*element.value().name.ns == "http://www.w3.org/1999/xhtml"
            && element.attr("open").is_some()
            && let Some(name) = element.attr("name").filter(|name| !name.is_empty())
        {
            // Table foster parenting can move a later panel earlier in DOM
            // order. Arena IDs retain creation order for parsed details tags.
            let id = node.id();
            let first = opened.entry((tree, name)).or_insert(id);
            if *first != id {
                closed.insert((*first).max(id));
                *first = (*first).min(id);
            }
        }
        nodes.extend(node.children().rev().map(|child| (child, tree)));
    }
    closed
}

fn declarative_shadow_root(host: ElementRef<'_>) -> Option<ElementRef<'_>> {
    let template = host.child_elements().find(|child| {
        child.value().name() == "template"
            && child.attr("shadowrootmode").is_some_and(|mode| {
                mode.eq_ignore_ascii_case("open") || mode.eq_ignore_ascii_case("closed")
            })
    })?;
    let name = host.value().name();
    // HTML parser names are already lowercase. Hyphenated custom element
    // names are eligible except for the reserved SVG and MathML names.
    let eligible = matches!(
        name,
        "article"
            | "aside"
            | "blockquote"
            | "body"
            | "div"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "main"
            | "nav"
            | "p"
            | "section"
            | "span"
    ) || (name.contains('-')
        && !matches!(
            name,
            "annotation-xml"
                | "color-profile"
                | "font-face"
                | "font-face-src"
                | "font-face-uri"
                | "font-face-format"
                | "font-face-name"
                | "missing-glyph"
        ));
    (eligible && &*host.value().name.ns == "http://www.w3.org/1999/xhtml").then_some(template)
}

fn has_shadow_slot(template: ElementRef<'_>) -> bool {
    // Stay inside this root: slots inside nested templates are not its slots.
    let Some(fragment) = template.first_child() else {
        return false;
    };
    let mut nodes: Vec<_> = fragment.children().filter_map(ElementRef::wrap).collect();
    while let Some(element) = nodes.pop() {
        match element.value().name() {
            "slot" => return true,
            "template" => {}
            _ => nodes.extend(element.child_elements()),
        }
    }
    false
}

fn initially_hidden(element: ElementRef<'_>) -> bool {
    element.attr("hidden").is_some() || element.attr("popover").is_some()
}

fn is_listbox(select: ElementRef<'_>) -> bool {
    // HTML's non-negative integer syntax admits leading whitespace, '+',
    // leading zeroes and trailing non-digits. Classify without overflow.
    let size = select.attr("size").unwrap_or_default().trim_ascii_start();
    let size = size
        .strip_prefix('+')
        .unwrap_or(size)
        .bytes()
        .take_while(u8::is_ascii_digit)
        .try_fold(0u32, |n, digit| {
            n.checked_mul(10)?.checked_add(u32::from(digit - b'0'))
        });
    match size {
        None | Some(0) => select.attr("multiple").is_some(),
        Some(n) => n > 1,
    }
}

fn selected_option(select: ElementRef<'_>) -> Option<ElementRef<'_>> {
    // HTML's option list in tree order, including wrappers but excluding
    // nested controls and nested optgroups. Carry group state down once.
    let multiple = select.attr("multiple").is_some();
    let mut nodes = vec![(select, None)];
    let mut selected = None;
    let mut first_enabled = None;
    while let Some((element, group_disabled)) = nodes.pop() {
        let group_disabled = match element.value().name() {
            "option" => {
                if element.attr("selected").is_some() {
                    // A closed multiple menu shows a count, not an option
                    // label, when more than one option is selected.
                    if multiple && selected.is_some() {
                        return None;
                    }
                    selected = Some(element);
                } else if !multiple
                    && first_enabled.is_none()
                    && group_disabled != Some(true)
                    && element.attr("disabled").is_none()
                {
                    first_enabled = Some(element);
                }
                continue;
            }
            "select" if element != select => continue,
            "datalist" | "hr" | "template" => continue,
            "optgroup" if group_disabled.is_some() => continue,
            "optgroup" => Some(element.attr("disabled").is_some()),
            _ => group_disabled,
        };
        nodes.extend(
            element
                .children()
                .rev()
                .filter_map(ElementRef::wrap)
                .map(|child| (child, group_disabled)),
        );
    }
    selected.or(first_enabled)
}

// Default HTML flow, list, control, and table boundaries, without CSS.
fn is_block(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "caption"
            | "center"
            | "blockquote"
            | "br"
            | "button"
            | "dd"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "legend"
            | "li"
            | "listing"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "optgroup"
            | "option"
            | "p"
            | "plaintext"
            | "pre"
            | "search"
            | "section"
            | "select"
            | "summary"
            | "table"
            | "textarea"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
            | "ul"
            | "xmp"
    )
}

struct Text {
    value: String,
    limit: usize,
    pending_space: bool,
    // Last safe truncation point outside the current attached expression.
    expression_start: usize,
    scripts: Vec<&'static str>,
    opened_scripts: usize,
}

impl Text {
    fn new(limit: usize) -> Self {
        Self {
            value: String::new(),
            limit,
            pending_space: false,
            expression_start: 0,
            scripts: Vec::new(),
            opened_scripts: 0,
        }
    }

    fn separate(&mut self) {
        self.pending_space = !self.value.is_empty();
        if self.opened_scripts == 0 {
            self.expression_start = self.value.len();
        }
    }

    fn begin_script(&mut self, prefix: &'static str) {
        self.scripts.push(prefix);
    }

    fn open_pending_scripts(&mut self) -> bool {
        // Empty scripts never spend excerpt space. Delay their prefixes until
        // the first visible, non-whitespace text reaches the collector.
        while let Some(&prefix) = self.scripts.get(self.opened_scripts) {
            self.opened_scripts += 1;
            if !self.append_run(prefix) {
                return false;
            }
        }
        true
    }

    fn end_script(&mut self) -> bool {
        if self.opened_scripts == self.scripts.len() {
            self.pending_space = false;
            if !self.append_run(")") {
                return false;
            }
            self.opened_scripts -= 1;
        }
        self.scripts.pop();
        true
    }

    fn append(&mut self, text: &str) -> bool {
        let sentence_stop = |ch| matches!(ch, '。' | '！' | '？');
        for part in text.split_inclusive(|ch: char| ch.is_whitespace() || sentence_stop(ch)) {
            let run = part.trim_end_matches(char::is_whitespace);
            if !run.is_empty() && (!self.open_pending_scripts() || !self.append_run(run)) {
                return false;
            }
            if run.len() != part.len() {
                self.separate();
            } else if self.scripts.is_empty() && run.ends_with(sentence_stop) {
                self.expression_start = self.value.len();
            }
        }
        true
    }

    fn append_run(&mut self, run: &str) -> bool {
        let space = usize::from(self.pending_space);
        let remaining = self.limit - self.value.len();
        if remaining <= space {
            return false;
        }
        let prefix = run.floor_char_boundary(remaining - space);
        if prefix == 0 {
            return false;
        }
        if self.pending_space {
            self.value.push(' ');
            self.pending_space = false;
        }
        self.value.push_str(&run[..prefix]);
        prefix == run.len()
    }
}
