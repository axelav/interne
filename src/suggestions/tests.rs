use std::sync::Mutex;

use axum::{
    Json, Router,
    body::{Body, Bytes, to_bytes},
    extract::State,
    http::{HeaderMap, Request, StatusCode, header},
    response::{Html, Redirect, Response},
    routing::{get, post},
};
use http_body_util::{BodyStream, Full, StreamBody};
use serde_json::{Value, json};
use tokio::{net::TcpListener, task::JoinHandle};
use tower::ServiceExt;
use tower_sessions::Session;
use tower_sessions_sqlx_store::SqliteStore;

use super::*;
use crate::{
    AuthServices,
    auth::login_user,
    config::{AuthConfig, SignupMode},
    github::GitHubOAuthClient,
};

const TEST_KEY: &str = "fixture-key";
const DESCRIPTION: &str = "The article explains how to repair a bicycle chain.";
const TITLE: &str = "Repair & maintenance";
// gzip of a public HTML document about growing herbs.
const GZIP_HTML: &[u8] = &[
    31, 139, 8, 0, 0, 0, 0, 0, 2, 19, 179, 41, 201, 44, 201, 73, 181, 115, 79, 44, 74, 73, 205,
    179, 209, 135, 240, 108, 10, 236, 60, 242, 203, 21, 74, 242, 21, 10, 114, 18, 243, 74, 20, 50,
    82, 139, 146, 138, 245, 108, 244, 11, 236, 0, 99, 74, 190, 92, 47, 0, 0, 0,
];

fn streamed(bytes: &'static [u8]) -> Body {
    // Do not advertise a content length for transfer-coded responses.
    Body::new(StreamBody::new(BodyStream::new(Full::new(
        Bytes::from_static(bytes),
    ))))
}

#[derive(Default)]
struct Outbound {
    pages: Mutex<Vec<(String, HeaderMap)>>,
    provider: Mutex<Vec<(HeaderMap, Value)>>,
}

