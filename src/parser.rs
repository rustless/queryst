#[cfg(feature = "regex1")]
use regex::Regex;
use serde_json::{Value, Map, Number};
use percent_encoding::percent_decode;

use merge::merge;
use helpers::{create_array, push_item_to_array};

#[cfg(feature = "regex1")]
lazy_static! {
    static ref PARENT_REGEX: Regex = Regex::new(r"^([^\]\[]+)").unwrap();
    static ref CHILD_REGEX: Regex = Regex::new(r"(\[[^\]\[]*\])").unwrap();
}

type Object = Map<String, Value>;

#[derive(Debug)]
#[allow(missing_copy_implementations)]
pub enum ParseErrorKind {
    DecodingError,
    Other
}

#[derive(Debug)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub message: String
}

pub type ParseResult<T> = Result<T,ParseError>;

pub fn decode_component(source: &str) -> Result<String,String> {
    let result = percent_decode(source.as_bytes()).decode_utf8_lossy().to_string();
    return Ok(result);
}

fn parse_pair(part: &str) -> (&str, Option<&str>) {
    let separator = part.find("]=")
                        .and_then(|pos| Some(pos+1))
                        .or_else(|| part.find("="));
    match separator {
        None => return (part, None),
        Some(pos) => {
            let key = &part[..pos];
            let val = &part[(pos + 1)..];
            return (key, Some(val));
        }
    }
}

fn parse_pairs(body: &str) -> Vec<(&str, Option<&str>)> {
    let mut pairs = vec![];
    for part in body.split("&") {
        pairs.push(parse_pair(part));
    }
    return pairs
}

#[cfg(feature = "regex1")]
fn parse_key(key: &str) -> ParseResult<Vec<String>> {
    let mut keys: Vec<String> = vec![];

    match PARENT_REGEX.captures(key) {
        Some(captures) => {
            match decode_component(captures.get(1).unwrap().as_str()) {
                Ok(decoded_key) => keys.push(decoded_key),
                Err(err_msg) => return Err(ParseError{ kind: ParseErrorKind::DecodingError, message: err_msg })
            }
        }
        None => ()
    };

    for captures in CHILD_REGEX.captures_iter(key) {
        match decode_component(captures.get(1).unwrap().as_str()) {
            Ok(decoded_key) => keys.push(decoded_key),
            Err(err_msg) => return Err(ParseError{ kind: ParseErrorKind::DecodingError, message: err_msg })
        }
    }

    Ok(keys)
}

#[cfg(not(feature = "regex1"))]
fn parse_key(key: &str) -> ParseResult<Vec<String>> {
    let mut keys: Vec<String> = vec![];

    match key.split(|c| c=='[' || c==']').next() {
        Some(parent) if !parent.is_empty() =>  {
            match decode_component(parent) {
                Ok(decoded_key) => keys.push(decoded_key),
                Err(err_msg) => return Err(ParseError{ kind: ParseErrorKind::DecodingError, message: err_msg })
            }
        }
        _ => ()
    }

    let mut prev_bracket = None;
    for (idx, ch) in key.char_indices() {
        match ch {
            '[' => prev_bracket = Some(idx),
            ']' => {
                if let Some(prev_idx) = prev_bracket {
                    prev_bracket = None;
                    let child = &key[prev_idx..=idx];
                    match decode_component(child) {
                        Ok(decoded_key) => keys.push(decoded_key),
                        Err(err_msg) => return Err(ParseError{ kind: ParseErrorKind::DecodingError, message: err_msg })
                    }
                }
            }
            _ => (),
        }
    }

    Ok(keys)
}


fn cleanup_key(key: &str) -> &str {
    if key.starts_with("[") && key.ends_with("]") {
        &key[1..(key.len()-1)]
    } else {
        key
    }
}

fn create_idx_merger(idx: u64, obj: Value) -> Value {
    let mut tree = Object::new();
    tree.insert("__idx".to_string(), Value::Number(Number::from(idx)));
    tree.insert("__object".to_string(), obj);
    return Value::Object(tree)
}

fn create_object_with_key(key: String, obj: Value) -> Value {
    let mut tree = Object::new();
    tree.insert(key, obj);
    return Value::Object(tree)
}

fn apply_object(keys: &[String], val: Value) -> Value {

    if keys.len() > 0 {
        let key = keys.get(0).unwrap();
        if key == "[]" {
            let mut new_array = create_array();
            let item = apply_object(&keys[1..], val);
            push_item_to_array(&mut new_array, item);
            return new_array;
        } else {
            let key = cleanup_key(key);
            let array_index = key.parse();

            match array_index {
                Ok(idx) => {
                    let result = apply_object(&keys[1..], val);
                    let item = create_idx_merger(idx, result);
                    return item;
                },
                Err(_) => {
                    return create_object_with_key(key.to_string(), apply_object(&keys[1..], val));
                }
            }
        }

    } else {
        return val;
    }
}

