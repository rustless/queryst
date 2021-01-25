extern crate serde;
extern crate serde_json;

#[cfg(feature = "regex1")]
extern crate regex;
extern crate percent_encoding;

#[cfg(feature = "regex1")]
#[macro_use]
extern crate lazy_static;

pub use parser::{parse, ParseResult, ParseError, ParseErrorKind};

mod merge;
mod helpers;
mod parser;

