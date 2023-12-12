use std::collections::HashSet;

use miette::{Diagnostic, NamedSource, SourceSpan};
use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, tag_no_case, take_until, take_while, take_while1},
    character::complete::{digit1, hex_digit1, multispace1, one_of, space1},
    combinator::{map, opt, recognize, value},
    error::ErrorKind,
    multi::{many0, separated_list0},
    sequence::{delimited, preceded, tuple},
    IResult, Parser,
};
use nom_greedyerror::GreedyError;
use nom_locate::LocatedSpan;
use nom_supreme::ParserExt;
use thiserror::Error;
use tracing::warn;

// easier to type and not move str around
type Span<'a> = LocatedSpan<&'a str>;
//type ParseError<'a> = VerboseError<Span<'a>>;
type ParseError<'a> = GreedyError<Span<'a>, ErrorKind>;

/// Fetch the deepest location of an error within an error type
pub trait DeepestIndex {
    fn depest_index(&self) -> Option<usize>;
}

impl<E> DeepestIndex for nom::Err<E>
where
    E: DeepestIndex,
{
    fn depest_index(&self) -> Option<usize> {
        match self {
            nom::Err::Error(e) => e.depest_index(),
            nom::Err::Failure(e) => e.depest_index(),
            nom::Err::Incomplete(_) => None,
        }
    }
}

impl<'a> DeepestIndex for GreedyError<Span<'a>, ErrorKind> {
    fn depest_index(&self) -> Option<usize> {
        self.errors.iter().map(|(p, _k)| p.location_offset()).max()
    }
}

/// Keep track of the deepest error encoutered
#[derive(Debug, PartialEq, Clone)]
struct DeepestError<E> {
    deepest: Option<(usize, E)>,
}

impl<E> DeepestError<E>
where
    E: DeepestIndex + Clone + std::fmt::Debug,
{
    pub fn new() -> Self {
        Self { deepest: None }
    }

    pub fn or(self, e: E) -> E {
        match self.deepest {
            Some((_, myerror)) => myerror,
            None => e,
        }
    }

    pub fn intercept<O>(&mut self, data: Result<O, E>) -> Result<O, E> {
        if let Err(ref e) = data {
            if let Some(depth) = e.depest_index() {
                let current_depth = self.deepest.as_ref().map(|(d, _)| *d).unwrap_or(0);
                if current_depth < depth {
                    self.deepest = Some((depth, e.clone()));
                }
            };
        }
        data
    }
}

/// How mature/usable a member of an API is
///
/// Most things should be stable, however while spec is developed
/// we expect PROVISIONAL to be set.
#[derive(Debug, PartialEq, Copy, Clone, Hash, PartialOrd, Eq, Ord, Default)]
pub enum ApiMaturity {
    #[default]
    STABLE,
    PROVISIONAL,
    INTERNAL,
    DEPRECATED,
}

/// A parser that CANNOT fail
///
/// Note that it will consume no input if no maturity is specified
/// in which case it returns 'STABLE'
///
/// Examples:
///
/// ```
/// use rs_matter_idl_parser::{api_maturity, ApiMaturity};
///
/// assert_eq!(
///    api_maturity("123".into()),
///    Ok(("123".into(), ApiMaturity::STABLE))
/// );
///
/// let result = api_maturity("provisional 123".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), " 123");
/// assert_eq!(result.1, ApiMaturity::PROVISIONAL);
/// ```
pub fn api_maturity(span: Span) -> IResult<Span, ApiMaturity, ParseError> {
    if let Ok((span, _)) = tag_no_case::<_, _, ()>("stable").parse(span) {
        return Ok((span, ApiMaturity::STABLE));
    }
    if let Ok((span, _)) = tag_no_case::<_, _, ()>("provisional").parse(span) {
        return Ok((span, ApiMaturity::PROVISIONAL));
    }
    if let Ok((span, _)) = tag_no_case::<_, _, ()>("internal").parse(span) {
        return Ok((span, ApiMaturity::INTERNAL));
    }
    if let Ok((span, _)) = tag_no_case::<_, _, ()>("deprecated").parse(span) {
        return Ok((span, ApiMaturity::DEPRECATED));
    }

    Ok((span, ApiMaturity::STABLE))
}

/// Parses a hex-formated integer
///
/// Examples:
///
/// ```
/// use rs_matter_idl_parser::hex_integer;
///
/// let result = hex_integer("0x12 abc".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), " abc");
/// assert_eq!(result.1, 0x12);
///
/// let result = hex_integer("0X12abctest".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "test");
/// assert_eq!(result.1, 0x12abc);
/// ```
pub fn hex_integer(span: Span) -> IResult<Span, u64, ParseError> {
    hex_digit1::<Span, ParseError>
        .preceded_by(tag_no_case("0x"))
        .map(|r| u64::from_str_radix(r.fragment(), 16).expect("valid hex digits"))
        .parse(span)
}

/// Parses a decimal-formated integer
///
/// Examples:
///
/// ```
/// use rs_matter_idl_parser::decimal_integer;
///
/// let result = decimal_integer("12 abc".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), " abc");
/// assert_eq!(result.1, 12);
///
/// let result = decimal_integer("12abctest".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "abctest");
/// assert_eq!(result.1, 12);
/// ```
pub fn decimal_integer(span: Span) -> IResult<Span, u64, ParseError> {
    digit1::<Span, ParseError>
        .map(|s| s.fragment().parse::<u64>().expect("valid digits"))
        .parse(span)
}

/// Parses a positive integer (hex or decimal)
///
/// Examples:
///
/// ```
/// use rs_matter_idl_parser::positive_integer;
///
/// let result = positive_integer("12 abc".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), " abc");
/// assert_eq!(result.1, 12);
///
/// let result = positive_integer("12abctest".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "abctest");
/// assert_eq!(result.1, 12);
///
/// let result = positive_integer("0x12abctest".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "test");
/// assert_eq!(result.1, 0x12abc);
/// ```
pub fn positive_integer(span: Span) -> IResult<Span, u64, ParseError> {
    // NOTE: orer is important so that
    // 0x123 is a hex not 0 followed by "x123"
    if let Ok(r) = hex_integer.parse(span) {
        return Ok(r);
    }
    decimal_integer.parse(span)
}

/// Represents a comment (i.e. something between `/** ... */`)
///
/// Typically placed before some element (e.g. cluster or command) to serve
/// as documentation for it.
///
/// Parsing whitespace yields doc-comments if the last comment in a whitespace
/// sequence is a doc comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocComment<'a>(pub &'a str);

/// Information returned while parsing whitespace.
///
/// Contains the underlying content of the whitespace, which is
/// especially useful for documentation comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Whitespace<'a> {
    DocComment(&'a str), // /** ... */
    CppComment(&'a str), // /* ... */ (and NOT a doc comment)
    CComment(&'a str),   // // ....
    Whitespace(&'a str), // general newline/space/tab
}

/// Parses whitespace (space/tab/newline and comments).
///
/// returns the content of the comment
pub fn whitespace_group(span: Span) -> IResult<Span, Whitespace<'_>, ParseError> {
    // NOTE: split into cases intentional. Using an ALT pattern here
    //       seems to slow down things quite a bit (as whitespace is used a lot
    //       inside our parsing)

    // C-style comment,
    if let Ok((span, c)) = preceded(tag::<_, _, ()>("//"), is_not("\n\r")).parse(span) {
        return Ok((span, Whitespace::CComment(c.fragment())));
    }

    // CPP-comment. May be a doc-comment if starting with '/**'
    if let Ok((span, cpp)) =
        delimited(tag::<_, _, ()>("/*"), take_until("*/"), tag("*/")).parse(span)
    {
        return Ok((
            span,
            if cpp.starts_with('*') {
                Whitespace::DocComment(&cpp.fragment()[1..])
            } else {
                Whitespace::CppComment(cpp.fragment())
            },
        ));
    }

    // finally just a set of spaces, must be at least 1
    multispace1
        .map(|c: Span| Whitespace::Whitespace(c.fragment()))
        .parse(span)
}

/// Parses 0 or more whitespaces.
/// It can NEVER fail.
///
/// If the last comment whitespace is a doc-comment, returns
/// that doc-comment.
///
/// Examples:
///
/// ```
/// use rs_matter_idl_parser::{whitespace0, DocComment};
///
/// let result = whitespace0(" /*comment*/\n12 abc".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "12 abc");
/// assert_eq!(result.1, None);
///
/// let result = whitespace0(" /**doc comment*/\n abc".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "abc");
/// assert_eq!(result.1, Some(DocComment("doc comment")));
///
/// let result = whitespace0("no whitespace".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "no whitespace");
/// assert_eq!(result.1, None);
/// ```
pub fn whitespace0(span: Span) -> IResult<Span, Option<DocComment>, ParseError> {
    // early bail out if it cannot be whitespace
    // Whitespace is only tab/newline/space or `/` for cpp/c comments
    match span.chars().next() {
        Some('\r' | '\n' | '\t' | ' ' | '/') => (),
        _ => return Ok((span, None)),
    }

    let (mut rest, mut doc) = match whitespace_group(span) {
        Err(_) => return Ok((span, None)),
        Ok((span, Whitespace::DocComment(c))) => (span, Some(DocComment(c))),
        Ok((span, _)) => (span, None),
    };

    // now consume all other results if any
    loop {
        match whitespace_group(rest) {
            Ok((span, whitespace)) => {
                rest = span;
                match whitespace {
                    Whitespace::DocComment(comment) => doc = Some(DocComment(comment)),
                    Whitespace::CComment(_) => doc = None,
                    Whitespace::CppComment(_) => doc = None,
                    Whitespace::Whitespace(_) => {}
                }
            }
            Err(_) => return Ok((rest, doc)),
        }
    }
}

/// Parses at least one whitespace
/// If the last comment whitespace is a doccomment, then
/// It returns that doc comment.
///
/// Examples:
///
/// ```
/// use rs_matter_idl_parser::{whitespace1, DocComment};
///
/// let result = whitespace1(" /*comment*/\n12 abc".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "12 abc");
/// assert_eq!(result.1, None);
///
/// let result = whitespace1(" /**doc comment*/\n abc".into()).expect("Valid");
/// assert_eq!(result.0.fragment().to_string(), "abc");
/// assert_eq!(result.1, Some(DocComment("doc comment")));
/// ```
pub fn whitespace1(span: Span) -> IResult<Span, Option<DocComment>, ParseError> {
    let parsed = whitespace0(span)?;

    if span == parsed.0 {
        // this WILL fail, using it as such just to get a proper error
        space1::<_, ParseError>(span)?;
    }

    Ok(parsed)
}

/// Parses a name id, of the form /[a-zA-Z_][a-zA-Z0-9_]*/
///
pub fn parse_id(span: Span) -> IResult<Span, &str, ParseError> {
    let valid_first = |c: char| c.is_ascii_alphabetic() || c == '_';
    let valid_second = |c: char| c.is_ascii_alphanumeric() || c == '_';
    map(
        recognize(tuple((take_while1(valid_first), take_while(valid_second)))),
        |data: Span| *data.fragment(),
    )(span)
}

/// A named numeric value.
///
/// A value that has a name (e.g. enumeration or bitmap constant).
/// May also have an associated maturity that defaults to STABLE
/// while parsing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstantEntry<'a> {
    pub maturity: ApiMaturity,
    pub id: &'a str,
    pub code: u64,
}

impl<'a> ConstantEntry<'a> {
    /// Parses a IDL representation of a constant entry.
    ///
    /// Consumes any whitespace BEFORE the entry.
    ///
    /// Examples:
    ///
    /// ```
    /// use rs_matter_idl_parser::{ConstantEntry, ApiMaturity};
    ///
    /// let parsed = ConstantEntry::parse("provisional kConstant = 0x123 ;".into()).expect("valid");
    /// assert_eq!(parsed.0.fragment().to_string(), "");
    /// assert_eq!(
    ///         parsed.1,
    ///         ConstantEntry {
    ///             id: "kConstant",
    ///             code: 0x123,
    ///             maturity: ApiMaturity::PROVISIONAL
    ///         }
    /// );
    /// ```
    pub fn parse(span: Span) -> IResult<Span, ConstantEntry<'_>, ParseError> {
        tuple((
            whitespace0,
            api_maturity,
            whitespace0,
            parse_id,
            whitespace0,
            tag("="),
            whitespace0,
            positive_integer,
            whitespace0,
            tag(";"),
        ))
        .map(|(_, maturity, _, id, _, _, _, code, _, _)| ConstantEntry { maturity, id, code })
        .parse(span)
    }
}

