//! A tiny JSON reader — just enough for a release feed.
//!
//! [`crate::theme::Theme::from_json`] reads a *flat* object of strings, which is
//! all a theme file ever is. A release payload is nested (`assets` is an array
//! of objects), so it needs a real value tree. This is that and no more: no
//! serializer, no numeric conversions nothing reads, and still no dependency.
//! Keeping it apart from the theme parser means neither grows cases the other
//! will never see.

/// A parsed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
  Null,
  Bool(bool),
  /// The number exactly as written. Release payloads carry byte sizes, and
  /// routing a `u64` through `f64` rounds the large ones — so the text is kept
  /// and parsed by whichever accessor knows what it wants.
  Num(String),
  Str(String),
  Arr(Vec<Json>),
  Obj(Vec<(String, Json)>),
}

impl Json {
  /// The value at `key`, for an object. `None` for every other kind, so a
  /// missing field and a wrong-shaped document read the same at the call site.
  pub(crate) fn get(&self, key: &str) -> Option<&Json> {
    match self {
      Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
      _ => None,
    }
  }

  pub(crate) fn as_str(&self) -> Option<&str> {
    match self {
      Json::Str(s) => Some(s),
      _ => None,
    }
  }

  pub(crate) fn as_u64(&self) -> Option<u64> {
    match self {
      Json::Num(text) => text.parse().ok(),
      _ => None,
    }
  }

  pub(crate) fn as_array(&self) -> Option<&[Json]> {
    match self {
      Json::Arr(items) => Some(items),
      _ => None,
    }
  }
}

/// Nesting cap. Without one, a feed of nothing but `[` recurses the parser as
/// deep as it likes and takes the process down with the stack — a remote input
/// crashing the app on a background update check.
const MAX_DEPTH: usize = 32;

/// Parse a whole JSON document. Trailing content is an error rather than being
/// ignored: a truncated or doubled response should fail the check, not
/// half-succeed.
pub(crate) fn parse(bytes: &[u8]) -> Result<Json, String> {
  let mut p = Parser { bytes, at: 0 };
  p.space();
  let value = p.value(0)?;
  p.space();
  if p.at != p.bytes.len() {
    return Err(format!("json: trailing input at byte {}", p.at));
  }
  Ok(value)
}

struct Parser<'a> {
  bytes: &'a [u8],
  at: usize,
}

impl<'a> Parser<'a> {
  fn peek(&self) -> Option<u8> {
    self.bytes.get(self.at).copied()
  }

  fn bump(&mut self) -> Result<u8, String> {
    let byte = self
      .peek()
      .ok_or_else(|| "json: unexpected end of input".to_string())?;
    self.at += 1;
    Ok(byte)
  }

