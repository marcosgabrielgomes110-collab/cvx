use cvx_core::{Error, Result};

/// Author or committer identity and timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// Unix timestamp in seconds.
    pub timestamp: i64,
    /// Timezone offset in minutes east of UTC.
    pub offset_minutes: i32,
}

impl Signature {
    /// Creates a signature.
    pub fn new(
        name: impl Into<String>,
        email: impl Into<String>,
        timestamp: i64,
        offset_minutes: i32,
    ) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
            timestamp,
            offset_minutes,
        }
    }

    /// Parses `Name <email> 1234567890 +0200`.
    pub fn parse(input: &str) -> Result<Self> {
        let open = input
            .rfind('<')
            .ok_or_else(|| Error::corrupt("signature is missing '<'"))?;
        let close = input[open..]
            .find('>')
            .map(|offset| open + offset)
            .ok_or_else(|| Error::corrupt("signature is missing '>'"))?;

        let name = input[..open].trim_end().to_owned();
        let email = input[open + 1..close].to_owned();
        let rest = input[close + 1..].trim();
        let mut fields = rest.split_whitespace();
        let timestamp = fields
            .next()
            .ok_or_else(|| Error::corrupt("signature is missing a timestamp"))?
            .parse::<i64>()
            .map_err(|_| Error::corrupt("signature timestamp is not an integer"))?;
        let timezone = fields
            .next()
            .ok_or_else(|| Error::corrupt("signature is missing a timezone"))?;
        if fields.next().is_some() {
            return Err(Error::corrupt("signature has unexpected trailing fields"));
        }
        let offset_minutes = parse_offset(timezone)?;

        Ok(Self {
            name,
            email,
            timestamp,
            offset_minutes,
        })
    }

    /// Formats the signature in Git's commit format.
    pub fn to_git_string(&self) -> String {
        format!(
            "{} <{}> {} {}",
            self.name,
            self.email,
            self.timestamp,
            format_offset(self.offset_minutes)
        )
    }

    /// Returns whether the signature contains characters that cannot be stored
    /// safely in a commit header.
    pub fn is_valid(&self) -> bool {
        !self.name.contains(['<', '>', '\n', '\0']) && !self.email.contains(['<', '>', '\n', '\0'])
    }
}

/// Parses a Git timezone such as `+0200` or `-0330`.
pub fn parse_timezone(value: &str) -> Result<i32> {
    parse_offset(value)
}

fn parse_offset(value: &str) -> Result<i32> {
    let bytes = value.as_bytes();
    if bytes.len() != 5 || !matches!(bytes[0], b'+' | b'-') {
        return Err(Error::corrupt(format!(
            "invalid signature timezone: {value}"
        )));
    }
    let digits = std::str::from_utf8(&bytes[1..])
        .map_err(|_| Error::corrupt("signature timezone is not ASCII"))?;
    let hours = digits[..2]
        .parse::<i32>()
        .map_err(|_| Error::corrupt("signature timezone has invalid hours"))?;
    let minutes = digits[2..]
        .parse::<i32>()
        .map_err(|_| Error::corrupt("signature timezone has invalid minutes"))?;
    if minutes >= 60 {
        return Err(Error::corrupt("signature timezone has invalid minutes"));
    }
    let total = hours * 60 + minutes;
    Ok(if bytes[0] == b'-' { -total } else { total })
}

fn format_offset(offset_minutes: i32) -> String {
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let absolute = offset_minutes.unsigned_abs();
    format!("{sign}{:02}{:02}", absolute / 60, absolute % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_signatures() {
        let input = "Jane Doe <jane@example.com> 1700000000 -0330";
        let signature = Signature::parse(input).expect("parse");
        assert_eq!(signature.name, "Jane Doe");
        assert_eq!(signature.email, "jane@example.com");
        assert_eq!(signature.timestamp, 1_700_000_000);
        assert_eq!(signature.offset_minutes, -210);
        assert_eq!(signature.to_git_string(), input);
    }

    #[test]
    fn rejects_bad_offsets() {
        assert!(Signature::parse("A <a@b> 1 +1260").is_err());
        assert!(Signature::parse("A <a@b> 1 0000").is_err());
        assert!(Signature::parse("A <a@b> 1 +00").is_err());
    }

    #[test]
    fn validates_identity_characters() {
        assert!(Signature::new("Jane", "jane@example.com", 0, 0).is_valid());
        assert!(!Signature::new("Jane <x>", "jane@example.com", 0, 0).is_valid());
    }
}