struct Fixture {
    app: Router,
    cookie: String,
    db: sqlx::SqlitePool,
    origin: String,
    upstream: Arc<Outbound>,
    server: JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Fixture {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let origin = format!("http://public.example:{}", addr.port());
        let upstream = Arc::new(Outbound::default());
        let record_page = |name: &str, headers, upstream: &Outbound| {
            upstream.pages.lock().unwrap().push((name.into(), headers));
        };
        let outbound = Router::new()
            .route("/article", get(move |State(upstream): State<Arc<Outbound>>, headers: HeaderMap| async move {
                record_page("article", headers, &upstream);
                Redirect::temporary("/final")
            }))
            .route("/final", get(move |State(upstream): State<Arc<Outbound>>, headers: HeaderMap| async move {
                record_page("final", headers, &upstream);
                Html(r#"<title> Repair &amp; maintenance </title><link rel="canonical" href="https://example.org/">
                    <main><p>The article explains how to repair a bicycle chain.</p>
                    <script>private script contents</script><p hidden>hidden contents</p></main>"#)
            }))
            .route("/private-redirect", get(move || async move {
                Redirect::temporary(&format!("http://{addr}/private"))
            }))
            .route("/private-dns-redirect", get(move || async move {
                Redirect::temporary(&format!("http://localhost:{}/private", addr.port()))
            }))
            .route("/private", get(move |State(upstream): State<Arc<Outbound>>, headers: HeaderMap| async move {
                record_page("private", headers, &upstream);
                Html("<title>Private title</title>Private page contents")
            }))
            .route("/no-title", get(|| async { Html("<p>How to repair a bicycle chain.</p>") }))
            .route("/ai", get(|| async { Html("<title>Lecture</title><p>Artificial intelligence research.</p>") }))
            .route("/invalid/{kind}", get(|axum::extract::Path(kind): axum::extract::Path<String>| async move {
                Html(format!("<title>Usable title</title><p>Invalid {kind}</p>"))
            }))
            .route("/title-only", get(|| async { Html("<title>Just a title</title><script>not evidence</script>") }))
            .route("/hidden-root", get(|| async {
                Html("<html hidden><head><title>Public title</title></head><body><p>Unshown draft</p></body></html>")
            }))
            .route("/hidden-body", get(|| async {
                Html("<head><title>Public title</title></head><body hidden><p>Unshown draft</p></body>")
            }))
            .route("/streamed-title", get(|| async {
                Html(concat!(
                    "<html><head></head><body>",
                    "<template><title>Reusable template</title></template>",
                    "<svg><title>Icon title</title></svg>",
                    "<article><p>How to repair a bicycle chain.</p></article>",
                    "<title>Streamed article title</title>",
                    "</body></html>",
                ))
            }))
            .route("/malformed-reply", get(|| async { Html("<title>Usable title</title><p>Bad model reply fixture</p>") }))
            .route("/too-large", get(|| async {
                Html(format!("<title>Do not suggest</title><meta name=description content='Too large'><ul>{}</ul>", "<li>Basil</li>".repeat(45_000)))
            }))
            .route("/refused", get(|| async {
                (StatusCode::FORBIDDEN, Html("<title>Do not suggest</title><meta name=description content='private refusal body'>"))
            }))
            .route("/field-notes", get(|| async {
                Html(format!(
                    "<title>Field notes</title><div hidden>{}</div><p>Seasonal planting notes.</p>",
                    "soil &amp; water; ".repeat(9000)
                ))
            }))
            .route("/legacy-charset", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html ;\t; note=\"before; ; charset=us-ascii; \\\"after\";; charset=\"windows-\\1252\"; ;")],
                    &b"<?xml encoding='utf-8'?><title>Caf\xe9</title><p>About caf\xe9s</p>"[..],
                )
            }))
            .route("/undeclared-legacy", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html")],
                    &b"<title>Tickets \xa310</title><p>Entry costs \x8012.</p>"[..],
                )
            }))
            .route("/http-token-charset", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html; revision=1.0~beta; charset=windows-1252")],
                    &b"<title>Caf\xe9</title><p>About caf\xe9s</p>"[..],
                )
            }))
            .route("/plain-charset", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/plain; revision=1.0~beta; charset=windows-1252")],
                    &b"<title>Caf\xe9</title><p>About caf\xe9s</p>"[..],
                )
            }))
            .route("/meta-charset", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html")],
                    &b"<?xml encoding='utf-8'?><meta charset=windows-1252><title>Caf\xe9</title><p>About caf\xe9s</p>"[..],
                )
            }))
            .route("/pragma-charset", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html")],
                    &b"<meta http-equiv=Content-Type content=\"text/html;\n\tCHARSET = 'Windows-1252'\"><title>Caf\xe9</title><p>About caf\xe9s</p>"[..],
                )
            }))
            .route("/xml-charset/{kind}", get(|axum::extract::Path(kind): axum::extract::Path<String>| async move {
                let utf8 = "<?xml encoding='utf-16'?><title>Café</title><p>About cafés</p>";
                let bytes = match kind.as_str() {
                    "legacy" => b"<?xml version='1.0' encoding = \"windows-1252\"?><title>Caf\xe9</title><p>About caf\xe9s</p>".to_vec(),
                    "unrelated-meta" => b"<?xml encoding='windows-1252'?><meta name=author><title>Caf\xe9</title><p>About caf\xe9s</p>".to_vec(),
                    "bom" => "\u{feff}<?xml encoding='windows-1252'?><title>Café</title><p>About cafés</p>".as_bytes().to_vec(),
                    "malformed" => "<?xml encoding=' windows-1252 '?><title>Café</title><p>About cafés</p>".as_bytes().to_vec(),
                    "ascii-utf16" => utf8.as_bytes().to_vec(),
                    "utf16le" => utf8.encode_utf16().flat_map(u16::to_le_bytes).collect(),
                    "utf16be" => utf8.encode_utf16().flat_map(u16::to_be_bytes).collect(),
                    _ => unreachable!(),
                };
                ([(header::CONTENT_TYPE, "text/html")], bytes)
            }))
            .route("/compressed", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html"), (header::CONTENT_ENCODING, "gzip")],
                    GZIP_HTML,
                )
            }))
            .route("/gzip-transfer", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html"), (header::TRANSFER_ENCODING, "gzip, chunked")],
                    streamed(GZIP_HTML),
                )
            }))
            .route("/chunked", get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/html"), (header::TRANSFER_ENCODING, "chunked")],
                    streamed(b"<title>Garden</title><p>How to plant herbs.</p>"),
                )
            }))
            .route("/xhtml", get(|| async {
                (
                    [(header::CONTENT_TYPE, "application/xhtml+xml")],
                    r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title><![CDATA[Repair & maintenance]]></title></head><body><p>About repairs.</p></body></html>"#,
                )
            }))
            .route("/inline-text", get(|| async {
                Html(concat!(
                    "<p>Version 1.<strong>2</strong> is supported.</p>",
                    "<p>See the <em>guide</em>.<br>Read it first.</p>",
                    "<details open><summary>Ingredients</summary>Flour</details>",
                    "More resources<search>Find a guide</search>Recipes.",
                    "<p><label>Memory capacities (GB)<select multiple size=2><option>16</option><option>32</option></select></label></p>",
                    "<p>Upgrade: 16<svg><title>arrow internals</title></svg>32 GB.</p>",
                    "<p>Ages 6<img src='/art' alt='–'>12. Keep co<img hidden src='/art' alt='omit'>de.</p>",
                    "<p>Area: 10<sup><em>3</em></sup> cm. Water: H<sub>2</sub>O.</p>",
                    "<p>Sizes 16<sup>&nbsp;</sup>32 and 8<sup><svg></svg></sup>16. Version 1.<sup><i hidden>bad</i></sup>2.</p>",
                    "<p>Price: <del>199</del><ins>99</ins> USD.</p>",
                    "<p>Sale: <s>249</s><strong>149</strong> USD.</p>",
                    "<p>Today: <strike>399</strike><ins>299</ins> USD.</p>",
                    "<p>Watch clips 1<video controls>Install a video player.</video>2. ",
                    "Listen to co<audio>Unsupported audio</audio>de samples.</p>",
                    "<p>Samples: 10<progress value='8' max='10'>20% done</progress>20<meter value='0.5'>empty</meter>30.</p>",
                    "<p>Sizes: 16<canvas>Obsolete size</canvas>32 GB.</p>",
                    "<x-registration><template shadowrootmode='open'>Registration closed.</template>Register now.</x-registration>",
                    "<details><summary>Specifications</summary>not yet available</details>",
                ))
            }))
            .route("/embedded-resources", get(|| async {
                Html(concat!(
                    "<p>Memory: 16<embed src='/final'>32 GB.</p>",
                    "<p>Range: 10<object data='/final'>Unavailable</object>20 mm.</p>",
                    "<p>Read the <object>printed</object> co<noembed>obsolete</noembed><noframes>obsolete</noframes>de.</p>",
                    "<select multiple><optgroup label='Old'><legend>Sizes: 6<object data='/final'>unavailable</object>8</legend><option>Wide</option></optgroup></select>",
                    "<select><option>Not<noembed> yet</noembed> paid</option></select>",
                ))
            }))
            .route("/disclosure-groups", get(|| async {
                Html(concat!(
                    "<template><details open name='shipping'>Inert</details></template>",
                    "<details name='shipping'><summary>Pickup</summary>Not selected</details>",
                    "<details open name='shipping'><summary>Standard</summary>Five days.</details>",
                    "<section><details open name='shipping'><summary>Express</summary>Tomorrow.</details></section>",
                    "<details open name='returns'><summary>Returns</summary>Free.</details>",
                    "<div hidden><details open name='staff'>Hidden first member.</details></div>",
                    "<details open name='staff'><summary>Staff</summary>Not public.</details>",
                    "<div><template shadowrootmode='open'><details open name='shipping'><summary>Local</summary>Today.</details></template></div>",
                    "<table><tr><td><details open name='packing'><summary>Box</summary>First.</details></td></tr>",
                    "<details open name='packing'><summary>Bag</summary>Second.</details></table>",
                ))
            }))
            .route("/control-text", get(|| async {
                Html(concat!(
                    "<p>Available capacities</p><button><strong>16</strong>.5</button><button>32</button>",
                    "<select multiple size=3><optgroup label='Memory'>",
                    "<option label='64 GB' value='sku-1'></option>",
                    "<option label='128 GB' value='sku-2'>Obsolete label</option>",
                    "<option label='' value='sku-3'>256 GB</option>",
                    "</optgroup></select>",
                    "<label>Available widths (mm):</label><textarea>16</textarea><textarea>32</textarea>",
                    "<p>Lengths: 16<input type=BUTTON value=or>32</p>",
                    "<input type=submit value=Apply><input type=reset value=Clear>",
                    "<p><label><input type=radio value=internal-one>16</label><label><input type=radio value=internal-two>32</label>",
                    "<input type=image src='/art' alt='Choose capacity'><input type=text value='not an excerpt'> GB.</p>",
                    "<input type=hidden value=internal><input type=password value=redacted>",
                ))
            }))
            .route("/selected-options", get(|| async {
                Html(concat!(
                    "<p>First: <select><option value='internal-a' selected>Paid</option><option>Overdue</option></select></p>",
                    "<p>Second: <select><option>Paid</option><optgroup label='Status'><option selected label='Overdue'>Older text</option></optgroup></select></p>",
                    "<p>Default: <select><optgroup disabled label='Unavailable'><div><option>Ignored</option></div></optgroup><option>Ready</option><option>Later</option></select></p>",
                    "<p>Picker: <select><option>Wrong</option><option hidden selected label='Current'>Old</option></select></p>",
                    "<p>Group: <select><optgroup hidden label='Group'><option selected>On hold</option></optgroup></select></p>",
                    "<p>List: <select multiple><option hidden selected label='Hidden'></option><option>Shown</option></select></p>",
                    "<p>Stock: <select multiple><optgroup label='Sold out'><legend><span hidden>In </span>stock</legend><option>Basil</option></optgroup></select></p>",
                    "<p>Wrapped: <select><option>Pending</option><div><option hidden selected><span hidden>Not </span>paid<script>ignore</script></option></div></select></p>",
                    "<select multiple><optgroup label='Sold out'><template></template><legend>In stock</legend><option>Basil</option></optgroup></select>",
                ))
            }))
            .route("/multiselect-sizes", get(|| async {
                Html(concat!(
                    "<p>Menu: <select multiple size=1><option>Draft</option><option selected>Approved</option></select></p>",
                    "<p>Rows: <select multiple size=2><option>Draft</option><option selected>Approved</option></select></p>",
                    "<p>Picker: <select multiple size=' +001x'><optgroup hidden><option selected><span hidden>Not </span>paid</option></optgroup><option>Paid</option></select></p>",
                    "<p>Zero: <select multiple size=0><option>Early</option><option>Late</option></select></p>",
                    "<p>Invalid: <select multiple size=bad><option>Low</option><option>High</option></select></p>",
                    "<p>Image: <select multiple size=1><option selected><img alt='Not '>available</option></select></p>",
                    "<p>Overflow: <select size=4294967296><option>Rejected</option><option selected>Approved</option></select></p>",
                ))
            }))
            .route("/multiselect-counts-only", get(|| async {
                Html(concat!(
                    "<title>Archive</title>",
                    "<select multiple size=1><option>Pending</option><option>Ready</option></select>",
                    "<select multiple size=1><option selected>Pending</option><option selected>Ready</option></select>",
                ))
            }))
            .route("/listbox-and-ruby", get(|| async {
                Html(concat!(
                    "<select multiple>Sold out<optgroup label='Stock'><option>Available</option></optgroup>Not for sale</select>",
                    "<select size='3'>Canceled<div><option>Ready</option></div></select>",
                    "<p>Year: <ruby>1446<rp> (</rp><rt>2025</rt><rp>)</rp></ruby>.</p>",
                    "<p><ruby>東<rt>とう</rt>京<rt>きょう</rt></ruby>駅</p>",
                ))
            }))
            .route("/listbox-text-only", get(|| async {
                Html("<title>Archive</title><select multiple>Sold out</select><select size=2>Canceled</select>")
            }))
            .route("/equation", get(|| async {
                Html("<p>Measurements<math><mfrac><mn>1</mn><mn>2</mn></mfrac><mi>cup</mi></math>Stir until smooth.</p>")
            }))
            .route("/structured-catalog/{kind}", get(move |axum::extract::Path(kind): axum::extract::Path<String>, State(upstream): State<Arc<Outbound>>, headers: HeaderMap| async move {
                record_page(&kind, headers, &upstream);
                let head = match kind.as_str() {
                    "both" => "<title>Plants &amp; herbs</title><META content='Grow A &gt; B' NAME=description>".into(),
                    "inert-template" => concat!(
                        "<template><ul><li><img src='/private'></li></ul>",
                        "<template><script>let text = '</template>';</script></template>",
                        "<meta name=description content='Not the page'><title>Not the title</title></template>",
                        "<title>Plant guide</title><meta name=description content='Soil notes'>",
                    ).into(),
                    "no-metadata" => "<meta name=author content='Not a description'>".into(),
                    "decoys" => concat!(
                        "<!-- <title>Comment</title><meta name=description content=comment> -->",
                        "<script>const s = '<title>Script</title><meta name=description content=script>';</script>",
                        "<template><title>Template</title><meta name=description content=template></template>",
                        "</head><body><title>Body</title><meta name=description content=body>",
                    ).into(),
                    "clipped-title" => format!("<meta name=description content='Plant guide'><title>Useful start{}end</title>", " ".repeat(16 * 1024)),
                    "clipped-meta" => format!("<title>Plant guide</title><meta name=description content='Useful start{}end'>", " ".repeat(16 * 1024)),
                    "late" => format!("{}<title>Late</title><meta name=description content=late>", " ".repeat(16 * 1024)),
                    _ => unreachable!(),
                };
                Html(format!("<head>{head}</head><ul>{}</ul>", "<li>Chamomile</li>".repeat(4200)))
            }))
            .route("/complex-charset-prescan/{kind}", get(move |axum::extract::Path(kind): axum::extract::Path<String>, State(upstream): State<Arc<Outbound>>, headers: HeaderMap| async move {
                record_page(&kind, headers, &upstream);
                let mut html = match kind.as_str() {
                    "ascii" => b"<title>Plants &amp; herbs</title><meta name=description content='Soil notes'>".to_vec(),
                    "utf8" => "<meta charset=utf-8><title>Café</title><meta name=description content='Thé vert'>".as_bytes().to_vec(),
                    "legacy" => b"<!-- \xe9 --><meta charset=windows-1252><title>Caf\xe9</title><meta name=description content='Th\xe9 vert'>".to_vec(),
                    _ => unreachable!(),
                };
                html.extend_from_slice(b"<div");
                for n in 0..140 {
                    html.extend_from_slice(format!(" a{n}").as_bytes());
                }
                html.push(b'>');
                ([(header::CONTENT_TYPE, "text/html")], html)
            }))
            .route("/catalogue-metadata", get(|| async {
                use std::fmt::Write;
                let mut html = String::from("<title>Catalogue</title><article");
                for column in 1..=129 {
                    write!(html, " data-column-{column}='available'").unwrap();
                }
                html.push_str(">The catalogue lists seasonal herbs.</article>");
                Html(html)
            }))
            .route("/catalogue-page", get(|| async {
                let mut html = String::from("<title>Catalogue</title><article data-notes='Estimated stock > zero.");
                for _ in 0..140 {
                    html.push_str(" Stock changes each week.");
                }
                html.push_str("'>The catalogue lists seasonal herbs.</article>");
                Html(html)
            }))
            .route("/formatting-catalogue", get(|| async {
                Html(format!(
                    "<title>Catalogue</title><ul>{}</ul>",
                    "<li><a href='/inventory' class='product'>Basil</a></li>".repeat(400),
                ))
            }))
            .route("/large-catalogue", get(|| async {
                let mut html = String::from("<title>Catalogue</title><ul>");
                for _ in 0..1800 {
                    html.push_str("<li data-family='herbs' data-season='summer' data-zone='6' data-stock='available'>Basil</li>");
                }
                html.push_str("</ul>");
                Html(html)
            }))
            .route("/long-passage", get(|| async { Html("<p>".to_owned() + &"自転車の修理。".repeat(500) + "</p>") }))
            .route("/long-formula", get(|| async {
                Html(format!(
                    "<p>{} 10<sup>30</sup> cm</p>",
                    "Measured. ".repeat(598) + "Last value.",
                ))
            }))
            .route("/clipped-attached", get(|| async {
                Html(format!("<p>{}Last: H<sub>2 3</sub><sup>{}</sup></p>", "Measured. ".repeat(597), "9".repeat(30)))
            }))
            .route("/clipped-detached", get(|| async {
                Html(format!("<p>{}Last: H<sub>2 3</sub></p><p><sup>{}</sup></p>", "Measured. ".repeat(597), "9".repeat(30)))
            }))
            .route("/clipped-annotation", get(|| async {
                Html(format!("<p>{}Approved.<sup> {}</sup></p>", "Measured. ".repeat(597), "9".repeat(30)))
            }))
            .route("/script-only", get(|| async {
                Html(format!("<title>Archive</title>H<sub>2 3</sub><sup>{}</sup>", "9".repeat(6000)))
            }))
            .route("/strike-only", get(|| async { Html("<title>Archive</title><strike>Sale price: 399</strike>") }))
            .route("/media-only", get(|| async {
                Html("<title>Archive</title><audio controls>Audio not supported.</audio><video>Video not supported.</video>")
            }))
            .route("/gauges-only", get(|| async {
                Html("<title>Archive</title><progress value='8' max='10'>20% done</progress><meter value='0.5'>empty</meter>")
            }))
            .route("/shadow-slot-only", get(|| async {
                Html("<title>Archive</title><div><template shadowrootmode='closed'><slot name='status'>Fallback</slot></template>Not projected</div>")
            }))
            .route("/legacy-fallback-only", get(|| async {
                Html(concat!(
                    "<title>Archive</title><body><noembed>Install a viewer.</noembed><noframes>Frames unavailable.</noframes>",
                    "<select multiple><optgroup label='Old'><legend><noembed>No viewer.</noembed><object data='/final'>Unavailable.</object></legend></optgroup></select></body>",
                ))
            }))
            .route("/formula-at-cutoff", get(|| async {
                Html(format!(
                    "<p>{}Observation: 10<sup>3</sup> followed by more prose.</p>",
                    "Measured. ".repeat(598),
                ))
            }))
            .route("/unspaced-formula", get(|| async {
                Html(format!(
                    "<p>{}完了。10<sup>30</sup></p>",
                    "測定しました。".repeat(285),
                ))
            }))
            .route("/empty-script-at-cutoff", get(|| async {
                Html(format!("<p>{}Done!!!<sup><span hidden>not shown</span></sup></p>", "Measured. ".repeat(599)))
            }))
            .route("/long-quantity", get(|| async {
                Html(format!("<p>{}Amount: 3200 units</p>", "Measured. ".repeat(599)))
            }))
            .route("/closed-dialog", get(|| async {
                Html("<title>Order status</title><dialog>Order canceled.</dialog>")
            }))
            .route("/closed-popover", get(|| async {
                Html("<title>Order status</title><aside popover>Order canceled.</aside>")
            }))
            .route("/closed-datalist", get(|| async {
                Html("<title>Order status</title><datalist><option label='Hidden status' value='canceled'></option></datalist>")
            }))
            .route("/open-dialog", get(|| async {
                Html("<title>Order status</title><dialog open>Order ready.</dialog>")
            }))
            .route("/responses", post(|State(upstream): State<Arc<Outbound>>, headers: HeaderMap, Json(mut body): Json<Value>| async move {
                let input: Value = serde_json::from_str(body["input"].as_str().unwrap()).unwrap();
                let mut reply = match input["excerpt"].as_str().unwrap() {
                    "Bad model reply fixture" => json!({}),
                    "Invalid shape" => json!({"description": DESCRIPTION, "tags": "one, two"}),
                    "Invalid delimiters" => json!({"description": DESCRIPTION, "tags": ["one,two", "bad\nlabel"]}),
                    "Invalid description" => json!({"description": [123], "tags": ["cycling"]}),
                    "Invalid missing-note" => json!({"tags": ["cycling"]}),
                    "Invalid null-note" => json!({"description": null, "tags": ["cycling"]}),
                    "Artificial intelligence research." => {
                        let candidates = input["candidate_tags"].as_array().unwrap();
                        let label = if candidates.iter().any(|tag| tag == "ai") { "ai" } else { "artificial intelligence" };
                        json!({"description": "Artificial intelligence research notes.", "tags": [label]})
                    }
                    _ => json!({"description": DESCRIPTION, "tags": ["cycling"]}),
                };
                let properties = &body["text"]["format"]["schema"]["properties"];
                reply.as_object_mut().unwrap().retain(|name, _| properties.get(name).is_some());
                body["input"] = input;
                upstream.provider.lock().unwrap().push((headers, body));
                let text = reply.to_string();
                Json(json!({
                    "status": "completed",
                    "output": [{"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]}]
                }))
            }))
            .with_state(upstream.clone());
        let server = tokio::spawn(async move { axum::serve(listener, outbound).await.unwrap() });

        // Route exactly one fixture hostname to the controlled page server. The real
        // page client still vets IP literals and resolves any other hostname normally.
        let pages = page::client().resolve("public.example", addr).build();
        let suggestions =
            LinkSuggestions::build(pages, &format!("http://{addr}/responses"), TEST_KEY).unwrap();
        let db = crate::db::init_pool("sqlite::memory:").await;
        let auth = AuthServices {
            config: AuthConfig::new("http://127.0.0.1:3000", SignupMode::Closed).unwrap(),
            github: Arc::new(GitHubOAuthClient::new("fixture", "fixture").unwrap()),
        };
        let app = crate::build_app(db.clone(), false, auth, Some(suggestions)).await;
        let cookie = Self::reader_cookie(&db, "42").await;
        Self {
            app,
            cookie,
            db,
            origin,
            upstream,
            server,
        }
    }

    async fn reader_cookie(db: &sqlx::SqlitePool, identity: &str) -> String {
        let now = chrono::Utc::now().to_rfc3339();
        let user = sqlx::query_as(
            "INSERT INTO users (id, name, github_user_id, github_login, created_at, updated_at) \
             VALUES (?, 'Reader', ?, ?, ?, ?) RETURNING *",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(identity)
        .bind(identity)
        .bind(&now)
        .bind(&now)
        .fetch_one(db)
        .await
        .unwrap();
        let session = Session::new(None, Arc::new(SqliteStore::new(db.clone())), None);
        login_user(&session, &user).await.unwrap();
        session.save().await.unwrap();
        format!("id={}", session.id().unwrap())
    }

    async fn send(&self, request: Request<Body>) -> Response {
        self.app.clone().oneshot(request).await.unwrap()
    }

    async fn get(&self, path: &str) -> String {
        let response = self
            .send(
                Request::get(path)
                    .header(header::COOKIE, &self.cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        String::from_utf8(
            to_bytes(response.into_body(), 32 * 1024)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }

    async fn suggest(&self, url: &str, authenticated: bool) -> Response {
        self.suggest_fields(
            url,
            authenticated,
            json!({"title": true, "description": true, "tags": true}),
        )
        .await
    }

    async fn suggest_fields(&self, url: &str, authenticated: bool, fields: Value) -> Response {
        let mut request =
            Request::post("/entries/suggest").header(header::CONTENT_TYPE, "application/json");
        if authenticated {
            request = request.header(header::COOKIE, &self.cookie);
        }
        self.send(
            request
                .body(Body::from(
                    json!({"url": url, "fields": fields}).to_string(),
                ))
                .unwrap(),
        )
        .await
    }

    async fn details(&self, path: &str) -> Value {
        let response = self.suggest(&format!("{}{path}", self.origin), true).await;
        assert_eq!(response.status(), StatusCode::OK);
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap()
    }

    async fn save(&self, url: &str, title: &str, description: &str, tags: &str) {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("url", url)
            .append_pair("title", title)
            .append_pair("description", description)
            .append_pair("tags", tags)
            .append_pair("duration", "1")
            .append_pair("interval", "weeks")
            .finish();
        let saved = self
            .send(
                Request::post("/entries")
                    .header(header::COOKIE, &self.cookie)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await;
        assert_eq!(saved.headers()[header::LOCATION], "/");
    }

    async fn saved_counts(&self) -> (i64, i64, i64, i64) {
        sqlx::query_as("SELECT (SELECT COUNT(*) FROM entries), (SELECT COUNT(*) FROM tags), (SELECT COUNT(*) FROM entry_tags), (SELECT COUNT(*) FROM visits)")
            .fetch_one(&self.db).await.unwrap()
    }
}

#[tokio::test]
async fn suggests_details_without_saving_or_replacing_the_original_target() {
    let fixture = Fixture::new().await;
    let url = format!("{}/article", fixture.origin);
    assert!(fixture.get("/entries/new").await.contains("to OpenAI"));
    let denied = fixture.suggest(&url, false).await;
    assert_eq!(denied.headers()[header::LOCATION], "/login");
    assert!(fixture.upstream.pages.lock().unwrap().is_empty());
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());

    let details = fixture.details("/article").await;
    assert_eq!(details["title"], TITLE);
    assert_eq!(details["description"], DESCRIPTION);
    assert_eq!(details["tags"], json!(["cycling"]));
    assert_eq!(fixture.saved_counts().await, (0, 0, 0, 0));
    assert!(!fixture.get("/all").await.contains(&fixture.origin));
    {
        let pages = fixture.upstream.pages.lock().unwrap();
        assert_eq!(
            pages
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["article", "final"]
        );
        assert!(pages.iter().all(
            |(_, h)| !h.contains_key(header::COOKIE) && !h.contains_key(header::AUTHORIZATION)
        ));
        let provider = fixture.upstream.provider.lock().unwrap();
        let [(headers, body)] = provider.as_slice() else {
            panic!("expected one provider request")
        };
        assert_eq!(headers[header::AUTHORIZATION], "Bearer fixture-key");
        assert_eq!(
            body["input"],
            json!({"excerpt": DESCRIPTION, "candidate_tags": []})
        );
        assert_eq!(body["store"], false);
        assert!(!body.to_string().contains(&fixture.origin));
    }

    fixture
        .save(
            &url,
            "My edited title",
            "My edited description",
            "custom-tag",
        )
        .await;
    let links = fixture.get("/all").await;
    assert!(links.contains(&format!("href=\"{url}\"")));
    assert!(links.contains("My edited title"));
    assert!(links.contains("My edited description"));
    let tags = fixture.get("/tags").await;
    assert!(tags.contains("custom-tag"));
    assert!(!tags.contains("cycling"));
    assert!(links.contains("Never viewed"));
    assert!(!links.contains(TITLE));
    assert!(!links.contains(DESCRIPTION));
}

#[tokio::test]
async fn requests_only_the_fields_the_reader_can_still_use() {
    let fixture = Fixture::new().await;
    fixture
        .save("https://example.org/", "Example", "", "personal-label")
        .await;
    let url = format!("{}/final", fixture.origin);
    for (title, description, tags) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let response = fixture
            .suggest_fields(
                &url,
                true,
                json!({"title": title, "description": description, "tags": tags}),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let details: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(details["title"], json!(title.then_some(TITLE)));
        assert_eq!(
            details["description"],
            json!(description.then_some(DESCRIPTION))
        );
        assert_eq!(
            details["tags"],
            if tags { json!(["cycling"]) } else { json!([]) }
        );
        assert!(details["warning"].is_null());
        if !description && !tags {
            assert_eq!(
                fixture.upstream.pages.lock().unwrap().len(),
                usize::from(title)
            );
            assert!(fixture.upstream.provider.lock().unwrap().is_empty());
        }
    }
    let requests = fixture.upstream.provider.lock().unwrap();
    let [(_, note), (_, tags)] = requests.as_slice() else {
        panic!("expected one note-only and one tag-only request");
    };
    assert_eq!(note["input"], json!({"excerpt": DESCRIPTION}));
    assert!(!note.to_string().contains("personal-label"));
    assert_eq!(
        note["text"]["format"]["schema"]["required"],
        json!(["description"])
    );
    assert_eq!(tags["input"]["candidate_tags"], json!(["personal-label"]));
    assert_eq!(
        tags["text"]["format"]["schema"]["required"],
        json!(["tags"])
    );
}

#[tokio::test]
async fn only_the_requesting_readers_labels_reach_the_provider() {
    let mut fixture = Fixture::new().await;
    fixture
        .save(
            "https://first-private.example/one",
            "First secret title",
            "First secret notes",
            "ai",
        )
        .await;
    let second = Fixture::reader_cookie(&fixture.db, "43").await;
    let first = std::mem::replace(&mut fixture.cookie, second);
    assert_eq!(
        fixture.details("/ai").await["tags"],
        json!(["artificial intelligence"])
    );
    fixture
        .save(
            "https://second-private.example/two",
            "Second secret title",
            "Second secret notes",
            "botany",
        )
        .await;
    let before = fixture.saved_counts().await;
    fixture.details("/ai").await;
    fixture.cookie = first;
    assert_eq!(fixture.details("/ai").await["tags"], json!(["ai"]));
    assert_eq!(fixture.saved_counts().await, before);
    let requests = fixture.upstream.provider.lock().unwrap();
    assert_eq!(requests.len(), 3);
    for ((_, body), labels) in requests
        .iter()
        .zip([json!([]), json!(["botany"]), json!(["ai"])])
    {
        assert_eq!(
            body["input"],
            json!({"excerpt": "Artificial intelligence research.", "candidate_tags": labels})
        );
        let request = body.to_string();
        for private in [
            "first-private.example",
            "second-private.example",
            "First secret",
            "Second secret",
        ] {
            assert!(!request.contains(private));
        }
    }
}

#[tokio::test]
async fn malformed_provider_fields_do_not_discard_usable_siblings() {
    let fixture = Fixture::new().await;
    for kind in ["shape", "delimiters"] {
        let details = fixture.details(&format!("/invalid/{kind}")).await;
        assert_eq!(details["title"], "Usable title");
        assert_eq!(details["description"], DESCRIPTION);
        assert_eq!(details["tags"], json!([]));
    }
    let details = fixture.details("/invalid/description").await;
    assert_eq!(details["title"], "Usable title");
    assert!(details["description"].is_null());
    assert_eq!(details["tags"], json!(["cycling"]));

    let missing = fixture.details("/invalid/missing-note").await;
    assert_eq!(missing["title"], "Usable title");
    assert_eq!(missing["tags"], json!(["cycling"]));
    assert!(missing["warning"].is_string());
    assert_eq!(missing["unavailable"], json!(["description"]));
    let empty = fixture.details("/invalid/null-note").await;
    assert!(empty["description"].is_null());
    assert_eq!(empty["tags"], json!(["cycling"]));
    assert!(empty["warning"].is_null());
    assert!(empty["unavailable"].as_array().is_none_or(Vec::is_empty));
}

#[tokio::test]
async fn non_public_targets_and_unusable_page_bodies_do_not_reach_openai() {
    let fixture = Fixture::new().await;
    let private = fixture.origin.replace("public.example", "127.0.0.1");
    let credentials = fixture.origin.replace("http://", "http://user:password@");
    assert!(
        tokio::net::lookup_host(("localhost", 0))
            .await
            .unwrap()
            .any(|addr| addr.ip() == std::net::Ipv4Addr::LOCALHOST)
    );
    for url in [
        format!("{private}/private"),
        format!("{credentials}/article"),
        format!("{}/private-redirect", fixture.origin),
        format!("{}/private-dns-redirect", fixture.origin),
        format!("{}/too-large", fixture.origin),
    ] {
        let response = fixture.suggest(&url, true).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY, "{url}");
    }
    assert!(fixture.upstream.pages.lock().unwrap().is_empty());
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());
}

#[tokio::test]
async fn explains_page_failures_without_logging_the_link_or_its_contents() {
    use tracing::instrument::WithSubscriber;
    use tracing_subscriber::fmt::{MakeWriter, writer::MutexGuardWriter};

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl<'a> MakeWriter<'a> for Capture {
        type Writer = MutexGuardWriter<'a, Vec<u8>>;

        fn make_writer(&'a self) -> Self::Writer {
            self.0.make_writer()
        }
    }

    let fixture = Fixture::new().await;
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(capture.clone())
        .finish();
    async {
        for (path, explanation) in [
            ("/refused?access=private-query", "denied access"),
            ("/private-dns-redirect", "public"),
            ("/large-catalogue", "complex"),
        ] {
            let url = format!("{}{path}", fixture.origin);
            let response = fixture.suggest(&url, true).await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            let body = to_bytes(response.into_body(), 1024).await.unwrap();
            assert!(std::str::from_utf8(&body).unwrap().contains(explanation));
        }
    }
    .with_subscriber(subscriber)
    .await;
    let bytes = capture.0.lock().unwrap();
    let logs = std::str::from_utf8(&bytes).unwrap();
    for reason in ["http_status", "restricted_destination", "dom_items"] {
        assert!(logs.contains(reason), "{logs}");
    }
    assert!(logs.contains("upstream_status=403"), "{logs}");
    for private in [
        "public.example",
        "/refused",
        "private-query",
        "private refusal body",
    ] {
        assert!(!logs.contains(private), "{logs}");
    }
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());
}

#[tokio::test]
async fn returns_usable_details_even_when_the_other_field_cannot_be_suggested() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/title-only").await;
    assert_eq!(details["title"], "Just a title");
    assert!(details["description"].is_null());
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());

    let details = fixture.details("/no-title").await;
    assert!(details["title"].is_null());
    assert_eq!(details["description"], DESCRIPTION);

    let details = fixture.details("/malformed-reply").await;
    assert_eq!(details["title"], "Usable title");
    assert!(details["description"].is_null());
    assert_eq!(details["tags"], json!([]));
    assert!(details["warning"].as_str().is_some_and(|s| !s.is_empty()));
}