pub fn parse(params: &str) -> ParseResult<Value> {
    let tree = Object::new();
    let mut obj = Value::Object(tree);
    let decoded_params = match decode_component(&params.replace("+", " ")) {
        Ok(val) => val,
        Err(err) => return Err(ParseError{ kind: ParseErrorKind::DecodingError, message: err })
    };
    let pairs = parse_pairs(&decoded_params);
    for &(key, value) in pairs.iter() {
        let parse_key_res = parse_key(key)?;
        let key_chain = &parse_key_res[0..];
        let decoded_value = match value {
            None => Value::default(),
            Some(val) => match decode_component(val) {
                Ok(decoded_value) => Value::String(decoded_value),
                Err(err) => return Err(ParseError{ kind: ParseErrorKind::DecodingError, message: err })
            }
        };
        let partial = apply_object(key_chain, decoded_value);
        merge(&mut obj, &partial);
    }

    Ok(obj)
}

#[cfg(test)]
mod tests {
    use parse;
    use super::parse_pair;
    use serde_json::{Value, to_string};

    fn eq_str(value: Value, string: &str) {
        assert_eq!(&to_string(&value).unwrap(), string)
    }

    #[test]
    fn test_parse_pair() {
        assert_eq!(parse_pair("foo=1"), ("foo", Some("1")));
        assert_eq!(parse_pair("empty="), ("empty", Some("")));
        assert_eq!(parse_pair("noval"), ("noval", None));
    }

    #[test]
    fn it_parses_simple_string() {
        eq_str(parse("0=foo").unwrap(), r#"{"0":"foo"}"#);
        eq_str(parse("a[<=>]==23").unwrap(), r#"{"a":{"<=>":"=23"}}"#);
        eq_str(parse(" foo = bar = baz ").unwrap(), r#"{" foo ":" bar = baz "}"#);
    }

    #[test]
    fn it_parses_nested_string() {
        eq_str(parse("a[b][c][d][e][f][g][h]=i").unwrap(),
            r#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":"i"}}}}}}}}"#);
    }

    #[test]
    fn it_parses_simple_array() {
        eq_str(parse("a=b&a=c&a=d&a=e").unwrap(),
            r#"{"a":["b","c","d","e"]}"#);
    }

    #[test]
    fn it_parses_explicit_array() {
        eq_str(parse("a[]=b&a[]=c&a[]=d").unwrap(),
            r#"{"a":["b","c","d"]}"#);
    }

    #[test]
    fn it_parses_nested_array() {
        eq_str(parse("a[b][]=c&a[b][]=d").unwrap(),
            r#"{"a":{"b":["c","d"]}}"#);
    }

    #[test]
    fn it_allows_to_specify_array_indexes() {
        eq_str(parse("a[0][]=c&a[1][]=d").unwrap(),
            r#"{"a":[["c"],["d"]]}"#);
    }

    #[test]
    fn it_transforms_arrays_to_object() {
        eq_str(parse("foo[0]=bar&foo[bad]=baz").unwrap(),
            r#"{"foo":{"0":"bar","bad":"baz"}}"#);

        eq_str(parse("foo[0][a]=a&foo[0][b]=b&foo[1][a]=aa&foo[1][b]=bb").unwrap(),
            r#"{"foo":[{"a":"a","b":"b"},{"a":"aa","b":"bb"}]}"#);
    }

    #[test]
    fn it_transforms_standalone_keys() {
        eq_str(parse("foo=bar&baz").unwrap(),
            r#"{"baz":null,"foo":"bar"}"#);
    }

    #[test]
    fn it_doesnt_produce_empty_keys() {
        eq_str(parse("_r=1&").unwrap(),
            r#"{"_r":"1"}"#);
    }

    #[test]
    fn it_supports_encoded_strings() {
        eq_str(parse("a[b%20c]=c%20d").unwrap(),
            r#"{"a":{"b c":"c d"}}"#);
    }

    #[test]
    fn it_parses_explicit_encoded_array() {
        eq_str(parse("a%5B%5D=b&a%5B%5D=c&a%5B%5D=d").unwrap(),
            r#"{"a":["b","c","d"]}"#);
    }
    #[test]
    fn it_parses_plus_sign() {
        eq_str(parse("a=b%20c+d%2B").unwrap(), r#"{"a":"b c d+"}"#);
     }
}
