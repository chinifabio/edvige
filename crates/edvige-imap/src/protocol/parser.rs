use edvige_core::MessageFlags;
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_until, take_while1},
    character::complete::{char, digit1, space0, space1},
    combinator::{map, opt},
    sequence::{delimited, preceded},
    IResult, Parser,
};

use super::response::{
    Continuation, FetchResponse, ImapLine, Status, TaggedResponse, UntaggedResponse,
};

/// Parses status word: OK, NO, BAD, PREAUTH, BYE
pub fn parse_status(input: &str) -> IResult<&str, Status> {
    alt((
        map(tag_no_case("OK"), |_| Status::Ok),
        map(tag_no_case("NO"), |_| Status::No),
        map(tag_no_case("BAD"), |_| Status::Bad),
        map(tag_no_case("PREAUTH"), |_| Status::PreAuth),
        map(tag_no_case("BYE"), |_| Status::Bye),
    ))
    .parse(input)
}

/// Parses an optional status code in brackets: `[UIDVALIDITY 123]`
pub fn parse_status_code(input: &str) -> IResult<&str, &str> {
    delimited(char('['), take_until("]"), char(']')).parse(input)
}

/// Parses a quoted string: `"INBOX"` or `"hello world"`
pub fn parse_quoted_string(input: &str) -> IResult<&str, String> {
    let (input, content) = delimited(char('"'), take_until("\""), char('"')).parse(input)?;
    Ok((input, content.to_string()))
}

/// Parses an atom (unquoted string without spaces or special delimiters)
pub fn parse_atom(input: &str) -> IResult<&str, String> {
    let (input, atom) = take_while1(|c: char| {
        !c.is_whitespace() && c != '(' && c != ')' && c != '{' && c != '}' && c != '%' && c != '*'
    })
    .parse(input)?;
    Ok((input, atom.to_string()))
}

/// Parses a string which could be quoted or an atom or NIL
pub fn parse_astring(input: &str) -> IResult<&str, String> {
    alt((
        parse_quoted_string,
        map(tag_no_case("NIL"), |_| String::new()),
        parse_atom,
    ))
    .parse(input)
}

/// Parses flags list: `(\Seen \Answered \Flagged \Draft \Deleted)`
pub fn parse_flags(input: &str) -> IResult<&str, MessageFlags> {
    let (input, flags_content) = delimited(char('('), take_until(")"), char(')')).parse(input)?;
    let mut flags = MessageFlags::default();

    for part in flags_content.split_whitespace() {
        match part.to_ascii_lowercase().as_str() {
            "\\seen" => flags.seen = true,
            "\\flagged" => flags.flagged = true,
            "\\answered" => flags.answered = true,
            "\\draft" => flags.draft = true,
            "\\deleted" => flags.deleted = true,
            _ => {}
        }
    }

    Ok((input, flags))
}

/// Parses a tagged response line: `A0001 OK [READ-WRITE] SELECT completed`
pub fn parse_tagged(input: &str) -> IResult<&str, TaggedResponse> {
    let (input, tag_id) = take_while1(|c: char| !c.is_whitespace()).parse(input)?;
    let (input, _) = space1(input)?;
    let (input, status) = parse_status(input)?;

    let (input, code) = opt(preceded(space1, parse_status_code)).parse(input)?;
    let (input, _) = space0(input)?;
    let text = input.trim().to_string();

    Ok((
        "",
        TaggedResponse {
            tag: tag_id.to_string(),
            status,
            code: code.map(|c| c.to_string()),
            text,
        },
    ))
}

/// Parses an untagged `* LIST (\HasNoChildren) "/" "INBOX"` response
pub fn parse_list_response(input: &str) -> IResult<&str, UntaggedResponse> {
    let (input, _) = tag_no_case("LIST")(input)?;
    let (input, _) = space1(input)?;

    // Parse flags in parentheses
    let (input, flags_content) = delimited(char('('), take_until(")"), char(')')).parse(input)?;
    let flags: Vec<String> = flags_content
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let (input, _) = space1(input)?;

    // Parse delimiter (can be `"/"`, `"\\"`, or `NIL`)
    let (input, delimiter) = alt((
        map(parse_quoted_string, Some),
        map(tag_no_case("NIL"), |_| None),
        map(parse_atom, |s| Some(s)),
    ))
    .parse(input)?;

    let (input, _) = space1(input)?;
    let (input, name) = parse_astring(input)?;

    Ok((
        input,
        UntaggedResponse::List {
            flags,
            delimiter,
            name,
        },
    ))
}

