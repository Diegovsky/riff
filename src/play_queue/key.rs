use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryKey {
    Context(usize),
    Queued(u64),
    NextLoop(usize),
}

impl fmt::Display for EntryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Context(i) => write!(f, "queue:c:{i}"),
            Self::Queued(uid) => write!(f, "queue:q:{uid}"),
            Self::NextLoop(i) => write!(f, "queue:n:{i}"),
        }
    }
}

impl FromStr for EntryKey {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rest = s.strip_prefix("queue:").ok_or(())?;
        if let Some(i) = rest.strip_prefix("c:") {
            i.parse().map(Self::Context).map_err(|_| ())
        } else if let Some(uid) = rest.strip_prefix("q:") {
            uid.parse().map(Self::Queued).map_err(|_| ())
        } else if let Some(i) = rest.strip_prefix("n:") {
            i.parse().map(Self::NextLoop).map_err(|_| ())
        } else {
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_key_round_trip() {
        for key in [
            EntryKey::Context(12),
            EntryKey::Queued(7),
            EntryKey::NextLoop(3),
        ] {
            assert_eq!(key.to_string().parse::<EntryKey>(), Ok(key));
        }
        assert!("abc".parse::<EntryKey>().is_err());
        assert!("queue:x:1".parse::<EntryKey>().is_err());
    }
}