#[tokio::test]
async fn preserves_declared_title_and_excerpt_encoding() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/legacy-charset").await;
    assert_eq!(details["title"], "Café");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "About cafés"
    );
}

#[tokio::test]
async fn decodes_titles_and_provider_text_using_the_leading_xml_declaration() {
    let fixture = Fixture::new().await;
    for kind in [
        "legacy",
        "unrelated-meta",
        "bom",
        "malformed",
        "ascii-utf16",
        "utf16le",
        "utf16be",
    ] {
        let details = fixture.details(&format!("/xml-charset/{kind}")).await;
        assert_eq!(details["title"], "Café", "{kind}");
        assert_eq!(
            fixture.upstream.provider.lock().unwrap().pop().unwrap().1["input"]["excerpt"],
            "About cafés",
            "{kind}"
        );
    }
}

#[tokio::test]
async fn recovers_prices_from_undeclared_legacy_html() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/undeclared-legacy").await;
    assert_eq!(details["title"], "Tickets £10");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "Entry costs €12."
    );
}

#[tokio::test]
async fn accepts_http_parameter_tokens_without_sniffing_non_html() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/http-token-charset").await;
    assert_eq!(details["title"], "Café");

    let plain = fixture
        .suggest(&format!("{}/plain-charset", fixture.origin), true)
        .await;
    assert_eq!(plain.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let requests = fixture.upstream.provider.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].1["input"]["excerpt"], "About cafés");
}

