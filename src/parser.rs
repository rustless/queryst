
use regex::Regex;
use collections::treemap::TreeMap;
use serialize::json;
use serialize::json::{Json};
use serialize::json::ToJson;
use url::percent_encoding::lossy_utf8_percent_decode;

use merge::merge;
use helpers::{create_array, push_item_to_array};

static PARENT_REGEX: Regex = regex!(r"^([^][]+)");
static CHILD_REGEX: Regex = regex!(r"(\[[^][]*\])");

#[deriving(Show)]
pub enum ParseErrorKind {
    DecodingError,
    Other
}

#[deriving(Show)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub message: String
}

pub type ParseResult<T> = Result<T,ParseError>;

pub fn decode_component(source: &str) -> Result<String,String> {
    return Ok(lossy_utf8_percent_decode(source.as_bytes()));
}

fn parse_pairs(body: &str) -> Vec<(&str, Option<&str>)> {

    let mut pairs = vec![];

    for part in body.split_str("&") {
        let separator = part.find_str("]=")
                            .and_then(|pos| Some(pos+1))
                            .or_else(|| part.find_str("="));

        match separator {
            None => pairs.push((part, None)),
            Some(pos) => {
                let key = part.slice_to(pos);
                let val = part.slice_from(pos + 1);
                pairs.push((key, Some(val)));
            }
        }
    }

    return pairs
}

fn parse_key(key: &str) -> ParseResult<Vec<String>> {
    let mut keys: Vec<String> = vec![];

    match PARENT_REGEX.captures(key) {
        Some(captures) => {
            match decode_component(captures.at(1)) {
                Ok(decoded_key) => keys.push(decoded_key),
                Err(err_msg) => return Err(ParseError{ kind: DecodingError, message: err_msg })
            }
        }
        None => ()
    };

    for captures in CHILD_REGEX.captures_iter(key) {
        match decode_component(captures.at(1)) {
            Ok(decoded_key) => keys.push(decoded_key),
            Err(err_msg) => return Err(ParseError{ kind: DecodingError, message: err_msg })
        }
    }

    Ok(keys)
}

fn cleanup_key(key: &str) -> &str {
    if key.starts_with("[") && key.ends_with("]") {
        key.slice_chars(1, key.len()-1)
    } else {
        key
    }
}

fn create_idx_merger(idx: uint, obj: Json) -> Json {
    let mut tree: TreeMap<String,Json> = TreeMap::new();
    tree.insert("__idx".to_string(), idx.to_json());
    tree.insert("__object".to_string(), obj);
    return json::Object(tree)
}

fn create_object_with_key(key: String, obj: Json) -> Json {
    let mut tree: TreeMap<String,Json> = TreeMap::new();
    tree.insert(key, obj);
    return json::Object(tree)
}

fn apply_object(keys: &[String], val: Json) -> Json {

    if keys.len() > 0 {
        let key = keys.get(0).unwrap();
        if key.as_slice() == "[]" {
            let mut new_array = create_array();
            let item = apply_object(keys.tail(), val);
            push_item_to_array(&mut new_array, item);
            return new_array;
        } else {
            let key = cleanup_key(key.as_slice());
            let array_index: Option<uint> = from_str(key);

            match array_index {
                Some(idx) => {
                    let result = apply_object(keys.tail(), val);
                    let item = create_idx_merger(idx, result);
                    return item;
                },
                None => {
                    return create_object_with_key(key.to_string(), apply_object(keys.tail(), val));
                }
            }
        }

    } else {
        return val;
    }
}

pub fn parse(params: &str) -> ParseResult<Json> {
    let tree: TreeMap<String,Json> = TreeMap::new();
    let mut obj = tree.to_json();
    let pairs = parse_pairs(params);
    for &(key, value) in pairs.iter() {
        let parse_key_res = try!(parse_key(key));
        let key_chain = parse_key_res.slice_from(0);
        let decoded_value = match decode_component(value.unwrap_or("")) {
            Ok(val) => val,
            Err(err) => return Err(ParseError{ kind: DecodingError, message: err })
        };
        let partial = apply_object(key_chain, decoded_value.to_json());
        merge(&mut obj, &partial);
    }

    Ok(obj)
}

#[test]
fn it_parses_simple_string() {
    assert_eq!(parse("0=foo").unwrap().to_string(), r#"{"0":"foo"}"#.to_string())
    assert_eq!(parse("a[<=>]==23").unwrap().to_string(), r#"{"a":{"<=>":"=23"}}"#.to_string())
    assert_eq!(parse(" foo = bar = baz ").unwrap().to_string(), r#"{" foo ":" bar = baz "}"#.to_string())
}

#[test]
fn it_parses_nested_string() {
    assert_eq!(parse("a[b][c][d][e][f][g][h]=i").unwrap().to_string(), 
        r#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":"i"}}}}}}}}"#.to_string())
}

#[test]
fn it_parses_simple_array() {
    assert_eq!(parse("a=b&a=c").unwrap().to_string(), 
        r#"{"a":["b","c"]}"#.to_string())
}

#[test]
fn it_parses_explicit_array() {
    assert_eq!(parse("a[]=b&a[]=c&a[]=d").unwrap().to_string(), 
        r#"{"a":["b","c","d"]}"#.to_string())
}

#[test]
fn it_parses_nested_array() {
    assert_eq!(parse("a[b][]=c&a[b][]=d").unwrap().to_string(), 
        r#"{"a":{"b":["c","d"]}}"#.to_string())
}

#[test]
fn it_allows_to_specify_array_indexes() {
    assert_eq!(parse("a[0][]=c&a[1][]=d").unwrap().to_string(), 
        r#"{"a":[["c"],["d"]]}"#.to_string())
}

#[test]
fn it_transforms_arrays_to_object() {
    assert_eq!(parse("foo[0]=bar&foo[bad]=baz").unwrap().to_string(), 
        r#"{"foo":{"0":"bar","bad":"baz"}}"#.to_string());

    assert_eq!(parse("foo[0][a]=a&foo[0][b]=b&foo[1][a]=aa&foo[1][b]=bb").unwrap().to_string(),
        r#"{"foo":[{"a":"a","b":"b"},{"a":"aa","b":"bb"}]}"#.to_string());
}

#[test]
fn it_doesnt_produce_empty_keys() {
    assert_eq!(parse("_r=1&").unwrap().to_string(),
        r#"{"_r":"1"}"#.to_string());
}

#[test]
fn it_supports_encoded_strings() {
    assert_eq!(parse("a[b%20c]=c%20d").unwrap().to_string(),
        r#"{"a":{"b c":"c d"}}"#.to_string());
}