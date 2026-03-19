use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{line_ending, space1},
    combinator::map,
    sequence::terminated,
};

#[derive(Debug, PartialEq, Clone)]
pub enum ImapResponse {
    /// Responses starting with '*'
    Untagged { data: String },
    /// Responses starting with your tag (e.g., 'A1')
    Tagged {
        tag: String,
        status: Status, // OK, NO, BAD
        message: String,
    },
    /// Responses starting with '+'
    Continuation(String),
}

#[derive(Debug, PartialEq, Clone)]
pub enum Status {
    Ok,
    No,
    Bad,
}

/// Parses a status string like "OK", "NO", or "BAD"
fn parse_status(input: &str) -> IResult<&str, Status> {
    alt((
        map(tag("OK"), |_| Status::Ok),
        map(tag("NO"), |_| Status::No),
        map(tag("BAD"), |_| Status::Bad),
    )).parse(input)
}

/// Parses a tagged response: "A1 OK Success\r\n"
fn parse_tagged(input: &str) -> IResult<&str, ImapResponse> {
    let (input, (id, _, status, _, msg)) = (
        take_while1(|c: char| c.is_alphanumeric()), // Tag: A1
        space1,
        parse_status, // Status: OK
        space1,
        take_until("\r\n"), // Message: ...
    ).parse(input)?;

    Ok((
        input,
        ImapResponse::Tagged {
            tag: id.to_string(),
            status,
            message: msg.to_string(),
        },
    ))
}

/// Parses an untagged response: "* 22 EXISTS\r\n"
fn parse_untagged(input: &str) -> IResult<&str, ImapResponse> {
    let (input, _) = tag("*")(input)?;
    let (input, _) = space1(input)?;
    let (input, content) = take_until("\r\n")(input)?;

    Ok((
        input,
        ImapResponse::Untagged {
            data: content.to_string(),
        },
    ))
}

/// The main entry point for parsing a single line
pub fn parse_imap_line(input: &str) -> IResult<&str, ImapResponse> {
    terminated(alt((parse_tagged, parse_untagged)), line_ending).parse(input)
}
