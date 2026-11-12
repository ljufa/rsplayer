//! One response model for both Subsonic wire formats.
//!
//! A [`Node`] is an element with attributes, an optional text value and
//! child elements. XML writes attributes as attributes and the text as
//! element content; JSON writes attributes as fields, the text as `value`,
//! a single child as an object and a [`Child::List`] as an array (also when
//! it has one element or none, which is what clients expect).

use axum::{
    body::Body,
    http::{header, StatusCode},
    response::Response,
};
use serde_json::{Map, Value as Json};

use api_models::serde_json;

/// Subsonic REST API version this server speaks.
pub const API_VERSION: &str = "1.16.1";
const SERVER_TYPE: &str = "rsplayer";
const XML_NAMESPACE: &str = "http://subsonic.org/restapi";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Self::Str(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}
impl From<&String> for Value {
    fn from(v: &String) -> Self {
        Self::Str(v.clone())
    }
}
impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Self::Int(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Self::Int(i64::from(v))
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Self::Int(i64::from(v))
    }
}
impl From<u64> for Value {
    fn from(v: u64) -> Self {
        Self::Int(i64::try_from(v).unwrap_or(i64::MAX))
    }
}
impl From<usize> for Value {
    fn from(v: usize) -> Self {
        Self::Int(i64::try_from(v).unwrap_or(i64::MAX))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Child {
    One(Node),
    /// Element name and the elements, serialized as a JSON array.
    List(&'static str, Vec<Node>),
    /// Plain values: a JSON array of scalars, in XML one text element each.
    Values(&'static str, Vec<Value>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: &'static str,
    pub attrs: Vec<(&'static str, Value)>,
    pub text: Option<String>,
    pub children: Vec<Child>,
}

impl Node {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            attrs: Vec::new(),
            text: None,
            children: Vec::new(),
        }
    }

    /// A node without an element of its own: its children go straight into
    /// the response (`getOpenSubsonicExtensions` answers with a bare list).
    pub const fn fragment() -> Self {
        Self::new("")
    }

    #[must_use]
    pub fn attr(mut self, key: &'static str, value: impl Into<Value>) -> Self {
        self.attrs.push((key, value.into()));
        self
    }

    /// Adds the attribute only when there is a value.
    #[must_use]
    pub fn opt<V: Into<Value>>(mut self, key: &'static str, value: Option<V>) -> Self {
        if let Some(v) = value {
            self.attrs.push((key, v.into()));
        }
        self
    }

    #[must_use]
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    #[must_use]
    pub fn child(mut self, node: Self) -> Self {
        self.children.push(Child::One(node));
        self
    }

    /// Children named `name`, a JSON array even when empty.
    #[must_use]
    pub fn list(mut self, name: &'static str, nodes: Vec<Self>) -> Self {
        self.children.push(Child::List(name, nodes));
        self
    }

    /// Scalar children named `name`, a JSON array of values.
    #[must_use]
    pub fn values(mut self, name: &'static str, values: Vec<Value>) -> Self {
        self.children.push(Child::Values(name, values));
        self
    }
}

/// A failed request: a Subsonic error code and message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub code: u32,
    pub message: String,
}

impl ApiError {
    pub fn generic(message: impl Into<String>) -> Self {
        Self {
            code: 0,
            message: message.into(),
        }
    }
    pub fn missing(param: &str) -> Self {
        Self {
            code: 10,
            message: format!("Required parameter is missing: {param}"),
        }
    }
    pub fn wrong_credentials() -> Self {
        Self {
            code: 40,
            message: "Wrong username or password".to_string(),
        }
    }
    pub fn not_found(what: &str) -> Self {
        Self {
            code: 70,
            message: format!("{what} not found"),
        }
    }
}

pub type ApiResult = Result<Option<Node>, ApiError>;

/// Response format asked for with `f`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Format {
    Xml,
    Json,
    /// JSON wrapped in a call of this (validated) function name.
    Jsonp(String),
}

impl Format {
    /// `f=json`, `f=jsonp` (with a usable `callback`), anything else XML,
    /// the spec default.
    pub fn from_params(f: Option<&str>, callback: Option<&str>) -> Self {
        match f {
            Some("json") => Self::Json,
            Some("jsonp") => callback
                .filter(|c| is_js_identifier(c))
                .map_or(Self::Json, |cb| Self::Jsonp(cb.to_string())),
            _ => Self::Xml,
        }
    }
}

/// Letters, digits, `_`, `$` and `.` only: the callback is echoed into a
/// script, so nothing that could end the call is let through.
fn is_js_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '.'))
        && !s.starts_with(|c: char| c.is_ascii_digit())
}

