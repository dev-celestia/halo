//! Parsed Postman export types and shared constants.

use super::*;

pub(crate) const MULTIPART_BOUNDARY: &str = "halo-postman-boundary";
pub(crate) const REQUESTS_FOLDER: &str = "Requests";

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PostmanImport {
    pub collections: Vec<ParsedCollection>,
    pub environments: Vec<ParsedEnvironment>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedCollection {
    pub name: String,
    pub folders: Vec<ParsedFolder>,
    /// Postman-style context JSON, present when the collection declares variables.
    pub variables_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedFolder {
    pub name: String,
    pub folders: Vec<ParsedFolder>,
    pub requests: Vec<ParsedRequest>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedRequest {
    pub name: String,
    pub method: String,
    pub url: String,
    /// JSON object string. Duplicate header names are joined with `", "`.
    pub headers_json: String,
    pub body: String,
    pub body_type: String,
    pub pre_script: String,
    pub test_script: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedEnvironment {
    pub name: String,
    pub variables_json: String,
}

pub(crate) struct Inherited<'a> {
    pub(crate) auth: Option<&'a Value>,
    pub(crate) pre: &'a str,
    pub(crate) test: &'a str,
}