#[tokio::test]
async fn uses_the_document_title_even_when_declared_in_the_body() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/streamed-title").await;
    assert_eq!(details["title"], "Streamed article title");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "How to repair a bicycle chain."
    );
}

#[tokio::test]
async fn sends_bounded_evidence_from_prose_without_spaces() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/long-passage").await;
    assert_eq!(details["description"], DESCRIPTION);
    let provider = fixture.upstream.provider.lock().unwrap();
    assert_eq!(
        provider[0].1["input"]["excerpt"],
        "自転車の修理。".repeat(285) + "自転車の…"
    );
}

#[tokio::test]
async fn reads_charset_declared_in_html_without_transport_charset() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/meta-charset").await;
    assert_eq!(details["title"], "Café");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "About cafés"
    );
}

#[tokio::test]
async fn reads_whitespace_and_quotes_in_legacy_html_charset_pragma() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/pragma-charset").await;
    assert_eq!(details["title"], "Café");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "About cafés"
    );
}

#[tokio::test]
async fn rejects_compressed_bytes_without_sending_them_as_text() {
    let fixture = Fixture::new().await;
    let response = fixture
        .suggest(&format!("{}/compressed", fixture.origin), true)
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());
}

#[tokio::test]
async fn accepts_chunk_framing_but_rejects_additional_transfer_codings() {
    let fixture = Fixture::new().await;
    let response = fixture
        .suggest(&format!("{}/gzip-transfer", fixture.origin), true)
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());

    let details = fixture.details("/chunked").await;
    assert_eq!(details["title"], "Garden");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "How to plant herbs."
    );
}