/// Parses `* 25 EXISTS`
pub fn parse_exists_response(input: &str) -> IResult<&str, UntaggedResponse> {
    let (input, num_str) = digit1(input)?;
    let (input, _) = space1(input)?;
    let (input, _) = tag_no_case("EXISTS")(input)?;
    let num: u32 = num_str.parse().map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
    })?;
    Ok((input, UntaggedResponse::Exists(num)))
}

/// Parses `* 2 RECENT`
pub fn parse_recent_response(input: &str) -> IResult<&str, UntaggedResponse> {
    let (input, num_str) = digit1(input)?;
    let (input, _) = space1(input)?;
    let (input, _) = tag_no_case("RECENT")(input)?;
    let num: u32 = num_str.parse().map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
    })?;
    Ok((input, UntaggedResponse::Recent(num)))
}

/// Parses `* 5 EXPUNGE`
pub fn parse_expunge_response(input: &str) -> IResult<&str, UntaggedResponse> {
    let (input, num_str) = digit1(input)?;
    let (input, _) = space1(input)?;
    let (input, _) = tag_no_case("EXPUNGE")(input)?;
    let num: u32 = num_str.parse().map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
    })?;
    Ok((input, UntaggedResponse::Expunge(num)))
}

/// Parses `* 1 FETCH (UID 123 FLAGS (\Seen) RFC822.SIZE 1024 ...)`
pub fn parse_fetch_header(input: &str) -> IResult<&str, (u32, &str)> {
    let (input, seq_str) = digit1(input)?;
    let (input, _) = space1(input)?;
    let (input, _) = tag_no_case("FETCH")(input)?;
    let (input, _) = space1(input)?;
    let (input, _) = char('(')(input)?;

    let seq: u32 = seq_str.parse().unwrap_or(0);
    Ok(("", (seq, input)))
}

/// Extracts attributes from a fetch attribute line or string
pub fn parse_fetch_attributes(seq: u32, body_content: &str) -> FetchResponse {
    let mut uid = None;
    let mut flags = None;
    let mut rfc822_size = None;

    let mut remaining = body_content;
    while let Some(idx) = remaining.find(|c: char| c.is_alphabetic() || c == '\\') {
        remaining = &remaining[idx..];

        if remaining.to_ascii_uppercase().starts_with("UID ") {
            remaining = &remaining[4..];
            if let Some((num_str, rest)) = split_number(remaining) {
                uid = num_str.parse().ok();
                remaining = rest;
            }
        } else if remaining.to_ascii_uppercase().starts_with("FLAGS ") {
            remaining = &remaining[6..];
            if let Ok((rest, parsed_flags)) = parse_flags(remaining) {
                flags = Some(parsed_flags);
                remaining = rest;
            }
        } else if remaining.to_ascii_uppercase().starts_with("RFC822.SIZE ") {
            remaining = &remaining[12..];
            if let Some((num_str, rest)) = split_number(remaining) {
                rfc822_size = num_str.parse().ok();
                remaining = rest;
            }
        } else {
            // Advance by one token
            if let Some(space_idx) = remaining.find(' ') {
                remaining = &remaining[space_idx + 1..];
            } else {
                break;
            }
        }
    }

    FetchResponse {
        seq,
        uid,
        flags,
        rfc822_size,
        rfc822_body: None,
    }
}

fn split_number(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some((&s[..end], &s[end..]))
    }
}

/// Checks if a line ends with an IMAP literal indicator like `{1234}`
pub fn parse_literal_length(line: &str) -> Option<usize> {
    let trimmed = line.trim_end_matches("\r\n").trim_end();
    if trimmed.ends_with('}') {
        if let Some(open_idx) = trimmed.rfind('{') {
            let num_part = &trimmed[open_idx + 1..trimmed.len() - 1];
            return num_part.parse::<usize>().ok();
        }
    }
    None
}