/// Parses a list of constant entries, delimeted by "{" "}".
///
/// Consumes the '{' '}' as well as any internal whitespace in them
fn constant_entries_list(span: Span) -> IResult<Span, Vec<ConstantEntry<'_>>, ParseError> {
    delimited(
        tag("{"),
        tuple((
            many0(tuple((whitespace0, ConstantEntry::parse)).map(|(_, v)| v)),
            whitespace0,
        )),
        tag("}"),
    )
    .map(|(v, _)| v)
    .parse(span)
}

/// A set of constant entries that correspont to an enumeration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Enum<'a> {
    pub doc_comment: Option<&'a str>,
    pub maturity: ApiMaturity,
    pub id: &'a str,
    pub base_type: &'a str,
    pub entries: Vec<ConstantEntry<'a>>,
}

impl Enum<'_> {
    pub fn parse(span: Span) -> IResult<Span, Enum<'_>, ParseError> {
        let (span, comment) = whitespace0(span)?;
        let doc_comment = comment.map(|DocComment(comment)| comment);
        let (span, maturity) = delimited(whitespace0, api_maturity, whitespace0).parse(span)?;

        Enum::parse_after_doc_maturity(doc_comment, maturity, span)
    }

    pub fn parse_after_doc_maturity<'a: 'c, 'b: 'c, 'c>(
        doc_comment: Option<&'a str>,
        maturity: ApiMaturity,
        span: Span<'b>,
    ) -> IResult<Span<'b>, Enum<'c>, ParseError<'b>> {
        tuple((
            tag_no_case("enum"),
            whitespace1,
            parse_id,
            whitespace0,
            tag(":"),
            whitespace0,
            parse_id,
            whitespace0,
            constant_entries_list,
        ))
        .map(|(_, _, id, _, _, _, base_type, _, entries)| Enum {
            doc_comment,
            maturity,
            id,
            base_type,
            entries,
        })
        .parse(span)
    }
}

/// A set of constant entries that correspont to a bitmap.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bitmap<'a> {
    pub doc_comment: Option<&'a str>,
    pub maturity: ApiMaturity,
    pub id: &'a str,
    pub base_type: &'a str,
    pub entries: Vec<ConstantEntry<'a>>,
}

impl Bitmap<'_> {
    pub fn parse(span: Span) -> IResult<Span, Bitmap<'_>, ParseError> {
        let (span, comment) = whitespace0(span)?;
        let doc_comment = comment.map(|DocComment(comment)| comment);
        let (span, maturity) = delimited(whitespace0, api_maturity, whitespace0).parse(span)?;

        Bitmap::parse_after_doc_maturity(doc_comment, maturity, span)
    }

    pub fn parse_after_doc_maturity<'a: 'c, 'b: 'c, 'c>(
        doc_comment: Option<&'a str>,
        maturity: ApiMaturity,
        span: Span<'b>,
    ) -> IResult<Span<'b>, Bitmap<'c>, ParseError<'b>> {
        tuple((
            tag_no_case("bitmap"),
            whitespace1,
            parse_id,
            whitespace0,
            tag(":"),
            whitespace0,
            parse_id,
            whitespace0,
            constant_entries_list,
        ))
        .map(|(_, _, id, _, _, _, base_type, _, entries)| Bitmap {
            doc_comment,
            maturity,
            id,
            base_type,
            entries,
        })
        .parse(span)
    }
}

/// A generic type such as integers, strings, enums etc.
///
/// Supports information if this is repeated/list as well
/// as a maximum length (if applicable).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DataType<'a> {
    name: &'a str,
    is_list: bool,
    max_length: Option<u64>,
}

impl<'a> DataType<'a> {
    pub fn scalar(name: &'_ str) -> DataType<'_> {
        DataType {
            name,
            is_list: false,
            max_length: None,
        }
    }

    pub fn list_of(name: &'_ str) -> DataType<'_> {
        DataType {
            name,
            is_list: true,
            max_length: None,
        }
    }

    pub fn scalar_of_size(name: &'_ str, max_length: u64) -> DataType<'_> {
        DataType {
            name,
            is_list: false,
            max_length: Some(max_length),
        }
    }
}

/// Represents a generic field.
///
/// Fields have a type, name(id) and numeric code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Field<'a> {
    pub data_type: DataType<'a>,
    pub id: &'a str,
    pub code: u64,
}

impl Field<'_> {
    pub fn parse(span: Span) -> IResult<Span, Field<'_>, ParseError> {
        tuple((
            whitespace0,
            parse_id,
            opt(tuple((
                whitespace0,
                tag("<"),
                whitespace0,
                positive_integer,
                whitespace0,
                tag(">"),
            ))
            .map(|(_, _, _, pos, _, _)| pos)),
            whitespace1,
            parse_id,
            whitespace0,
            opt(tuple((tag("["), whitespace0, tag("]"), whitespace0))),
            tag("="),
            whitespace0,
            positive_integer,
        ))
        .map(
            |(_, type_name, max_length, _, id, _, list_marker, _, _, code)| Field {
                data_type: DataType {
                    name: type_name,
                    is_list: list_marker.is_some(),
                    max_length,
                },
                id,
                code,
            },
        )
        .parse(span)
    }
}

/// Grabs a tag set which are whitespace-separated list of items
///
/// Returns applyin the parser and extracting a HashSet of the given tags.
macro_rules! tags_set {
    ($span:ident, $($tags:expr),+) => {{
        let mut result = HashSet::new();
        let mut rest = $span;
        loop {
           let mut element_start = rest;
           if !result.is_empty() {
               match whitespace1.parse(element_start) {
                   Ok((p, _)) => element_start = p,
                   Err(_) => break,
               }
           }

           $(
           if let Ok((tail, tag)) = nom::bytes::complete::tag_no_case::<_,_,()>($tags).parse(element_start) {
               rest = tail;
               result.insert(*tag.fragment());
               continue;
           } else
           )+
           {
              break;
           }
        }
        (rest, result)
    }
    };
}

/// Represents a field entry within a struct.
///
/// Specifically this adds structure specific information
/// such as API maturity, optional/nullable/fabric_sensitive
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StructField<'a> {
    pub field: Field<'a>,
    pub maturity: ApiMaturity,
    pub is_optional: bool,
    pub is_nullable: bool,
    pub is_fabric_sensitive: bool,
}

impl StructField<'_> {
    pub fn parse(span: Span) -> IResult<Span, StructField<'_>, ParseError> {
        let (span, maturity) = delimited(whitespace0, api_maturity, whitespace0).parse(span)?;
        let (span, attributes) = tags_set!(span, "optional", "nullable", "fabric_sensitive");

        let is_optional = attributes.contains("optional");
        let is_nullable = attributes.contains("nullable");
        let is_fabric_sensitive = attributes.contains("fabric_sensitive");

        let (span, field) = Field::parse(span)?;

        Ok((
            span,
            StructField {
                field,
                maturity,
                is_optional,
                is_nullable,
                is_fabric_sensitive,
            },
        ))
    }
}