fn envelope(result: &ApiResult) -> Node {
    let ok = result.is_ok();
    let mut root = Node::new("subsonic-response")
        .attr("status", if ok { "ok" } else { "failed" })
        .attr("version", API_VERSION)
        .attr("type", SERVER_TYPE)
        .attr("serverVersion", env!("CARGO_PKG_VERSION"))
        .attr("openSubsonic", true);
    match result {
        Ok(Some(node)) if node.name.is_empty() => root.children.extend(node.children.iter().cloned()),
        Ok(Some(node)) => root = root.child(node.clone()),
        Ok(None) => {}
        Err(e) => {
            root = root.child(Node::new("error").attr("code", e.code).attr("message", e.message.as_str()));
        }
    }
    root
}

/// The body for `result` in `format`.
pub fn render_body(format: &Format, result: &ApiResult) -> (String, &'static str) {
    let root = envelope(result);
    match format {
        Format::Xml => {
            let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
            write_xml(&root, Some(XML_NAMESPACE), &mut out);
            (out, "text/xml; charset=utf-8")
        }
        Format::Json => (json_document(&root), "application/json"),
        Format::Jsonp(callback) => (format!("{callback}({});", json_document(&root)), "application/javascript"),
    }
}

/// The HTTP response for `result`. Subsonic errors are HTTP 200 too, the
/// status is in the body.
pub fn render(format: &Format, result: &ApiResult) -> Response {
    let (body, content_type) = render_body(format, result);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

fn json_document(root: &Node) -> String {
    let mut doc = Map::new();
    doc.insert(root.name.to_string(), to_json(root));
    Json::Object(doc).to_string()
}

fn json_value(value: &Value) -> Json {
    match value {
        Value::Str(s) => Json::String(s.clone()),
        Value::Int(i) => Json::from(*i),
        Value::Bool(b) => Json::Bool(*b),
    }
}

fn to_json(node: &Node) -> Json {
    let mut map = Map::new();
    for (key, value) in &node.attrs {
        map.insert((*key).to_string(), json_value(value));
    }
    if let Some(text) = &node.text {
        map.insert("value".to_string(), Json::String(text.clone()));
    }
    for child in &node.children {
        match child {
            Child::One(n) => {
                map.insert(n.name.to_string(), to_json(n));
            }
            Child::List(name, nodes) => {
                map.insert((*name).to_string(), Json::Array(nodes.iter().map(to_json).collect()));
            }
            Child::Values(name, values) => {
                map.insert((*name).to_string(), Json::Array(values.iter().map(json_value).collect()));
            }
        }
    }
    Json::Object(map)
}

fn write_xml(node: &Node, namespace: Option<&str>, out: &mut String) {
    out.push('<');
    out.push_str(node.name);
    if let Some(ns) = namespace {
        out.push_str(" xmlns=\"");
        out.push_str(ns);
        out.push('"');
    }
    for (key, value) in &node.attrs {
        out.push(' ');
        out.push_str(key);
        out.push_str("=\"");
        write_xml_value(value, out);
        out.push('"');
    }
    let has_children = node.children.iter().any(|c| match c {
        Child::One(_) => true,
        Child::List(_, nodes) => !nodes.is_empty(),
        Child::Values(_, values) => !values.is_empty(),
    });
    if node.text.is_none() && !has_children {
        out.push_str("/>");
        return;
    }
    out.push('>');
    if let Some(text) = &node.text {
        escape_xml(text, out);
    }
    for child in &node.children {
        match child {
            Child::One(n) => write_xml(n, None, out),
            Child::List(_, nodes) => nodes.iter().for_each(|n| write_xml(n, None, out)),
            Child::Values(name, values) => {
                for value in values {
                    out.push('<');
                    out.push_str(name);
                    out.push('>');
                    write_xml_value(value, out);
                    out.push_str("</");
                    out.push_str(name);
                    out.push('>');
                }
            }
        }
    }
    out.push_str("</");
    out.push_str(node.name);
    out.push('>');
}

fn write_xml_value(value: &Value, out: &mut String) {
    match value {
        Value::Str(s) => escape_xml(s, out),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
    }
}

/// Escapes for attribute values and text alike; drops characters XML 1.0
/// does not allow at all.
fn escape_xml(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    #[test]
    fn ping_as_xml() {
        let (body, ct) = render_body(&Format::Xml, &Ok(None));
        assert_eq!(ct, "text/xml; charset=utf-8");
        assert_eq!(
            body,
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><subsonic-response xmlns=\"http://subsonic.org/restapi\" \
                 status=\"ok\" version=\"1.16.1\" type=\"rsplayer\" serverVersion=\"{}\" openSubsonic=\"true\"/>",
                version()
            )
        );
    }

    #[test]
    fn ping_as_json() {
        let (body, ct) = render_body(&Format::Json, &Ok(None));
        assert_eq!(ct, "application/json");
        let v: Json = serde_json::from_str(&body).unwrap();
        let r = &v["subsonic-response"];
        assert_eq!(r["status"], "ok");
        assert_eq!(r["version"], API_VERSION);
        assert_eq!(r["type"], "rsplayer");
        assert_eq!(r["serverVersion"], version());
        assert_eq!(r["openSubsonic"], true);
    }

    #[test]
    fn error_in_both_formats() {
        let result = Err(ApiError::wrong_credentials());
        let (xml, _) = render_body(&Format::Xml, &result);
        assert!(xml.contains("status=\"failed\""), "{xml}");
        assert!(
            xml.contains("<error code=\"40\" message=\"Wrong username or password\"/></subsonic-response>"),
            "{xml}"
        );
        let (json, _) = render_body(&Format::Json, &result);
        let v: Json = serde_json::from_str(&json).unwrap();
        assert_eq!(v["subsonic-response"]["status"], "failed");
        assert_eq!(v["subsonic-response"]["error"]["code"], 40);
    }

    #[test]
    fn lists_are_arrays_in_json_and_repeated_elements_in_xml() {
        let album = Node::new("album")
            .attr("id", "al-1")
            .attr("name", "A & B <live>")
            .list("song", vec![Node::new("song").attr("id", "so-1").attr("track", 1)]);
        let result = Ok(Some(album));
        let (json, _) = render_body(&Format::Json, &result);
        let v: Json = serde_json::from_str(&json).unwrap();
        let a = &v["subsonic-response"]["album"];
        assert_eq!(a["name"], "A & B <live>");
        assert!(a["song"].is_array());
        assert_eq!(a["song"][0]["track"], 1);
        let (xml, _) = render_body(&Format::Xml, &result);
        assert!(
            xml.contains("<album id=\"al-1\" name=\"A &amp; B &lt;live&gt;\"><song id=\"so-1\" track=\"1\"/></album>"),
            "{xml}"
        );
    }

    #[test]
    fn text_value() {
        let node = Node::new("genres").list("genre", vec![Node::new("genre").attr("songCount", 2).text("Rock")]);
        let (xml, _) = render_body(&Format::Xml, &Ok(Some(node.clone())));
        assert!(xml.contains("<genres><genre songCount=\"2\">Rock</genre></genres>"), "{xml}");
        let (json, _) = render_body(&Format::Json, &Ok(Some(node)));
        let v: Json = serde_json::from_str(&json).unwrap();
        assert_eq!(v["subsonic-response"]["genres"]["genre"][0]["value"], "Rock");
    }

    #[test]
    fn scalar_lists() {
        let node = Node::new("openSubsonicExtensions")
            .attr("name", "formPost")
            .values("versions", vec![Value::Int(1)]);
        let (xml, _) = render_body(&Format::Xml, &Ok(Some(node.clone())));
        assert!(
            xml.contains("<openSubsonicExtensions name=\"formPost\"><versions>1</versions></openSubsonicExtensions>"),
            "{xml}"
        );
        let (json, _) = render_body(&Format::Json, &Ok(Some(node)));
        let v: Json = serde_json::from_str(&json).unwrap();
        assert_eq!(v["subsonic-response"]["openSubsonicExtensions"]["versions"][0], 1);
    }

    #[test]
    fn fragment_children_go_into_the_response() {
        let node = Node::fragment().list(
            "openSubsonicExtensions",
            vec![Node::new("openSubsonicExtensions").attr("name", "formPost")],
        );
        let (json, _) = render_body(&Format::Json, &Ok(Some(node)));
        let v: Json = serde_json::from_str(&json).unwrap();
        assert_eq!(v["subsonic-response"]["openSubsonicExtensions"][0]["name"], "formPost");
    }

    #[test]
    fn jsonp_needs_a_safe_callback() {
        assert_eq!(Format::from_params(Some("jsonp"), Some("cb_1")), Format::Jsonp("cb_1".to_string()));
        assert_eq!(Format::from_params(Some("jsonp"), Some("alert(1)//")), Format::Json);
        assert_eq!(Format::from_params(None, None), Format::Xml);
        let (body, ct) = render_body(&Format::Jsonp("cb".to_string()), &Ok(None));
        assert_eq!(ct, "application/javascript");
        assert!(body.starts_with("cb({\"subsonic-response\""));
        assert!(body.ends_with(");"));
    }
}