/// Parses an untagged line starting after `* `
pub fn parse_untagged(input: &str) -> IResult<&str, UntaggedResponse> {
    let (input, _) = tag("*")(input)?;
    let (input, _) = space1(input)?;

    if let Ok((_, res)) = parse_list_response(input) {
        return Ok(("", res));
    }
    if let Ok((_, res)) = parse_exists_response(input) {
        return Ok(("", res));
    }
    if let Ok((_, res)) = parse_recent_response(input) {
        return Ok(("", res));
    }
    if let Ok((_, res)) = parse_expunge_response(input) {
        return Ok(("", res));
    }

    if let Ok((_, (seq, fetch_body))) = parse_fetch_header(input) {
        let fetch_res = parse_fetch_attributes(seq, fetch_body);
        return Ok(("", UntaggedResponse::Fetch(fetch_res)));
    }

    if let Ok((input_rest, status)) = parse_status(input) {
        let (input_rest, code) = opt(preceded(space1, parse_status_code)).parse(input_rest)?;
        let text = input_rest.trim().to_string();
        let code_str = code.map(|c| c.to_string());
        return match status {
            Status::Ok => Ok(("", UntaggedResponse::Ok { code: code_str, text })),
            Status::No => Ok(("", UntaggedResponse::No { code: code_str, text })),
            Status::Bad => Ok(("", UntaggedResponse::Bad { code: code_str, text })),
            _ => Ok(("", UntaggedResponse::Other(input.trim().to_string()))),
        };
    }

    Ok(("", UntaggedResponse::Other(input.trim().to_string())))
}

/// Top level line parser
pub fn parse_line(line: &str) -> Result<ImapLine, nom::Err<nom::error::Error<String>>> {
    let line_trimmed = line.trim_end_matches("\r\n");

    if line_trimmed.starts_with('+') {
        return Ok(ImapLine::Continuation(Continuation(
            line_trimmed[1..].trim().to_string(),
        )));
    }

    if line_trimmed.starts_with('*') {
        match parse_untagged(line_trimmed) {
            Ok((_, res)) => Ok(ImapLine::Untagged(res)),
            Err(_e) => Err(nom::Err::Error(nom::error::Error::new(
                line.to_string(),
                nom::error::ErrorKind::Tag,
            ))),
        }
    } else {
        match parse_tagged(line_trimmed) {
            Ok((_, res)) => Ok(ImapLine::Tagged(res)),
            Err(_e) => Err(nom::Err::Error(nom::error::Error::new(
                line.to_string(),
                nom::error::ErrorKind::Tag,
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tagged_ok() {
        let line = "A001 OK [READ-WRITE] SELECT completed\r\n";
        let parsed = parse_line(line).unwrap();
        match parsed {
            ImapLine::Tagged(t) => {
                assert_eq!(t.tag, "A001");
                assert_eq!(t.status, Status::Ok);
                assert_eq!(t.code.as_deref(), Some("READ-WRITE"));
                assert_eq!(t.text, "SELECT completed");
            }
            _ => panic!("Expected Tagged"),
        }
    }

    #[test]
    fn test_parse_list() {
        let line = "* LIST (\\HasNoChildren \\Drafts) \"/\" \"Drafts\"\r\n";
        let parsed = parse_line(line).unwrap();
        match parsed {
            ImapLine::Untagged(UntaggedResponse::List { flags, delimiter, name }) => {
                assert_eq!(flags, vec!["\\HasNoChildren", "\\Drafts"]);
                assert_eq!(delimiter.as_deref(), Some("/"));
                assert_eq!(name, "Drafts");
            }
            _ => panic!("Expected Untagged List"),
        }
    }

    #[test]
    fn test_parse_exists_and_recent() {
        assert_eq!(
            parse_line("* 120 EXISTS\r\n").unwrap(),
            ImapLine::Untagged(UntaggedResponse::Exists(120))
        );
        assert_eq!(
            parse_line("* 3 RECENT\r\n").unwrap(),
            ImapLine::Untagged(UntaggedResponse::Recent(3))
        );
        assert_eq!(
            parse_line("* 4 EXPUNGE\r\n").unwrap(),
            ImapLine::Untagged(UntaggedResponse::Expunge(4))
        );
    }

    #[test]
    fn test_parse_fetch() {
        let line = "* 5 FETCH (UID 42 FLAGS (\\Seen \\Flagged) RFC822.SIZE 2048)\r\n";
        let parsed = parse_line(line).unwrap();
        match parsed {
            ImapLine::Untagged(UntaggedResponse::Fetch(f)) => {
                assert_eq!(f.seq, 5);
                assert_eq!(f.uid, Some(42));
                assert_eq!(f.rfc822_size, Some(2048));
                let flags = f.flags.unwrap();
                assert!(flags.seen);
                assert!(flags.flagged);
                assert!(!flags.answered);
            }
            _ => panic!("Expected Fetch response"),
        }
    }

    #[test]
    fn test_parse_literal_length() {
        assert_eq!(
            parse_literal_length("* 1 FETCH (RFC822 {1024}\r\n"),
            Some(1024)
        );
        assert_eq!(
            parse_literal_length("* 1 FETCH (FLAGS (\\Seen))\r\n"),
            None
        );
    }
}