fn struct_fields(span: Span) -> IResult<Span, Vec<StructField<'_>>, ParseError> {
    delimited(
        tag("{"),
        many0(delimited(
            whitespace0,
            StructField::parse,
            tuple((whitespace0, tag(";"))),
        )),
        tuple((whitespace0, tag("}"))),
    )
    .parse(span)
}

/// Defines the type of a structure.
///
/// Response structures contain the underlying code used to send
/// that structure as a reply.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StructType {
    Regular,
    Request,
    Response(u64), // response with a code
}

/// A structure defined in IDL.
///
/// Structures may be regular (as data types), request (used in command inputs)
/// or responses (used as command outputs, have an id)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Struct<'a> {
    pub doc_comment: Option<&'a str>,
    pub maturity: ApiMaturity,
    pub struct_type: StructType,
    pub id: &'a str,
    pub fields: Vec<StructField<'a>>,
    pub is_fabric_scoped: bool,
}

impl Struct<'_> {
    pub fn parse(span: Span) -> IResult<Span, Struct<'_>, ParseError> {
        let (span, doc_comment) = whitespace0.parse(span)?;
        let doc_comment = doc_comment.map(|DocComment(s)| s);
        let (span, maturity) = delimited(whitespace0, api_maturity, whitespace0).parse(span)?;

        Self::parse_after_doc_maturity(doc_comment, maturity, span)
    }

    pub fn parse_after_doc_maturity<'a: 'c, 'b: 'c, 'c>(
        doc_comment: Option<&'a str>,
        maturity: ApiMaturity,
        span: Span<'b>,
    ) -> IResult<Span<'b>, Struct<'c>, ParseError<'b>> {
        let (span, struct_type) =
            opt(alt((tag_no_case("request"), tag_no_case("response"))))(span)?;
        let struct_type = struct_type.map(|f| *f.fragment());

        let (span, _) = whitespace0.parse(span)?;

        let (span, attributes) = tags_set!(span, "fabric_scoped");

        let is_fabric_scoped = attributes.contains("fabric_scoped");

        let (span, id) = delimited(
            tuple((whitespace0, tag_no_case("struct"), whitespace1)),
            parse_id,
            whitespace0,
        )
        .parse(span)?;

        let (span, struct_type) = match struct_type {
            Some("request") => (span, StructType::Request),
            Some("response") => tuple((tag("="), whitespace0, positive_integer, whitespace0))
                .map(|(_, _, id, _)| StructType::Response(id))
                .parse(span)?,
            _ => (span, StructType::Regular),
        };

        let (span, fields) = struct_fields(span)?;

        Ok((
            span,
            Struct {
                doc_comment,
                maturity,
                struct_type,
                id,
                fields,
                is_fabric_scoped,
            },
        ))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessPrivilege {
    View,
    Operate,
    Manage,
    Administer,
}

pub fn access_privilege(span: Span) -> IResult<Span, AccessPrivilege, ParseError> {
    if let Ok((span, _)) = tag_no_case::<_, _, ()>("view").parse(span) {
        return Ok((span, AccessPrivilege::View));
    }
    if let Ok((span, _)) = tag_no_case::<_, _, ()>("operate").parse(span) {
        return Ok((span, AccessPrivilege::Operate));
    }
    if let Ok((span, _)) = tag_no_case::<_, _, ()>("manage").parse(span) {
        return Ok((span, AccessPrivilege::Manage));
    }

    value(AccessPrivilege::Administer, tag_no_case("administer")).parse(span)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventPriority {
    Critical,
    Info,
    Debug,
}

pub fn event_priority(span: Span) -> IResult<Span, EventPriority, ParseError> {
    if let Ok((span, _)) = tag_no_case::<_, _, ()>("info").parse(span) {
        return Ok((span, EventPriority::Info));
    }

    if let Ok((span, _)) = tag_no_case::<_, _, ()>("critical").parse(span) {
        return Ok((span, EventPriority::Critical));
    }

    value(EventPriority::Debug, tag_no_case("debug")).parse(span)
}

/// An event structure inside the IDL
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Event<'a> {
    pub doc_comment: Option<&'a str>,
    pub maturity: ApiMaturity,
    pub priority: EventPriority,
    pub access: AccessPrivilege,
    pub id: &'a str,
    pub code: u64,
    pub fields: Vec<StructField<'a>>,
    pub is_fabric_sensitive: bool,
}

impl Event<'_> {
    pub fn parse(span: Span) -> IResult<Span, Event<'_>, ParseError> {
        let (span, doc_comment) = whitespace0.parse(span)?;
        let doc_comment = doc_comment.map(|DocComment(s)| s);
        let (span, maturity) = delimited(whitespace0, api_maturity, whitespace0).parse(span)?;

        Self::parse_after_doc_maturity(doc_comment, maturity, span)
    }

    pub fn parse_after_doc_maturity<'a: 'c, 'b: 'c, 'c>(
        doc_comment: Option<&'a str>,
        maturity: ApiMaturity,
        span: Span<'b>,
    ) -> IResult<Span<'b>, Event<'c>, ParseError<'b>> {
        let (span, attributes) = tags_set!(span, "fabric_sensitive");
        let is_fabric_sensitive = attributes.contains("fabric_sensitive");

        tuple((
            preceded(whitespace0, event_priority),
            whitespace1,
            tag_no_case("event"),
            whitespace1,
            opt(delimited(
                tuple((
                    tag_no_case("access"),
                    whitespace0,
                    tag("("),
                    whitespace0,
                    tag_no_case("read"),
                    tag(":"),
                    whitespace0,
                )),
                access_privilege,
                tuple((whitespace0, tag(")"))),
            ))
            .map(|p| p.unwrap_or(AccessPrivilege::View)),
            preceded(whitespace0, parse_id),
            preceded(
                tuple((whitespace0, tag("="), whitespace0)),
                positive_integer,
            ),
            preceded(whitespace0, struct_fields),
        ))
        .map(|(priority, _, _, _, access, id, code, fields)| Event {
            doc_comment,
            maturity,
            priority,
            access,
            id,
            code,
            fields,
            is_fabric_sensitive,
        })
        .parse(span)
    }
}

/// A command that can be executed on a cluster
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Command<'a> {
    pub doc_comment: Option<&'a str>,
    pub maturity: ApiMaturity,
    pub access: AccessPrivilege, // invoke access privilege
    pub id: &'a str,
    pub input: Option<&'a str>,
    pub output: &'a str,
    pub code: u64,
    pub is_timed: bool,
    pub is_fabric_scoped: bool,
}

impl Default for Command<'_> {
    fn default() -> Self {
        Self {
            access: AccessPrivilege::Operate,
            doc_comment: None,
            maturity: ApiMaturity::STABLE,
            id: "",
            input: None,
            output: "DefaultSuccess",
            code: 0,
            is_timed: false,
            is_fabric_scoped: false,
        }
    }
}

impl Command<'_> {
    pub fn parse(span: Span) -> IResult<Span, Command<'_>, ParseError> {
        let (span, doc_comment) = whitespace0.parse(span)?;
        let doc_comment = doc_comment.map(|DocComment(s)| s);
        let (span, maturity) = delimited(whitespace0, api_maturity, whitespace0).parse(span)?;

        Self::parse_after_doc_maturity(doc_comment, maturity, span)
    }

    pub fn parse_after_doc_maturity<'a: 'c, 'b: 'c, 'c>(
        doc_comment: Option<&'a str>,
        maturity: ApiMaturity,
        span: Span<'b>,
    ) -> IResult<Span<'b>, Command<'c>, ParseError<'b>> {
        let (span, qualities) = tags_set!(span, "timed", "fabric");
        let is_timed = qualities.contains("timed");
        let is_fabric_scoped = qualities.contains("fabric");

        let access_parser = opt(tuple((
            tuple((
                whitespace0,
                tag_no_case("access"),
                whitespace0,
                tag("("),
                whitespace0,
                tag_no_case("invoke"),
                tag(":"),
                whitespace0,
            )),
            access_privilege,
            tuple((whitespace0, tag(")"))),
        ))
        .map(|(_, p, _)| p))
        .map(|opt_access| opt_access.unwrap_or(AccessPrivilege::Operate));

        tuple((
            tuple((whitespace0, tag_no_case("command"))),
            access_parser,
            whitespace0,
            parse_id,
            tuple((whitespace0, tag("("), whitespace0)),
            opt(parse_id),
            tuple((whitespace0, tag(")"), whitespace0, tag(":"), whitespace0)),
            parse_id,
            tuple((whitespace0, tag("="), whitespace0)),
            positive_integer,
            tuple((whitespace0, tag(";"))),
        ))
        .map(
            |(_, access, _, id, _, input, _, output, _, code, _)| Command {
                doc_comment,
                maturity,
                access,
                id,
                input,
                output,
                code,
                is_timed,
                is_fabric_scoped,
            },
        )
        .parse(span)
    }
}

/// An attribute within a cluster
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attribute<'a> {
    pub doc_comment: Option<&'a str>,
    pub maturity: ApiMaturity,
    pub field: StructField<'a>,
    pub read_acl: AccessPrivilege,
    pub write_acl: AccessPrivilege,
    pub is_read_only: bool,
    pub is_no_subscribe: bool,
    pub is_timed_write: bool,
}