  fn space(&mut self) {
    while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
      self.at += 1;
    }
  }

  fn expect(&mut self, want: u8) -> Result<(), String> {
    let got = self.bump()?;
    if got == want {
      Ok(())
    } else {
      Err(format!(
        "json: expected `{}` at byte {}",
        want as char,
        self.at - 1
      ))
    }
  }

  fn value(&mut self, depth: usize) -> Result<Json, String> {
    if depth > MAX_DEPTH {
      return Err(format!("json: nested deeper than {MAX_DEPTH}"));
    }
    match self
      .peek()
      .ok_or_else(|| "json: unexpected end of input".to_string())?
    {
      b'{' => self.object(depth),
      b'[' => self.array(depth),
      b'"' => self.string().map(Json::Str),
      b't' => self.literal("true", Json::Bool(true)),
      b'f' => self.literal("false", Json::Bool(false)),
      b'n' => self.literal("null", Json::Null),
      b'-' | b'0'..=b'9' => self.number(),
      other => Err(format!(
        "json: unexpected `{}` at byte {}",
        other as char, self.at
      )),
    }
  }

  fn object(&mut self, depth: usize) -> Result<Json, String> {
    self.expect(b'{')?;
    let mut fields = Vec::new();
    self.space();
    if self.peek() == Some(b'}') {
      self.at += 1;
      return Ok(Json::Obj(fields));
    }
    loop {
      self.space();
      let key = self.string()?;
      self.space();
      self.expect(b':')?;
      self.space();
      let value = self.value(depth + 1)?;
      fields.push((key, value));
      self.space();
      match self.bump()? {
        b',' => continue,
        b'}' => return Ok(Json::Obj(fields)),
        _ => {
          return Err(format!(
            "json: expected `,` or `}}` at byte {}",
            self.at - 1
          ))
        }
      }
    }
  }

  fn array(&mut self, depth: usize) -> Result<Json, String> {
    self.expect(b'[')?;
    let mut items = Vec::new();
    self.space();
    if self.peek() == Some(b']') {
      self.at += 1;
      return Ok(Json::Arr(items));
    }
    loop {
      self.space();
      items.push(self.value(depth + 1)?);
      self.space();
      match self.bump()? {
        b',' => continue,
        b']' => return Ok(Json::Arr(items)),
        _ => return Err(format!("json: expected `,` or `]` at byte {}", self.at - 1)),
      }
    }
  }

  fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
    if self.bytes[self.at..].starts_with(word.as_bytes()) {
      self.at += word.len();
      Ok(value)
    } else {
      Err(format!("json: expected `{word}` at byte {}", self.at))
    }
  }

  /// Numbers are kept as text — see [`Json::Num`]. Only their *shape* is
  /// checked here, which is enough to find the end of the token.
  fn number(&mut self) -> Result<Json, String> {
    let start = self.at;
    if self.peek() == Some(b'-') {
      self.at += 1;
    }
    while matches!(self.peek(), Some(b'0'..=b'9')) {
      self.at += 1;
    }
    if self.peek() == Some(b'.') {
      self.at += 1;
      while matches!(self.peek(), Some(b'0'..=b'9')) {
        self.at += 1;
      }
    }
    if matches!(self.peek(), Some(b'e' | b'E')) {
      self.at += 1;
      if matches!(self.peek(), Some(b'+' | b'-')) {
        self.at += 1;
      }
      while matches!(self.peek(), Some(b'0'..=b'9')) {
        self.at += 1;
      }
    }
    let text = std::str::from_utf8(&self.bytes[start..self.at])
      .map_err(|_| format!("json: invalid number at byte {start}"))?;
    if text.is_empty() || text == "-" {
      return Err(format!("json: invalid number at byte {start}"));
    }
    Ok(Json::Num(text.to_string()))
  }

  fn string(&mut self) -> Result<String, String> {
    self.expect(b'"')?;
    let mut out = String::new();
    loop {
      let byte = self.bump()?;
      match byte {
        b'"' => return Ok(out),
        b'\\' => {
          let escape = self.bump()?;
          match escape {
            b'"' => out.push('"'),
            b'\\' => out.push('\\'),
            b'/' => out.push('/'),
            b'b' => out.push('\u{8}'),
            b'f' => out.push('\u{c}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'u' => out.push(self.escaped_char()?),
            other => {
              return Err(format!(
                "json: unknown escape `\\{}` at byte {}",
                other as char,
                self.at - 1
              ))
            }
          }
        }
        0x00..=0x1f => {
          return Err(format!(
            "json: control character in string at byte {}",
            self.at - 1
          ))
        }
        // Pass multi-byte UTF-8 through as written rather than decoding
        // it: the whole sequence is copied in one go, so a stray
        // continuation byte fails as invalid UTF-8 instead of being
        // silently mangled.
        lead => {
          let start = self.at - 1;
          let len = match lead {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => return Err(format!("json: invalid utf-8 at byte {start}")),
          };
          self.at = start + len;
          let text = self
            .bytes
            .get(start..self.at)
            .and_then(|s| std::str::from_utf8(s).ok())
            .ok_or_else(|| format!("json: invalid utf-8 at byte {start}"))?;
          out.push_str(text);
        }
      }
    }
  }

  /// A `\uXXXX` escape, joining a surrogate pair when it finds one. An
  /// unpaired surrogate is not a character, so it can only be an error or a
  /// replacement — erroring keeps a mangled feed from parsing as valid.
  fn escaped_char(&mut self) -> Result<char, String> {
    let first = self.hex4()?;
    let code = match first {
      0xd800..=0xdbff => {
        if self.peek() != Some(b'\\') {
          return Err(format!("json: unpaired surrogate at byte {}", self.at));
        }
        self.at += 1;
        self.expect(b'u')?;
        let low = self.hex4()?;
        if !(0xdc00..=0xdfff).contains(&low) {
          return Err(format!("json: unpaired surrogate at byte {}", self.at));
        }
        0x10000 + ((first - 0xd800) << 10) + (low - 0xdc00)
      }
      0xdc00..=0xdfff => return Err(format!("json: unpaired surrogate at byte {}", self.at)),
      other => other,
    };
    char::from_u32(code).ok_or_else(|| format!("json: bad escape at byte {}", self.at))
  }

  fn hex4(&mut self) -> Result<u32, String> {
    let start = self.at;
    let mut value = 0u32;
    for _ in 0..4 {
      let digit = (self.bump()? as char)
        .to_digit(16)
        .ok_or_else(|| format!("json: bad \\u escape at byte {start}"))?;
      value = value * 16 + digit;
    }
    Ok(value)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_the_shape_a_release_feed_has() {
    let body = br#"{
            "tag_name": "v1.2.3",
            "draft": false,
            "assets": [{"name": "app.dmg", "size": 87357960}]
        }"#;
    let value = parse(body).unwrap();
    assert_eq!(value.get("tag_name").unwrap().as_str(), Some("v1.2.3"));
    assert_eq!(value.get("draft"), Some(&Json::Bool(false)));
    let assets = value.get("assets").unwrap().as_array().unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].get("name").unwrap().as_str(), Some("app.dmg"));
    assert_eq!(assets[0].get("size").unwrap().as_u64(), Some(87_357_960));
  }

  /// Sizes are `u64` and must survive verbatim: a value past `f64`'s exact
  /// range would come back rounded if numbers were parsed as floats, and the
  /// download would then be reported as truncated.
  #[test]
  fn large_integers_keep_every_digit() {
    let value = parse(br#"{"size": 9007199254740993}"#).unwrap();
    assert_eq!(
      value.get("size").unwrap().as_u64(),
      Some(9_007_199_254_740_993)
    );
  }

  #[test]
  fn accessors_are_shape_tolerant() {
    let value = parse(br#"{"a": "text", "b": [1], "c": null}"#).unwrap();
    assert_eq!(value.get("missing"), None);
    assert_eq!(value.get("a").unwrap().as_u64(), None);
    assert_eq!(value.get("a").unwrap().as_array(), None);
    assert_eq!(value.get("b").unwrap().as_str(), None);
    // A non-object has no fields rather than being an error to ask.
    assert_eq!(value.get("b").unwrap().get("a"), None);
    assert_eq!(value.get("c").unwrap().as_str(), None);
  }

  #[test]
  fn empty_containers_parse() {
    assert_eq!(parse(b"{}").unwrap(), Json::Obj(Vec::new()));
    assert_eq!(parse(b"[]").unwrap(), Json::Arr(Vec::new()));
    assert_eq!(
      parse(br#"{"a": {}, "b": []}"#).unwrap().get("a"),
      Some(&Json::Obj(Vec::new()))
    );
  }

  #[test]
  fn escapes_decode() {
    let value = parse(r#"{"s": "a\"b\\c\/d\ne\tféA"}"#.as_bytes()).unwrap();
    assert_eq!(value.get("s").unwrap().as_str(), Some("a\"b\\c/d\ne\tféA"));
  }

  #[test]
  fn surrogate_pairs_become_one_character() {
    let value = parse(r#"{"s": "🚀"}"#.as_bytes()).unwrap();
    assert_eq!(value.get("s").unwrap().as_str(), Some("🚀"));
  }

  #[test]
  fn unpaired_surrogates_are_refused() {
    assert!(parse(br#"{"s": "\ud83d"}"#).is_err());
    assert!(parse(br#"{"s": "\ud83dx"}"#).is_err());
    assert!(parse(br#"{"s": "\ude80"}"#).is_err());
  }

  #[test]
  fn multibyte_text_passes_through() {
    let value = parse("{\"s\": \"héllo — 世界\"}".as_bytes()).unwrap();
    assert_eq!(value.get("s").unwrap().as_str(), Some("héllo — 世界"));
  }

  #[test]
  fn malformed_input_is_an_error_not_a_partial_value() {
    for body in [
      &b"not json"[..],
      b"{",
      b"{\"a\"}",
      b"{\"a\": }",
      b"[1,]",
      b"{\"a\": 1,}",
      // Trailing content: a doubled or truncated response.
      b"{} {}",
      b"{\"a\": 01x}",
      b"{\"a\": tru}",
      // A raw newline inside a string is a control character.
      b"{\"a\": \"x\ny\"}",
    ] {
      assert!(
        parse(body).is_err(),
        "{:?} parsed",
        String::from_utf8_lossy(body)
      );
    }
  }

  /// A hostile feed must not be able to recurse the parser into a stack
  /// overflow, which would crash the app from a background update check.
  #[test]
  fn deep_nesting_is_refused_rather_than_overflowing() {
    let deep = "[".repeat(MAX_DEPTH + 8);
    let err = parse(deep.as_bytes()).unwrap_err();
    assert!(err.contains("nested deeper"), "{err}");
  }
}