#[tokio::test]
async fn rejects_xhtml_instead_of_treating_xml_as_html() {
    let fixture = Fixture::new().await;
    let response = fixture
        .suggest(&format!("{}/xhtml", fixture.origin), true)
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());
}

#[tokio::test]
async fn preserves_inline_words_and_separates_blocks_in_the_excerpt() {
    let fixture = Fixture::new().await;
    fixture.details("/inline-text").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        concat!(
            "Version 1.2 is supported. See the guide. Read it first. Ingredients Flour ",
            "More resources Find a guide Recipes. Memory capacities (GB) 16 32 ",
            "Upgrade: 16 32 GB. Ages 6–12. Keep code. Area: 10^(3) cm. Water: H_(2)O. ",
            "Sizes 16 32 and 8 16. Version 1.2. Price: 99 USD. Sale: 149 USD. Today: 299 USD. ",
            "Watch clips 1 2. Listen to code samples. Samples: 10 20 30. ",
            "Sizes: 16 32 GB. Registration closed. Specifications"
        )
    );
}

#[tokio::test]
async fn embeds_separate_values_without_fetching_resources_or_sending_their_fallback() {
    let fixture = Fixture::new().await;
    fixture.details("/embedded-resources").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "Memory: 16 32 GB. Range: 10 20 mm. Read the printed code. Sizes: 6 8 Wide Not yet paid"
    );
    assert!(fixture.upstream.pages.lock().unwrap().is_empty());
}