impl<'a> Default for Attribute<'a> {
    fn default() -> Self {
        Self {
            doc_comment: None,
            maturity: ApiMaturity::STABLE,
            field: Default::default(),
            read_acl: AccessPrivilege::View,
            write_acl: AccessPrivilege::Operate,
            is_read_only: false,
            is_no_subscribe: false,
            is_timed_write: false,
        }
    }
}

// Returns read & write access,
// CANNOT fail (returns defaults if it fails)
fn attribute_access(span: Span) -> IResult<Span, (AccessPrivilege, AccessPrivilege), ParseError> {
    let (span, tags) = opt(delimited(
        tuple((
            whitespace0,
            tag_no_case("access"),
            whitespace0,
            tag("("),
            whitespace0,
        )),
        separated_list0(
            tuple((whitespace0, tag(","), whitespace0)),
            tuple((
                whitespace0,
                alt((tag_no_case("read"), tag_no_case("write"))),
                whitespace0,
                tag(":"),
                whitespace0,
                access_privilege,
                whitespace0,
            ))
            .map(|(_, k, _, _, _, v, _)| (*k.fragment(), v)),
        ),
        tuple((whitespace0, tag(")"))),
    ))
    .parse(span)?;

    let mut read_acl = AccessPrivilege::View;
    let mut write_acl = AccessPrivilege::Operate;

    if let Some(entries) = tags {
        for entry in entries {
            match entry.0 {
                "read" => read_acl = entry.1,
                "write" => write_acl = entry.1,
                _ => panic!("Should hjave only matched read or write"),
            }
        }
    }

    Ok((span, (read_acl, write_acl)))
}

impl Attribute<'_> {
    pub fn parse(span: Span) -> IResult<Span, Attribute<'_>, ParseError> {
        let (span, doc_comment) = whitespace0.parse(span)?;
        let doc_comment = doc_comment.map(|DocComment(s)| s);
        let (span, maturity) = delimited(whitespace0, api_maturity, whitespace0).parse(span)?;

        Self::parse_after_doc_maturity(doc_comment, maturity, span)
    }

    pub fn parse_after_doc_maturity<'a: 'c, 'b: 'c, 'c>(
        doc_comment: Option<&'a str>,
        maturity: ApiMaturity,
        span: Span<'b>,
    ) -> IResult<Span<'b>, Attribute<'c>, ParseError<'b>> {
        let (span, qualities) = tags_set!(span, "readonly", "nosubscribe", "timedwrite");
        let is_read_only = qualities.contains("readonly");
        let is_no_subscribe = qualities.contains("nosubscribe");
        let is_timed_write = qualities.contains("timedwrite");

        tuple((
            whitespace0,
            tag_no_case("attribute"),
            whitespace1,
            attribute_access,
            whitespace0,
            StructField::parse,
            whitespace0,
            tag(";"),
        ))
        .map(
            |(_, _, _, (read_acl, write_acl), _, field, _, _)| Attribute {
                doc_comment,
                maturity,
                field,
                read_acl,
                write_acl,
                is_read_only,
                is_no_subscribe,
                is_timed_write,
            },
        )
        .parse(span)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Cluster<'a> {
    pub doc_comment: Option<&'a str>,
    pub maturity: ApiMaturity,

    pub id: &'a str,
    pub code: u64,
    pub revision: u64,

    pub bitmaps: Vec<Bitmap<'a>>,
    pub enums: Vec<Enum<'a>>,
    pub structs: Vec<Struct<'a>>,

    pub events: Vec<Event<'a>>,
    pub attributes: Vec<Attribute<'a>>,
    pub commands: Vec<Command<'a>>,
}

impl<'a> Cluster<'a> {
    fn parse_member<'b: 'a, 'c>(&'c mut self, span: Span<'b>) -> Option<Span<'b>> {
        let (span, (doc_comment, maturity, _)) = tuple((
            whitespace0.map(|o| o.map(|DocComment(s)| s)),
            api_maturity,
            whitespace0,
        ))
        .parse(span)
        .ok()?;

        if let Ok((rest, revision)) = delimited(
            tuple((tag_no_case("revision"), whitespace1)),
            positive_integer,
            tuple((whitespace0, tag(";"))),
        )
        .parse(span)
        {
            self.revision = revision;
            return Some(rest);
        }

        if let Ok((rest, b)) = Bitmap::parse_after_doc_maturity(doc_comment, maturity, span) {
            self.bitmaps.push(b);
            return Some(rest);
        }
        if let Ok((rest, e)) = Enum::parse_after_doc_maturity(doc_comment, maturity, span) {
            self.enums.push(e);
            return Some(rest);
        }
        if let Ok((rest, s)) = Struct::parse_after_doc_maturity(doc_comment, maturity, span) {
            self.structs.push(s);
            return Some(rest);
        }
        if let Ok((rest, a)) = Attribute::parse_after_doc_maturity(doc_comment, maturity, span) {
            self.attributes.push(a);
            return Some(rest);
        }
        if let Ok((rest, c)) = Command::parse_after_doc_maturity(doc_comment, maturity, span) {
            self.commands.push(c);
            return Some(rest);
        }
        if let Ok((rest, e)) = Event::parse_after_doc_maturity(doc_comment, maturity, span) {
            self.events.push(e);
            return Some(rest);
        }
        None
    }

    pub fn parse(span: Span) -> IResult<Span, Cluster<'_>, ParseError> {
        let (span, doc_comment) = whitespace0.parse(span)?;
        let doc_comment = doc_comment.map(|DocComment(s)| s);

        let (span, maturity) = tuple((api_maturity, whitespace0))
            .map(|(m, _)| m)
            .parse(span)?;

        let (span, mut cluster) = delimited(
            tuple((
                opt(tuple((
                    alt((tag_no_case("client"), tag_no_case("server"))),
                    whitespace1,
                ))),
                tag_no_case("cluster"),
                whitespace1,
            )),
            tuple((
                parse_id,
                whitespace0,
                tag("="),
                whitespace0,
                positive_integer,
            )),
            whitespace0,
        )
        .map(|(id, _, _, _, code)| Cluster {
            doc_comment,
            maturity,
            id,
            code,
            ..Default::default()
        })
        .parse(span)?;

        let (mut span, _) = tag("{").parse(span)?;
        while let Some(rest) = cluster.parse_member(span) {
            span = rest;
        }

        // finally consume the final tag
        value(cluster, tuple((whitespace0, tag("}")))).parse(span)
    }
}

// Represents a specific device type
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct DeviceType<'a> {
    pub name: &'a str,
    pub code: u64,
    pub version: u64,
}

