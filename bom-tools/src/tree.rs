use std::io::BufRead;
use std::str::{FromStr, SplitAsciiWhitespace};

pub(crate) struct Dependency {
    pub(crate) id: String,
    pub(crate) version: semver::Version,
}

fn get_package_id(iter: &mut SplitAsciiWhitespace) -> Result<String, Box<dyn std::error::Error>> {
    for next in iter {
        // if next only contains valid characters than it is the id!
        if next
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Ok(next.to_string());
        }
    }
    Err("Line missing package id".into())
}

impl FromStr for Dependency {
    type Err = Box<dyn std::error::Error>;

    /// parse a line into a Dependency
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut iter = s.split_ascii_whitespace();
        let id = get_package_id(&mut iter)?;
        let version = iter.next().ok_or("line missing version")?;
        let version = match version.strip_prefix('v') {
            Some(x) => x,
            None => return Err("version does not begin 'v'".into()),
        };
        Ok(Dependency {
            id,
            version: semver::Version::from_str(version)?,
        })
    }
}

pub(crate) fn parse_tree<R>(reader: R) -> Result<Vec<Dependency>, Box<dyn std::error::Error>>
where
    R: std::io::Read,
{
    let mut deps = Vec::new();
    let mut lines = std::io::BufReader::new(reader).lines();
    // skip the first line as this is the root
    lines.next();

    for line in lines {
        let line = line?;
        let dep = Dependency::from_str(line.as_str())?;
        deps.push(dep);
    }
    Ok(deps)
}

#[cfg(test)]
mod test {
    use super::Dependency;
    use std::str::FromStr;

    #[test]
    fn parses_line_with_trailing_asterisk() {
        let line = "    │   └── tracing-core v0.1.28 (*)";
        let dep = Dependency::from_str(line).unwrap();
        assert_eq!(dep.id, "tracing-core");
        assert_eq!(dep.version, semver::Version::new(0, 1, 28));
    }
}