#[tokio::test]
async fn only_the_first_initially_open_disclosure_in_each_tree_and_group_sends_its_body() {
    let fixture = Fixture::new().await;
    fixture.details("/disclosure-groups").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "Pickup Standard Five days. Express Returns Free. Staff Local Today. Bag Box First."
    );
}

#[tokio::test]
async fn complex_pages_return_only_complete_head_metadata_without_a_second_fetch_or_model_call() {
    let fixture = Fixture::new().await;
    for (kind, title, description) in [
        ("both", Some("Plants & herbs"), Some("Grow A > B")),
        ("inert-template", Some("Plant guide"), Some("Soil notes")),
        ("clipped-title", None, Some("Plant guide")),
        ("clipped-meta", Some("Plant guide"), None),
    ] {
        let details = fixture
            .details(&format!("/structured-catalog/{kind}"))
            .await;
        assert_eq!(details["title"], json!(title), "{kind}");
        assert_eq!(details["description"], json!(description), "{kind}");
        assert_eq!(details["tags"], json!([]), "{kind}");
    }
    for kind in ["no-metadata", "decoys", "late"] {
        let url = format!("{}/structured-catalog/{kind}", fixture.origin);
        assert_eq!(
            fixture.suggest(&url, true).await.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{kind}"
        );
    }
    for (kind, title, note) in [
        ("ascii", "Plants & herbs", "Soil notes"),
        ("utf8", "Café", "Thé vert"),
        ("legacy", "Café", "Thé vert"),
    ] {
        let details = fixture
            .details(&format!("/complex-charset-prescan/{kind}"))
            .await;
        assert_eq!(details["title"], title, "{kind}");
        assert_eq!(details["description"], note, "{kind}");
        assert_eq!(details["tags"], json!([]), "{kind}");
    }
    let pages = fixture.upstream.pages.lock().unwrap();
    assert_eq!(
        pages
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>(),
        [
            "both",
            "inert-template",
            "clipped-title",
            "clipped-meta",
            "no-metadata",
            "decoys",
            "late",
            "ascii",
            "utf8",
            "legacy",
        ]
    );
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());
}