/// parse a device type. Does NOT expect preceeding whitespace
pub fn device_type(span: Span) -> IResult<Span, DeviceType, ParseError> {
    tuple((
        parse_id.preceded_by(tuple((
            tag_no_case("device"),
            whitespace1,
            tag_no_case("type"),
            whitespace1,
        ))),
        positive_integer.preceded_by(tuple((whitespace0, tag("="), whitespace0))),
        positive_integer
            .preceded_by(tuple((
                whitespace0,
                tag(","),
                whitespace0,
                tag_no_case("version"),
                whitespace0,
            )))
            .terminated(tuple((whitespace0, tag(";")))),
    ))
    .map(|(name, code, version)| DeviceType {
        name,
        code,
        version,
    })
    .parse(span)
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum DefaultAttributeValue {
    Number(u64),
    Signed(i64),
    String(String),
    Bool(bool),
}

/// Parses a default attribute value.
///
/// Does NOT consume leading spaces or spaces after the value
pub fn default_attribute_value(span: Span) -> IResult<Span, DefaultAttributeValue, ParseError> {
    let mut deepest_error = DeepestError::new();

    // make sure we have some default before trying to parse
    let (span, _) = deepest_error.intercept(
        tuple((tag_no_case("default"), whitespace0, tag("="), whitespace0)).parse(span),
    )?;

    if let Ok((rest, n)) = deepest_error.intercept(positive_integer.parse(span)) {
        // TODO: bitwise compare here may be rough
        return Ok((rest, DefaultAttributeValue::Number(n)));
    }

    // at this point there is a default.
    if let Ok((rest, n)) = deepest_error.intercept(nom::character::complete::i64.parse(span)) {
        return Ok((rest, DefaultAttributeValue::Signed(n)));
    }

    if let Ok((rest, _)) = deepest_error.intercept(tag_no_case("true").parse(span)) {
        return Ok((rest, DefaultAttributeValue::Bool(true)));
    }

    if let Ok((rest, _)) = deepest_error.intercept(tag_no_case("false").parse(span)) {
        return Ok((rest, DefaultAttributeValue::Bool(false)));
    }

    // remove prefix and parse
    // This lengthy unescape logic is because I could not get
    // nom escape/escape_transform to work
    let (mut span, _) = match tag("\"").parse(span) {
        Ok(x) => x,
        Err(e) => return Err(deepest_error.or(e)),
    };
    let mut result = String::new();

    loop {
        let (rest, data) = take_while(|c| c != '\"' && c != '\\').parse(span)?;
        let data = *data.fragment();

        result.push_str(data);

        // reached an end. MUST be quote or backslash
        let (rest, ch) = one_of("\"\\").parse(rest)?;

        match ch {
            '\\' => (), // escaped tag
            '"' => {
                // string end
                span = rest;
                break;
            }
            _ => panic!("Not expected!"),
        }

        // unescape the next
        let (rest, ch) = one_of(r#""\nrt"#)
            .map(|c| match c {
                '"' | '\\' => c,
                'n' => '\n',
                't' => '\t',
                _ => panic!("Invalid escape"),
            })
            .parse(rest)?;

        result.push(ch); // add the backslash
        span = rest;
    }

    Ok((span, DefaultAttributeValue::String(result)))
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub enum AttributeHandlingType {
    #[default]
    Ram,
    Callback,
    Persist,
}

pub fn attribute_handling_type(span: Span) -> IResult<Span, AttributeHandlingType, ParseError> {
    if let Ok(r) = value(AttributeHandlingType::Ram, tag_no_case::<_, _, ()>("ram")).parse(span) {
        return Ok(r);
    }
    if let Ok(r) = value(
        AttributeHandlingType::Callback,
        tag_no_case::<_, _, ()>("callback"),
    )
    .parse(span)
    {
        return Ok(r);
    }
    value(AttributeHandlingType::Persist, tag_no_case("persist")).parse(span)
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct AttributeInstantiation<'a> {
    pub handle_type: AttributeHandlingType,
    pub name: &'a str,
    pub default: Option<DefaultAttributeValue>,
}

pub fn attribute_instantiation(span: Span) -> IResult<Span, AttributeInstantiation, ParseError> {
    // TODO: if opt fails here, error reporting does not recurse deep inside the optional
    //       since it is optional and the "terminated" will refuse
    // May need to figure out how to detect the farthest we ever parsed, maybe using span itself
    // as an overload.
    tuple((
        attribute_handling_type,
        parse_id.preceded_by(tuple((whitespace1, tag_no_case("attribute"), whitespace1))),
        opt(default_attribute_value.preceded_by(whitespace1)),
    ))
    .terminated(tuple((whitespace0, tag(";"))))
    .map(|(handle_type, name, default)| AttributeInstantiation {
        handle_type,
        name,
        default,
    })
    .parse(span)
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct ClusterInstantiation<'a> {
    pub name: &'a str,
    pub attributes: Vec<AttributeInstantiation<'a>>,
    pub commands: Vec<&'a str>,
    pub events: Vec<&'a str>,
}

pub fn cluster_instantiation(span: Span) -> IResult<Span, ClusterInstantiation<'_>, ParseError> {
    let (mut span, name) = parse_id
        .preceded_by(tuple((
            whitespace0,
            tag_no_case("server"),
            whitespace1,
            tag_no_case("cluster"),
            whitespace1,
        )))
        .terminated(tuple((whitespace0, tag("{"))))
        .parse(span)?;

    let mut attributes = Vec::new();
    let mut commands = Vec::new();
    let mut events = Vec::new();

    let mut deepest_error = DeepestError::new();

    loop {
        let (mut rest, _) = whitespace0.parse(span)?;

        if let Ok((tail, a)) = deepest_error.intercept(attribute_instantiation(rest)) {
            attributes.push(a);
            rest = tail;
        } else if let Ok((tail, cmd)) = deepest_error.intercept(
            parse_id
                .preceded_by(tuple((
                    tag_no_case("handle"),
                    whitespace1,
                    tag_no_case("command"),
                    whitespace1,
                )))
                .terminated(tuple((whitespace0, tag(";"))))
                .parse(rest),
        ) {
            commands.push(cmd);
            rest = tail;
        } else if let Ok((tail, e)) = deepest_error.intercept(
            parse_id
                .preceded_by(tuple((
                    tag_no_case("emits"),
                    whitespace1,
                    tag_no_case("event"),
                    whitespace1,
                )))
                .terminated(tuple((whitespace0, tag(";"))))
                .parse(rest),
        ) {
            events.push(e);
            rest = tail;
        } else {
            break;
        }
        span = rest;
    }

    let result = deepest_error.intercept(
        value(
            ClusterInstantiation {
                name,
                attributes,
                commands,
                events,
            },
            tuple((whitespace0, tag("}"))),
        )
        .parse(span),
    );

    match result {
        Ok(_) => result,
        Err(e) => Err(deepest_error.or(e)),
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct Endpoint<'a> {
    id: u64,
    device_types: Vec<DeviceType<'a>>,
    bindings: Vec<&'a str>,
    instantiations: Vec<ClusterInstantiation<'a>>,
}

pub fn endpoint(span: Span) -> IResult<Span, Endpoint<'_>, ParseError> {
    let (mut span, id) = positive_integer
        .preceded_by(tuple((whitespace0, tag_no_case("endpoint"), whitespace1)))
        .terminated(tuple((whitespace0, tag("{"))))
        .parse(span)?;

    let mut device_types = Vec::new();
    let mut instantiations = Vec::new();
    let mut bindings = Vec::new();

    let mut deepest_error = DeepestError::new();

    loop {
        // eat any whitespace, then try to parse some content
        let (rest, _) = whitespace0.parse(span)?;

        if let Ok((tail, dt)) = deepest_error.intercept(device_type.parse(rest)) {
            device_types.push(dt);
            span = tail;
        } else if let Ok((tail, b)) = deepest_error.intercept(
            parse_id
                .preceded_by(tuple((
                    whitespace0,
                    tag_no_case("binding"),
                    whitespace1,
                    tag_no_case("cluster"),
                    whitespace1,
                )))
                .terminated(tuple((whitespace0, tag(";"))))
                .parse(rest),
        ) {
            bindings.push(b);
            span = tail;
        } else if let Ok((tail, ci)) = deepest_error.intercept(cluster_instantiation(rest)) {
            instantiations.push(ci);
            span = tail;
        } else {
            span = rest;
            break;
        }
    }

    match value(
        Endpoint {
            id,
            device_types,
            instantiations,
            bindings,
        },
        tuple((whitespace0, tag("}"))),
    )
    .parse(span)
    {
        Ok(x) => Ok(x),
        Err(e) => Err(deepest_error.or(e)),
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct Idl<'a> {
    pub clusters: Vec<Cluster<'a>>,
    pub endpoints: Vec<Endpoint<'a>>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum InternalIdlParsedData<'a> {
    Cluster(Cluster<'a>),
    Endpoint(Endpoint<'a>),
    Whitespace,
}

#[derive(Error, Debug, Diagnostic)]
#[error("Failed to parse IDL.")]
#[diagnostic(
    code("matter::idl::parse::failure"),
    help("Failed to parse IDL. Check IDL format")
)]
pub struct IdlParsingError {
    #[source_code]
    pub src: NamedSource,

    #[label("Parse error location")]
    pub error_location: SourceSpan,
}

impl IdlParsingError {
    fn from<'a>(input: Span<'a>, span: Span<'a>, error: nom::Err<ParseError<'a>>) -> Self {
        let pos = match error.depest_index() {
            None => input.len() - span.len(),
            Some(error_pos) => error_pos,
        };

        warn!("IDL parse error: {:?}", error);

        IdlParsingError {
            src: NamedSource::new("input idl", input.fragment().to_string()),
            error_location: (pos, 1).into(),
        }
    }
}

impl Idl<'_> {
    pub fn parse(input: Span) -> Result<Idl, IdlParsingError> {
        let mut idl = Idl::default();

        let mut span = input;
        while !span.is_empty() {
            let (rest, r) = alt((
                Cluster::parse.map(InternalIdlParsedData::Cluster),
                endpoint.map(InternalIdlParsedData::Endpoint),
                value(InternalIdlParsedData::Whitespace, whitespace1),
            ))
            .parse(span)
            .map_err(|e| IdlParsingError::from(input, span, e))?;

            match r {
                InternalIdlParsedData::Cluster(c) => idl.clusters.push(c),
                InternalIdlParsedData::Endpoint(e) => idl.endpoints.push(e),
                InternalIdlParsedData::Whitespace => (),
            }
            span = rest;
        }

        Ok(idl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn remove_loc<'a, O>(
        src: IResult<Span<'a>, O, ParseError<'a>>,
    ) -> IResult<Span<'a>, O, ParseError<'a>> {
        src.map(|(span, o)| ((*span.fragment()).into(), o))
    }

    fn assert_parse_ok<R: PartialEq + std::fmt::Debug>(
        parsed: IResult<Span, R, ParseError>,
        expected: R,
    ) {
        let actual = parsed.expect("Parse should have succeeded").1;
        assert_eq!(actual, expected);
    }
    #[rstest]
    #[case("ram      attribute description default = \"B3\";",
           AttributeInstantiation{
               handle_type: AttributeHandlingType::Ram,
               name: "description",
               default: Some(DefaultAttributeValue::String("B3".into())),
           })]
    #[case(
        "ram      attribute batChargeLevel default = 0x123;",
        AttributeInstantiation{
               handle_type: AttributeHandlingType::Ram,
               name: "batChargeLevel",
               default: Some(DefaultAttributeValue::Number(0x123)),
           })]
    #[case(
        "ram      attribute batReplacementNeeded;",
        AttributeInstantiation{
               handle_type: AttributeHandlingType::Ram,
               name: "batReplacementNeeded",
               default: None,
         })]
    #[case("callback attribute endpointList;",
           AttributeInstantiation{
               handle_type: AttributeHandlingType::Callback,
               name: "endpointList",
               default: None,
         })]
    fn test_parse_attribute_instantiation(
        #[case] input: &str,
        #[case] expected: AttributeInstantiation<'_>,
    ) {
        assert_parse_ok(attribute_instantiation(input.into()), expected);
    }

    #[rstest]
    #[case("default = 1", DefaultAttributeValue::Number(1))]
    #[case("default = 0x1234abcd", DefaultAttributeValue::Number(0x1234abcd))]
    #[case(r#"default = """#, DefaultAttributeValue::String("".into()))]
    #[case("default = \"B3\"", DefaultAttributeValue::String("B3".into()))]
    #[case(r#"default = "test""#, DefaultAttributeValue::String("test".into()))]
    #[case(r#"default = "test\\test""#, DefaultAttributeValue::String("test\\test".into()))]
    #[case("default = \"escaped\\\\and quote\\\"\"", DefaultAttributeValue::String("escaped\\and quote\"".into()))]
    #[case("default = -1", DefaultAttributeValue::Signed(-1))]
    #[case("default = -100", DefaultAttributeValue::Signed(-100))]
    #[case("default = true", DefaultAttributeValue::Bool(true))]
    #[case("default = false", DefaultAttributeValue::Bool(false))]
    fn test_parse_default_attribute_value(
        #[case] input: &str,
        #[case] expected: DefaultAttributeValue,
    ) {
        assert_parse_ok(default_attribute_value(input.into()), expected);
    }

    #[rstest]
    #[case(
        "device type ma_rootdevice = 22, version 1;",
        DeviceType {name: "ma_rootdevice", code: 22, version: 1}
    )]
    #[case(
        "device type ma_powersource = 17, version 2;",
            DeviceType {name: "ma_powersource", code: 17, version: 2}
    )]
    #[case(
        "dEVICe tYPe
           ma_secondary_network_commissioning = //large number on next line
           0xFFF10002, version 0x123  /*test*/
        ;
        ",
        DeviceType {name: "ma_secondary_network_commissioning", code: 0xfff10002 , version: 0x123}
    )]
    fn test_parse_device_type(#[case] input: &str, #[case] expected: DeviceType) {
        assert_parse_ok(device_type(input.into()), expected);
    }

    #[test]
    fn test_cluster_instantiation() {
        assert_parse_ok(
            cluster_instantiation(
                "
                server cluster Test {
                  emits event Foo;
                  emits event Bar;

                  callback attribute attributeList;
                  ram      attribute clusterRevision default = 0x0002;

                  handle command TestEventTrigger;
                  handle command TimeSnapshot;
                }
        "
                .into(),
            ),
            ClusterInstantiation {
                name: "Test",
                attributes: vec![
                    AttributeInstantiation {
                        handle_type: AttributeHandlingType::Callback,
                        name: "attributeList",
                        default: None,
                    },
                    AttributeInstantiation {
                        handle_type: AttributeHandlingType::Ram,
                        name: "clusterRevision",
                        default: Some(DefaultAttributeValue::Number(2)),
                    },
                ],
                commands: vec!["TestEventTrigger", "TimeSnapshot"],
                events: vec!["Foo", "Bar"],
            },
        );
    }

    #[test]
    fn test_parse_attribute_handling() {
        assert!(attribute_handling_type("xyz".into()).is_err());

        assert_parse_ok(
            attribute_handling_type("ram".into()),
            AttributeHandlingType::Ram,
        );
        assert_parse_ok(
            attribute_handling_type("persist".into()),
            AttributeHandlingType::Persist,
        );
        assert_parse_ok(
            attribute_handling_type("callback".into()),
            AttributeHandlingType::Callback,
        );
    }

    #[test]
    fn test_parse_cluster() {
        assert_parse_ok(Cluster::parse("
          /** This is totally made up */
          internal cluster MyTestCluster = 0x123 {
             revision 22; // just for testing

             info event StateChanged = 0 {
               int16u actionID = 0;
             }

             bitmap Feature : bitmap32 {
               kCalendarFormat = 0x1;
             }

             enum ApplyUpdateActionEnum : enum8 {
               kProceed = 0;
               kAwaitNextAction = 1;
               kDiscontinue = 2;
             }

             response struct CommissioningCompleteResponse = 5 {
               char_string debugText = 1;
             }

             readonly attribute attrib_id attributeList[] = 65531;
             fabric command access(invoke: administer) CommissioningComplete(): CommissioningCompleteResponse = 4;
          }
        ".into()), Cluster {
            doc_comment: Some(" This is totally made up "),
            maturity: ApiMaturity::INTERNAL,
            id: "MyTestCluster",
            code: 0x123,
            revision: 22,
            enums: vec![
                Enum {
                    doc_comment: None,
                    maturity: ApiMaturity::STABLE,
                    id: "ApplyUpdateActionEnum",
                    base_type: "enum8",
                    entries: vec![
                        ConstantEntry { maturity: ApiMaturity::STABLE, id: "kProceed", code: 0 },
                        ConstantEntry { maturity: ApiMaturity::STABLE, id: "kAwaitNextAction", code: 1 },
                        ConstantEntry { maturity: ApiMaturity::STABLE, id: "kDiscontinue", code: 2 },
                    ]
               },
            ],
            attributes: vec![Attribute {
                field: StructField {
                    field: Field { data_type: DataType::list_of("attrib_id"), id: "attributeList", code: 65531 },
                    ..Default::default()
                },
                read_acl: AccessPrivilege::View,
                write_acl: AccessPrivilege::Operate,
                is_read_only: true,
                ..Default::default()
            }],
            commands: vec![
                Command {
                    access: AccessPrivilege::Administer,
                    id: "CommissioningComplete",
                    output: "CommissioningCompleteResponse",
                    code: 4,
                    is_fabric_scoped: true,
                    ..Default::default()
            }],
            structs: vec![
                Struct {
                    doc_comment: None,
                    maturity: ApiMaturity::STABLE,
                    struct_type: StructType::Response(5),
                    id: "CommissioningCompleteResponse",
                    fields: vec![StructField {
                        field: Field {
                            data_type: DataType::scalar("char_string"),
                            id: "debugText",
                            code: 1
                        },
                        ..Default::default()
                    }],
                    is_fabric_scoped: false,
                }
            ],
            bitmaps: vec![Bitmap {
                doc_comment: None,
                maturity: ApiMaturity::STABLE,
                id: "Feature",
                base_type: "bitmap32",
                entries: vec![ConstantEntry { maturity: ApiMaturity::STABLE, id: "kCalendarFormat", code: 1 }] }],
            events: vec![Event {
                doc_comment: None,
                maturity: ApiMaturity::STABLE,
                priority: EventPriority::Info,
                access: AccessPrivilege::View,
                id: "StateChanged",
                code: 0,
                fields: vec![
                    StructField {
                        field: Field { data_type: DataType::scalar("int16u") , id: "actionID", code: 0 },
                        ..Default::default()
                    }
                ],
                is_fabric_sensitive: false,
            }],
            ..Default::default()
        });
    }

    #[test]
    fn test_parse_attribute() {
        assert_parse_ok(
            Attribute::parse("attribute int16u identifyTime = 123;".into()),
            Attribute {
                doc_comment: None,
                maturity: ApiMaturity::STABLE,
                field: StructField {
                    field: Field {
                        data_type: DataType::scalar("int16u"),
                        id: "identifyTime",
                        code: 123,
                    },
                    maturity: ApiMaturity::STABLE,
                    is_optional: false,
                    is_nullable: false,
                    is_fabric_sensitive: false,
                },
                read_acl: AccessPrivilege::View,
                write_acl: AccessPrivilege::Operate,
                is_read_only: false,
                is_no_subscribe: false,
                is_timed_write: false,
            },
        );
        assert_parse_ok(
            Attribute::parse(
                "
            /**mix of tests*/
            internal timedwrite
               attribute
               access(read: manage, write: administer)
               optional boolean x[] = 0x123
            ;"
                .into(),
            ),
            Attribute {
                doc_comment: Some("mix of tests"),
                maturity: ApiMaturity::INTERNAL,
                field: StructField {
                    field: Field {
                        data_type: DataType::list_of("boolean"),
                        id: "x",
                        code: 0x123,
                    },
                    maturity: ApiMaturity::STABLE,
                    is_optional: true,
                    is_nullable: false,
                    is_fabric_sensitive: false,
                },
                read_acl: AccessPrivilege::Manage,
                write_acl: AccessPrivilege::Administer,
                is_read_only: false,
                is_no_subscribe: false,
                is_timed_write: true,
            },
        );
    }

    #[test]
    fn test_parse_command() {
        assert_parse_ok(
            Command::parse("
            /** Test with many options. */
            internal fabric timed command access(invoke: administer) GetSetupPIN(GetSetupPINRequest): GetSetupPINResponse = 0;
            ".into()),
            Command {
                doc_comment: Some(" Test with many options. "),
                maturity: ApiMaturity::INTERNAL,
                access: AccessPrivilege::Administer,
                id: "GetSetupPIN",
                input: Some("GetSetupPINRequest"),
                output: "GetSetupPINResponse",
                code: 0,
                is_timed: true,
                is_fabric_scoped: true,
            });

        assert_parse_ok(
            Command::parse("command TestVeryBasic(): DefaultSuccess = 0x123;".into()),
            Command {
                doc_comment: None,
                maturity: ApiMaturity::STABLE,
                access: AccessPrivilege::Operate,
                id: "TestVeryBasic",
                input: None,
                output: "DefaultSuccess",
                code: 0x123,
                is_timed: false,
                is_fabric_scoped: false,
            },
        );
    }

    #[test]
    fn test_parse_event() {
        assert_parse_ok(
            Event::parse(
                "
              /** this is a catch-all */
              fabric_sensitive info event access(read: administer) AccessControlEntryChanged = 0 {
                nullable node_id adminNodeID = 1;
                // !! NOTE More things excluded from the real bits, just to have some test
                fabric_idx fabricIndex = 254;
              }"
                .into(),
            ),
            Event {
                doc_comment: Some(" this is a catch-all "),
                maturity: ApiMaturity::STABLE,
                priority: EventPriority::Info,
                access: AccessPrivilege::Administer,
                id: "AccessControlEntryChanged",
                code: 0,
                is_fabric_sensitive: true,
                fields: vec![
                    StructField {
                        field: Field {
                            data_type: DataType::scalar("node_id"),
                            id: "adminNodeID",
                            code: 1,
                        },
                        maturity: ApiMaturity::STABLE,
                        is_optional: false,
                        is_nullable: true,
                        is_fabric_sensitive: false,
                    },
                    StructField {
                        field: Field {
                            data_type: DataType::scalar("fabric_idx"),
                            id: "fabricIndex",
                            code: 254,
                        },
                        maturity: ApiMaturity::STABLE,
                        is_optional: false,
                        is_nullable: false,
                        is_fabric_sensitive: false,
                    },
                ],
            },
        );
    }

    #[test]
    fn test_parse_event_priority() {
        assert!(event_priority("xyz".into()).is_err());
        assert!(event_priority("FooBar".into()).is_err());
        assert!(event_priority("MaybeView".into()).is_err());

        // does NOT consume whitespace
        assert!(event_priority("   info".into()).is_err());
        assert!(event_priority("   debug   ".into()).is_err());

        assert_parse_ok(event_priority("info".into()), EventPriority::Info);
        assert_parse_ok(event_priority("debug".into()), EventPriority::Debug);
        assert_parse_ok(event_priority("criTICal".into()), EventPriority::Critical);
    }

    #[test]
    fn test_parse_access_privilege() {
        assert!(access_privilege("xyz".into()).is_err());
        assert!(access_privilege("FooBar".into()).is_err());
        assert!(access_privilege("MaybeView".into()).is_err());

        // does NOT consume whitespace
        assert!(access_privilege("   view".into()).is_err());
        assert!(access_privilege("   manage   ".into()).is_err());

        assert_parse_ok(access_privilege("view".into()), AccessPrivilege::View);
        assert_parse_ok(access_privilege("operate".into()), AccessPrivilege::Operate);
        assert_parse_ok(access_privilege("ManaGe".into()), AccessPrivilege::Manage);
        assert_parse_ok(
            access_privilege("adminisTER".into()),
            AccessPrivilege::Administer,
        );
    }

    #[test]
    fn test_parse_struct() {
        assert_parse_ok(
            Struct::parse(
                "
              struct ExtensionFieldSet {
                cluster_id clusterID = 0;
                AttributeValuePair attributeValueList[] = 1;
              }"
                .into(),
            ),
            Struct {
                doc_comment: None,
                maturity: ApiMaturity::STABLE,
                struct_type: StructType::Regular,
                id: "ExtensionFieldSet",
                fields: vec![
                    StructField {
                        field: Field {
                            data_type: DataType::scalar("cluster_id"),
                            id: "clusterID",
                            code: 0,
                        },
                        maturity: ApiMaturity::STABLE,
                        is_optional: false,
                        is_nullable: false,
                        is_fabric_sensitive: false,
                    },
                    StructField {
                        field: Field {
                            data_type: DataType::list_of("AttributeValuePair"),
                            id: "attributeValueList",
                            code: 1,
                        },
                        maturity: ApiMaturity::STABLE,
                        is_optional: false,
                        is_nullable: false,
                        is_fabric_sensitive: false,
                    },
                ],
                is_fabric_scoped: false,
            },
        );
        assert_parse_ok(
            Struct::parse(
                "
                 request struct TestEventTriggerRequest {
                   octet_string<16> enableKey = 0;
                   int64u eventTrigger = 1;
                 }"
                .into(),
            ),
            Struct {
                doc_comment: None,
                maturity: ApiMaturity::STABLE,
                struct_type: StructType::Request,
                id: "TestEventTriggerRequest",
                fields: vec![
                    StructField {
                        field: Field {
                            data_type: DataType::scalar_of_size("octet_string", 16),
                            id: "enableKey",
                            code: 0,
                        },
                        maturity: ApiMaturity::STABLE,
                        is_optional: false,
                        is_nullable: false,
                        is_fabric_sensitive: false,
                    },
                    StructField {
                        field: Field {
                            data_type: DataType::scalar("int64u"),
                            id: "eventTrigger",
                            code: 1,
                        },
                        maturity: ApiMaturity::STABLE,
                        is_optional: false,
                        is_nullable: false,
                        is_fabric_sensitive: false,
                    },
                ],
                is_fabric_scoped: false,
            },
        );

        assert_parse_ok(
            Struct::parse(
                "
                 /** this tests responses */
                 internal response struct TimeSnapshotResponse = 2 {
                   systime_us systemTimeUs = 0;
                   nullable epoch_us UTCTimeUs = 1;
                 }"
                .into(),
            ),
            Struct {
                doc_comment: Some(" this tests responses "),
                maturity: ApiMaturity::INTERNAL,
                struct_type: StructType::Response(2),
                id: "TimeSnapshotResponse",
                fields: vec![
                    StructField {
                        field: Field {
                            data_type: DataType {
                                name: "systime_us",
                                is_list: false,
                                max_length: None,
                            },
                            id: "systemTimeUs",
                            code: 0,
                        },
                        maturity: ApiMaturity::STABLE,
                        is_optional: false,
                        is_nullable: false,
                        is_fabric_sensitive: false,
                    },
                    StructField {
                        field: Field {
                            data_type: DataType {
                                name: "epoch_us",
                                is_list: false,
                                max_length: None,
                            },
                            id: "UTCTimeUs",
                            code: 1,
                        },
                        maturity: ApiMaturity::STABLE,
                        is_optional: false,
                        is_nullable: true,
                        is_fabric_sensitive: false,
                    },
                ],
                is_fabric_scoped: false,
            },
        );

        assert_parse_ok(
            Struct::parse(
                "fabric_scoped struct ProviderLocation {
                   node_id providerNodeID = 1;
                   endpoint_no endpoint = 2;
                   fabric_idx fabricIndex = 254;
                 }"
                .into(),
            ),
            Struct {
                doc_comment: None,
                maturity: ApiMaturity::STABLE,
                struct_type: StructType::Regular,
                id: "ProviderLocation",
                fields: vec![
                    StructField {
                        field: Field {
                            data_type: DataType::scalar("node_id"),
                            id: "providerNodeID",
                            code: 1,
                        },
                        ..Default::default()
                    },
                    StructField {
                        field: Field {
                            data_type: DataType::scalar("endpoint_no"),
                            id: "endpoint",
                            code: 2,
                        },
                        ..Default::default()
                    },
                    StructField {
                        field: Field {
                            data_type: DataType::scalar("fabric_idx"),
                            id: "fabricIndex",
                            code: 254,
                        },
                        ..Default::default()
                    },
                ],
                is_fabric_scoped: true,
            },
        );
    }

    #[test]
    fn test_parse_struct_field() {
        assert_parse_ok(
            StructField::parse("int8u sceneCount = 0;".into()),
            StructField {
                field: Field {
                    data_type: DataType::scalar("int8u"),
                    id: "sceneCount",
                    code: 0,
                },
                maturity: ApiMaturity::STABLE,
                is_optional: false,
                is_nullable: false,
                is_fabric_sensitive: false,
            },
        );
        assert_parse_ok(
            StructField::parse("fabric_sensitive int8u currentScene = 1;".into()),
            StructField {
                field: Field {
                    data_type: DataType::scalar("int8u"),
                    id: "currentScene",
                    code: 1,
                },
                maturity: ApiMaturity::STABLE,
                is_optional: false,
                is_nullable: false,
                is_fabric_sensitive: true,
            },
        );
        assert_parse_ok(
            StructField::parse(
                "optional nullable ExtensionFieldSet extensionFieldSets[] = 5;".into(),
            ),
            StructField {
                field: Field {
                    data_type: DataType::list_of("ExtensionFieldSet"),
                    id: "extensionFieldSets",
                    code: 5,
                },
                maturity: ApiMaturity::STABLE,
                is_optional: true,
                is_nullable: true,
                is_fabric_sensitive: false,
            },
        );
    }

    #[test]
    fn test_parse_field() {
        assert_parse_ok(
            Field::parse("bool test = 1".into()),
            Field {
                data_type: DataType::scalar("bool"),
                id: "test",
                code: 1,
            },
        );
        assert_parse_ok(
            Field::parse("int32u test[] = 0x12".into()),
            Field {
                data_type: DataType::list_of("int32u"),
                id: "test",
                code: 0x12,
            },
        );
        assert_parse_ok(
            Field::parse("octet_string<123> other=10".into()),
            Field {
                data_type: DataType::scalar_of_size("octet_string", 123),
                id: "other",
                code: 10,
            },
        );
    }

    #[test]
    fn test_parse_enum() {
        assert_parse_ok(
            Enum::parse(
                "
  enum EffectIdentifierEnum : enum8 {
    kBlink = 0;
    kBreathe = 1;
    kOkay = 2;
    kChannelChange = 11;
    kFinishEffect = 254;
    kStopEffect = 255;
  }"
                .into(),
            ),
            Enum {
                doc_comment: None,
                maturity: ApiMaturity::STABLE,
                id: "EffectIdentifierEnum",
                base_type: "enum8",
                entries: vec![
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kBlink",
                        code: 0,
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kBreathe",
                        code: 1,
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kOkay",
                        code: 2,
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kChannelChange",
                        code: 11,
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kFinishEffect",
                        code: 254,
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kStopEffect",
                        code: 255,
                    },
                ],
            },
        );
    }

    #[test]
    fn test_parse_bitmap() {
        assert_eq!(
            Bitmap::parse(
                "
  /** Test feature bitmap */
  bitmap Feature : bitmap32 {
    kSceneNames = 0x1;
    kExplicit = 0x2;
    kTableSize = 0x4;
    provisional kFabricScenes = 0x8;
  }"
                .into()
            )
            .expect("valid value")
            .1,
            Bitmap {
                doc_comment: Some(" Test feature bitmap "),
                maturity: ApiMaturity::STABLE,
                id: "Feature",
                base_type: "bitmap32",
                entries: vec![
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kSceneNames",
                        code: 0x01
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kExplicit",
                        code: 0x02
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kTableSize",
                        code: 0x04
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::PROVISIONAL,
                        id: "kFabricScenes",
                        code: 0x08
                    },
                ]
            }
        );
    }

    #[test]
    fn test_parse_constant_entry_list() {
        assert_eq!(
            remove_loc(constant_entries_list("{}".into())),
            Ok(("".into(), vec![]))
        );
        assert_eq!(
            remove_loc(constant_entries_list(
                "{ a = 1; provisional b = 2; }".into()
            )),
            Ok((
                "".into(),
                vec![
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "a",
                        code: 1
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::PROVISIONAL,
                        id: "b",
                        code: 2
                    },
                ]
            ))
        );
        assert_eq!(
            remove_loc(constant_entries_list(
                "{
                // Comment
                kConstantOne = 123;
                internal kAnother = 0x23abc /* this tests hex */;
            }suffix"
                    .into()
            )),
            Ok((
                "suffix".into(),
                vec![
                    ConstantEntry {
                        maturity: ApiMaturity::STABLE,
                        id: "kConstantOne",
                        code: 123
                    },
                    ConstantEntry {
                        maturity: ApiMaturity::INTERNAL,
                        id: "kAnother",
                        code: 0x23abc
                    },
                ]
            ))
        );
    }

    #[test]
    fn test_parse_maturity() {
        assert_eq!(
            api_maturity("123".into()),
            Ok(("123".into(), ApiMaturity::STABLE))
        );
        assert_eq!(
            remove_loc(api_maturity("stable abc".into())),
            Ok((" abc".into(), ApiMaturity::STABLE))
        );
        assert_eq!(
            remove_loc(api_maturity("provisional abc".into())),
            Ok((" abc".into(), ApiMaturity::PROVISIONAL))
        );
        assert_eq!(
            remove_loc(api_maturity("internal xyz".into())),
            Ok((" xyz".into(), ApiMaturity::INTERNAL))
        );
        assert_eq!(
            remove_loc(api_maturity("deprecated foobar".into())),
            Ok((" foobar".into(), ApiMaturity::DEPRECATED))
        );

        assert_eq!(
            remove_loc(api_maturity("DepreCAteD CaseTest".into())),
            Ok((" CaseTest".into(), ApiMaturity::DEPRECATED))
        );
    }

    #[test]
    fn test_hex_integer() {
        assert!(hex_integer("".into()).is_err());
        assert!(hex_integer("123".into()).is_err());
        assert!(hex_integer("0xzzz".into()).is_err());
        assert!(hex_integer("0x".into()).is_err());

        assert_eq!(
            remove_loc(hex_integer("0x12 abc".into())),
            Ok((" abc".into(), 0x12))
        );
        assert_eq!(
            remove_loc(hex_integer("0XABC XYZ".into())),
            Ok((" XYZ".into(), 0xABC))
        );
    }

    #[test]
    fn test_parse_decimal() {
        assert!(decimal_integer("a".into()).is_err());
        assert!(decimal_integer("".into()).is_err());

        assert_eq!(
            remove_loc(decimal_integer("123".into())),
            Ok(("".into(), 123))
        );
        assert_eq!(
            remove_loc(decimal_integer("1 2 3".into())),
            Ok((" 2 3".into(), 1))
        );
        assert_eq!(
            remove_loc(decimal_integer("0x123".into())),
            Ok(("x123".into(), 0))
        );
    }

    #[test]
    fn test_positive_integer() {
        assert!(positive_integer("a".into()).is_err());
        assert!(positive_integer("".into()).is_err());

        assert_eq!(
            remove_loc(positive_integer("123".into())),
            Ok(("".into(), 123))
        );
        assert_eq!(
            remove_loc(positive_integer("1 2 3".into())),
            Ok((" 2 3".into(), 1))
        );
        assert_eq!(
            remove_loc(positive_integer("0x123".into())),
            Ok(("".into(), 0x123))
        );
        assert_eq!(
            remove_loc(positive_integer("12ab".into())),
            Ok(("ab".into(), 12))
        );
        assert_eq!(
            remove_loc(positive_integer("0x12abcxyz".into())),
            Ok(("xyz".into(), 0x12abc))
        );
    }

    #[test]
    fn test_whitespace_group() {
        assert!(whitespace_group("a".into()).is_err());
        assert!(whitespace_group("".into()).is_err());

        assert_eq!(
            remove_loc(whitespace_group("   abc".into())),
            Ok(("abc".into(), Whitespace::Whitespace("   ")))
        );
        assert_eq!(
            remove_loc(whitespace_group("/* cpp comment */rest of text".into())),
            Ok((
                "rest of text".into(),
                Whitespace::CppComment(" cpp comment ")
            ))
        );
        assert_eq!(
            remove_loc(whitespace_group("/** Doc comment */rest of text".into())),
            Ok((
                "rest of text".into(),
                Whitespace::DocComment(" Doc comment ")
            ))
        );

        // only one (first) whitespace is removed
        assert_eq!(
            remove_loc(whitespace_group("//test   \nxyz".into())),
            Ok(("\nxyz".into(), Whitespace::CComment("test   ")))
        );
        assert_eq!(
            remove_loc(whitespace_group("  \n//test   \nxyz".into())),
            Ok(("//test   \nxyz".into(), Whitespace::Whitespace("  \n")))
        );
    }

    #[test]
    fn test_whitespace_group1() {
        assert!(whitespace1("a".into()).is_err());
        assert!(whitespace1("".into()).is_err());

        assert_eq!(
            remove_loc(whitespace1("//test\n123".into())),
            Ok(("123".into(), None))
        );
        assert_eq!(
            remove_loc(whitespace1("//test\n/*cpp */  \t  \t\r\n123".into())),
            Ok(("123".into(), None))
        );

        // doc comments are extracted
        assert_eq!(
            remove_loc(whitespace1("//test\n/** Comment! */123".into())),
            Ok(("123".into(), Some(DocComment(" Comment! "))))
        );
        assert_eq!(
            remove_loc(whitespace1("//test\n/** Comment! */\n\n  \n\n123".into())),
            Ok(("123".into(), Some(DocComment(" Comment! "))))
        );
        assert_eq!(
            remove_loc(whitespace1("/** Comment! *///separated\n123".into())),
            Ok(("123".into(), None))
        );
        assert_eq!(
            remove_loc(whitespace1("/** Comment! *//*separated*/123".into())),
            Ok(("123".into(), None))
        );
    }

    #[test]
    fn test_whitespace_group0() {
        assert_eq!(remove_loc(whitespace0("a".into())), Ok(("a".into(), None)));
        assert_eq!(remove_loc(whitespace0("".into())), Ok(("".into(), None)));
        assert_eq!(
            remove_loc(whitespace0("//test\n/** Comment! */123".into())),
            Ok(("123".into(), Some(DocComment(" Comment! "))))
        );
        assert_eq!(
            remove_loc(whitespace0("/** Comment! *//*separated*/123".into())),
            Ok(("123".into(), None))
        );
    }

    #[test]
    fn test_parse_id() {
        assert!(parse_id("  xyz".into()).is_err());
        assert!(parse_id("/".into()).is_err());
        assert!(parse_id("#test".into()).is_err());
        assert!(parse_id("123abc".into()).is_err());

        assert_eq!(
            remove_loc(parse_id("abc123 other".into())),
            Ok((" other".into(), "abc123"))
        );
        assert_eq!(
            remove_loc(parse_id("this_is_a_test and more data".into())),
            Ok((" and more data".into(), "this_is_a_test"))
        );
        assert_eq!(
            remove_loc(parse_id("_Test 123".into())),
            Ok((" 123".into(), "_Test"))
        );
    }

    #[test]
    fn test_parse_constant_entry() {
        assert!(ConstantEntry::parse("abc".into()).is_err());
        assert!(ConstantEntry::parse("a = 1".into()).is_err());
        assert!(ConstantEntry::parse("a = ;".into()).is_err());
        assert!(ConstantEntry::parse("provisional a = ;".into()).is_err());

        assert_eq!(
            remove_loc(ConstantEntry::parse("a=0;".into())),
            Ok((
                "".into(),
                ConstantEntry {
                    id: "a",
                    code: 0,
                    maturity: ApiMaturity::STABLE
                }
            ))
        );

        assert_eq!(
            remove_loc(ConstantEntry::parse("   provisional xyz = 0x123 ;".into())),
            Ok((
                "".into(),
                ConstantEntry {
                    id: "xyz",
                    code: 0x123,
                    maturity: ApiMaturity::PROVISIONAL
                }
            ))
        );

        assert_eq!(
            remove_loc(ConstantEntry::parse("InterNAL kTest = 0xabc ;".into())),
            Ok((
                "".into(),
                ConstantEntry {
                    id: "kTest",
                    code: 0xABC,
                    maturity: ApiMaturity::INTERNAL
                }
            ))
        );
        assert_eq!(
            remove_loc(ConstantEntry::parse(
                "
                internal
                kTest\t
                     =
                      0xabc

                       ;"
                .into()
            )),
            Ok((
                "".into(),
                ConstantEntry {
                    id: "kTest",
                    code: 0xABC,
                    maturity: ApiMaturity::INTERNAL
                }
            ))
        );
        assert_eq!(
            remove_loc(ConstantEntry::parse(
                "
            /*comment*/ internal
            //test
            kTest //more comments
                 = /*test*/
                   // and more
                  0xabc //test ;;; these are in comments ;;;
                  ;"
                .into()
            )),
            Ok((
                "".into(),
                ConstantEntry {
                    id: "kTest",
                    code: 0xABC,
                    maturity: ApiMaturity::INTERNAL
                }
            ))
        );
    }
}