#[tokio::test]
async fn bounds_tag_attributes_without_treating_quoted_text_as_markup() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/catalogue-metadata").await;
    assert_eq!(details["title"], "Catalogue");
    assert!(details["description"].is_null());
    assert_eq!(details["tags"], json!([]));
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());

    let details = fixture.details("/catalogue-page").await;
    assert_eq!(details["title"], "Catalogue");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "The catalogue lists seasonal herbs."
    );
}

#[tokio::test]
async fn bounds_formatting_work_without_disabling_ordinary_formatted_pages() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/formatting-catalogue").await;
    assert_eq!(details["title"], "Catalogue");
    assert!(details["description"].is_null());
    assert_eq!(details["tags"], json!([]));
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());

    fixture.details("/inline-text").await;
    let provider = fixture.upstream.provider.lock().unwrap();
    assert_eq!(provider.len(), 1);
    assert!(
        provider[0].1["input"]["excerpt"]
            .as_str()
            .unwrap()
            .starts_with("Version 1.2 is supported.")
    );
}

#[tokio::test]
async fn reads_a_compact_document_with_many_adjacent_text_fragments() {
    let fixture = Fixture::new().await;
    let details = fixture.details("/field-notes").await;
    assert_eq!(details["title"], "Field notes");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "Seasonal planting notes."
    );
}

#[tokio::test]
async fn enforces_document_budget_before_calling_the_provider() {
    let fixture = Fixture::new().await;
    let response = fixture
        .suggest(&format!("{}/large-catalogue", fixture.origin), true)
        .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());

    let details = fixture.details("/catalogue-page").await;
    assert_eq!(details["title"], "Catalogue");
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "The catalogue lists seasonal herbs."
    );
}

#[tokio::test]
async fn hidden_document_regions_keep_the_title_without_sending_body_text() {
    let fixture = Fixture::new().await;
    for path in ["/hidden-root", "/hidden-body"] {
        let details = fixture.details(path).await;
        assert_eq!(details["title"], "Public title");
        assert!(details["description"].is_null());
    }
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sends_displayed_control_labels_as_separate_values() {
    let fixture = Fixture::new().await;
    fixture.details("/control-text").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        concat!(
            "Available capacities 16.5 32 Memory 64 GB 128 GB 256 GB Available widths (mm): 16 32 ",
            "Lengths: 16 or 32 Apply Clear 16 32 Choose capacity GB."
        )
    );
}

#[tokio::test]
async fn unsupported_math_does_not_turn_into_different_numbers() {
    let fixture = Fixture::new().await;
    fixture.details("/equation").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "Measurements Stir until smooth."
    );
}

#[tokio::test]
async fn omits_expressions_that_cannot_fit_in_the_excerpt() {
    let fixture = Fixture::new().await;
    fixture.details("/long-formula").await;
    let provider = fixture.upstream.provider.lock().unwrap();
    let evidence = provider[0].1["input"]["excerpt"].as_str().unwrap();
    assert!(evidence.len() <= 6000);
    assert!(evidence.ends_with(" Measured. Last value.…"));
}

#[tokio::test]
async fn clipped_scripts_omit_only_the_expression_they_attach_to() {
    let fixture = Fixture::new().await;
    for (path, ending) in [
        ("/clipped-attached", "Last:…"),
        ("/clipped-detached", "Last: H_(2 3)…"),
        ("/clipped-annotation", "Approved.…"),
    ] {
        fixture.details(path).await;
        assert_eq!(
            fixture.upstream.provider.lock().unwrap().pop().unwrap().1["input"]["excerpt"],
            "Measured. ".repeat(597) + ending,
            "{path}"
        );
    }
}

#[tokio::test]
async fn omitted_body_evidence_keeps_the_title_without_a_provider_call() {
    let fixture = Fixture::new().await;
    for path in [
        "/script-only",
        "/strike-only",
        "/media-only",
        "/gauges-only",
        "/shadow-slot-only",
        "/legacy-fallback-only",
        "/listbox-text-only",
        "/multiselect-counts-only",
    ] {
        let details = fixture.details(path).await;
        assert_eq!(details["title"], "Archive", "{path}");
        assert!(details["description"].is_null(), "{path}");
    }
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_cutoff_in_a_formula_preserves_unspaced_sentences() {
    let fixture = Fixture::new().await;
    fixture.details("/unspaced-formula").await;
    let provider = fixture.upstream.provider.lock().unwrap();
    assert_eq!(
        provider[0].1["input"]["excerpt"],
        "測定しました。".repeat(285) + "完了。…"
    );
}

#[tokio::test]
async fn an_empty_script_does_not_consume_the_excerpt_budget() {
    let fixture = Fixture::new().await;
    fixture.details("/empty-script-at-cutoff").await;
    let provider = fixture.upstream.provider.lock().unwrap();
    assert_eq!(
        provider[0].1["input"]["excerpt"],
        "Measured. ".repeat(599) + "Done!!!"
    );
}

#[tokio::test]
async fn sends_listbox_rows_and_ruby_base_text_without_changing_their_meaning() {
    let fixture = Fixture::new().await;
    fixture.details("/listbox-and-ruby").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "Stock Available Ready Year: 1446. 東京駅"
    );
}

#[tokio::test]
async fn collapsed_selects_supply_only_their_initially_selected_label() {
    let fixture = Fixture::new().await;
    fixture.details("/selected-options").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        concat!(
            "First: Paid Second: Overdue Default: Ready Picker: Current Group: On hold ",
            "List: Shown Stock: stock Basil Wrapped: Not paid In stock Basil"
        )
    );
}

#[tokio::test]
async fn multiselects_send_the_displayed_menu_label_or_listbox_rows() {
    let fixture = Fixture::new().await;
    fixture.details("/multiselect-sizes").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "Menu: Approved Rows: Draft Approved Picker: Not paid Zero: Early Late Invalid: Low High Image: available Overflow: Approved"
    );
}

#[tokio::test]
async fn cutoff_marker_does_not_cut_an_already_closed_exponent() {
    let fixture = Fixture::new().await;
    fixture.details("/formula-at-cutoff").await;
    let provider = fixture.upstream.provider.lock().unwrap();
    let evidence = provider[0].1["input"]["excerpt"].as_str().unwrap();
    assert!(evidence.len() <= 6000);
    assert!(evidence.ends_with(" Measured. Observation:…"));
}

#[tokio::test]
async fn marks_a_clipped_quantity_as_incomplete() {
    let fixture = Fixture::new().await;
    fixture.details("/long-quantity").await;
    let provider = fixture.upstream.provider.lock().unwrap();
    let evidence = provider[0].1["input"]["excerpt"].as_str().unwrap();
    assert!(evidence.len() <= 6000);
    assert!(evidence.ends_with(" Measured. Amount:…"));
}

#[tokio::test]
async fn closed_native_panels_do_not_reach_the_provider() {
    let fixture = Fixture::new().await;
    for path in ["/closed-dialog", "/closed-popover", "/closed-datalist"] {
        let closed = fixture.details(path).await;
        assert_eq!(closed["title"], "Order status");
        assert!(closed["description"].is_null());
    }
    assert!(fixture.upstream.provider.lock().unwrap().is_empty());

    fixture.details("/open-dialog").await;
    assert_eq!(
        fixture.upstream.provider.lock().unwrap()[0].1["input"]["excerpt"],
        "Order ready."
    );
}
