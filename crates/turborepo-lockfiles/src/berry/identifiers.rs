use std::{borrow::Cow, fmt, ops::Index};

use lazy_static::lazy_static;
use regex::Regex;
use thiserror::Error;

lazy_static! {
    static ref IDENT: Regex = Regex::new(r"^(?:@([^/]+?)/)?([^@/]+)$").unwrap();
    static ref DESCRIPTOR: Regex = Regex::new(r"^(?:@([^/]+?)/)?([^@/]+?)(?:@(.+))$").unwrap();
    static ref PATCH_REF: Regex = Regex::new(r"patch:(.+)#(?:\./)?([^:]+)(?:::)?.*$").unwrap();
    static ref MULTIKEY: Regex = Regex::new(r" *, *").unwrap();
    static ref BUILTIN: Regex = Regex::new(r"^builtin<([^>]+)>$").unwrap();
    static ref PROTOCOL: Regex = Regex::new(r"^[A-Za-z]+:").unwrap();
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid identifier ({0})")]
    Ident(String),
    #[error("Invalid descriptor ({0})")]
    Descriptor(String),
    #[error("Invalid locator ({0})")]
    Locator(String),
}

/// A package scope and name
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ident<'a> {
    scope: Option<Cow<'a, str>>,
    name: Cow<'a, str>,
}

/// An identifier with a semver range
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Descriptor<'a> {
    pub ident: Ident<'a>,
    pub range: Cow<'a, str>,
}

/// An identifier  with a resolved version.
/// They are similar to descriptors except that descriptors can reference
/// multiple packages whereas a locator references exactly one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Locator<'a> {
    pub ident: Ident<'a>,
    pub reference: Cow<'a, str>,
}

impl<'a> Ident<'a> {
    pub fn into_owned(&self) -> Ident<'static> {
        let Ident { scope, name } = self;
        let scope = scope
            .as_ref()
            .map(|scope| scope.to_string())
            .map(Cow::Owned);
        let name = Cow::Owned(name.to_string());
        Ident { scope, name }
    }
}

impl<'a> TryFrom<&'a str> for Ident<'a> {
    type Error = Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let make_err = || Error::Ident(value.to_string());
        let captures = IDENT.captures(value).ok_or_else(make_err)?;
        let scope = captures.get(1).map(|m| Cow::Borrowed(m.as_str()));
        let name = Cow::Borrowed(captures.get(2).map(|m| m.as_str()).ok_or_else(make_err)?);
        Ok(Self { scope, name })
    }
}

impl<'a> fmt::Display for Ident<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(scope) = self.scope.as_deref() {
            f.write_fmt(format_args!("@{scope}/"))?;
        }
        f.write_str(&self.name)
    }
}

impl<'a> TryFrom<&'a str> for Descriptor<'a> {
    type Error = Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        let make_err = || Error::Descriptor(value.to_string());
        let captures = DESCRIPTOR.captures(value).ok_or_else(make_err)?;
        let scope = captures.get(1).map(|m| Cow::Borrowed(m.as_str()));
        let name = Cow::Borrowed(captures.get(2).map(|m| m.as_str()).ok_or_else(make_err)?);
        let range = Cow::Borrowed(captures.get(3).map(|m| m.as_str()).ok_or_else(make_err)?);
        let ident = Ident { scope, name };
        Ok(Descriptor { ident, range })
    }
}

impl<'a> fmt::Display for Descriptor<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("{}@{}", self.ident, self.range))
    }
}

impl<'a> Descriptor<'a> {
    pub fn new(ident: &'a str, range: &'a str) -> Result<Self, Error> {
        let ident = Ident::try_from(ident)?;
        let range = range.into();
        Ok(Self { ident, range })
    }

    /// Extracts all descriptors that are present in a lockfile entry key
    pub fn from_lockfile_key(key: &'a str) -> impl Iterator<Item = Result<Descriptor<'a>, Error>> {
        MULTIKEY.split(key).map(Descriptor::try_from)
    }

    pub fn strip_protocol(range: &str) -> &str {
        range
            .find(':')
            .map_or(range, |colon_index| &range[colon_index + 1..])
    }

    pub fn range_without_protocol(&self) -> &str {
        self.range
            .find(':')
            .map_or(&self.range, |colon_index| &self.range[colon_index + 1..])
    }

    pub fn into_owned(self) -> Descriptor<'static> {
        let Self { ident, range } = self;
        let range = Cow::Owned(range.to_string());
        Descriptor {
            ident: ident.into_owned(),
            range,
        }
    }

    pub fn protocol(&self) -> Option<&str> {
        self.range.find(':').map(|i| &self.range[0..i])
    }

    /// Access the range based on the lifetime of the underlying string slice
    pub fn range(&self) -> Option<&'a str> {
        match self.range {
            Cow::Borrowed(s) => Some(s),
            _ => None,
        }
    }

    /// If the descriptor is a patch returns the version that the patch targets
    pub fn primary_version(&self) -> Option<String> {
        let Locator { reference, .. } = Locator::from_patch_reference(&self.range)?;
        // This is always owned due to needing to replace '%3A' with ':' so
        // we extract the owned string.
        Some(reference.into_owned())
    }
}

impl<'a> TryFrom<&'a str> for Locator<'a> {
    type Error = Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        // Descriptors and locators have the same structure so we use the descriptor
        // parsing logic
        let Descriptor { ident, range } = Descriptor::try_from(value).map_err(|err| match err {
            Error::Descriptor(val) => Error::Locator(val),
            _ => err,
        })?;
        Ok(Locator {
            ident,
            reference: range,
        })
    }
}

impl<'a> fmt::Display for Locator<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("{}@{}", self.ident, self.reference))
    }
}

const WORKSPACE_PROTOCOL: &str = "workspace:";

impl<'a> Locator<'a> {
    pub fn new(ident: &'a str, reference: &'a str) -> Result<Self, Error> {
        let ident = Ident::try_from(ident)?;
        Ok(Self {
            ident,
            reference: reference.into(),
        })
    }

    fn from_patch_reference(patch_reference: &'a str) -> Option<Self> {
        let caps = PATCH_REF.captures(patch_reference)?;
        let capture_group = caps.get(1)?;
        let Locator { ident, reference } = Locator::try_from(capture_group.as_str()).ok()?;
        // This might seem like a special case hack, but this is what yarn does
        let mut decoded_reference = reference.replace("npm%3A", "npm:");
        // Some older versions of yarn don't encode the npm protocol
        if !decoded_reference.starts_with("npm:") {
            decoded_reference.insert_str(0, "npm:");
        }
        Some(Locator {
            ident,
            reference: Cow::Owned(decoded_reference),
        })
    }

    pub fn is_patch_builtin(patch: &str) -> bool {
        patch.starts_with('~') || BUILTIN.is_match(patch)
    }

    pub fn is_workspace_path(&self, workspace_path: &str) -> bool {
        // This is slightly awkward, but it allows us to avoid an allocation
        self.reference.starts_with(WORKSPACE_PROTOCOL)
            && &self.reference[WORKSPACE_PROTOCOL.len()..] == workspace_path
    }

    /// Converts a possibly borrowed Locator to one that must be owned
    pub fn as_owned(&self) -> Locator<'static> {
        let Locator { ident, reference } = self;
        let ident = ident.into_owned();
        let reference = Cow::Owned(reference.to_string());
        Locator { ident, reference }
    }

    pub fn patch_file(&self) -> Option<&str> {
        PATCH_REF
            .captures(&self.reference)
            .and_then(|caps| caps.get(2))
            .map(|m| m.as_str())
    }

    pub fn patched_locator(&self) -> Option<Locator> {
        Locator::from_patch_reference(&self.reference)
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_parse_ident_with_scope() {
        assert_eq!(
            Ident::try_from("@babel/parser").unwrap(),
            Ident {
                scope: Some("babel".into()),
                name: "parser".into()
            }
        )
    }

    #[test]
    fn test_parse_ident_without_scope() {
        assert_eq!(
            Ident::try_from("turbo").unwrap(),
            Ident {
                scope: None,
                name: "turbo".into(),
            }
        )
    }

    #[test]
    fn test_ident_roundtrip() {
        for ident in ["turbo", "@babel/parser"] {
            assert_eq!(ident, Ident::try_from(ident).unwrap().to_string());
        }
    }

    #[test]
    fn test_parse_descriptor() {
        assert_eq!(
            Descriptor::try_from("@babel/code-frame@npm:7.12.11").unwrap(),
            Descriptor {
                ident: Ident {
                    scope: Some("babel".into()),
                    name: "code-frame".into()
                },
                range: "npm:7.12.11".into(),
            }
        )
    }

    #[test]
    fn test_descriptor_roundtrip() {
        for descriptor in [
            "@babel/code-frame@npm:7.12.11",
            "lodash@patch:lodash@npm%3A4.17.21#./.yarn/patches/lodash-npm-4.17.21-6382451519.\
             patch::version=4.17.21&hash=2c6e9e&locator=berry-patch%40workspace%3A.",
        ] {
            assert_eq!(
                descriptor,
                Descriptor::try_from(descriptor).unwrap().to_string()
            )
        }
    }

    #[test]
    fn test_locator_patch_file() {
        struct TestCase {
            locator: &'static str,
            file: Option<&'static str>,
        }
        let test_cases = [
            TestCase {
                locator: "lodash@patch:lodash@npm%3A4.17.21#./.yarn/patches/lodash-npm-4.17.\
                          21-6382451519.patch::version=4.17.21&hash=2c6e9e&locator=berry-patch%\
                          40workspace%3A.",
                file: Some(".yarn/patches/lodash-npm-4.17.21-6382451519.patch"),
            },
            TestCase {
                locator: "lodash@npm:4.17.21",
                file: None,
            },
            TestCase {
                locator: "resolve@patch:resolve@npm%3A2.0.0-next.4#~builtin<compat/\
                          resolve>::version=2.0.0-next.4&hash=07638b",
                file: Some("~builtin<compat/resolve>"),
            },
        ];
        for tc in test_cases {
            let locator = Locator::try_from(tc.locator).unwrap();
            assert_eq!(locator.patch_file(), tc.file);
        }
    }

    #[test]
    fn test_locator_patch_original_locator() {
        let locator = Locator::try_from(
            "lodash@patch:lodash@npm%3A4.17.21#./.yarn/patches/lodash-npm-4.17.21-6382451519.\
             patch::version=4.17.21&hash=2c6e9e&locator=berry-patch%40workspace%3A.",
        )
        .unwrap();
        let original = locator.patched_locator().unwrap();
        assert_eq!(original, Locator::try_from("lodash@npm:4.17.21").unwrap())
    }

    #[test]
    fn test_patch_primary_version() {
        struct TestCase {
            locator: &'static str,
            version: Option<&'static str>,
        }
        let test_cases = [
            TestCase {
                locator: "lodash@patch:lodash@npm%3A4.17.21#./.yarn/patches/lodash-npm-4.17.\
                          21-6382451519.patch::locator=berry-patch%40workspace%3A.",
                version: Some("npm:4.17.21"),
            },
            TestCase {
                locator: "typescript@patch:typescript@^4.5.2#~builtin<compat/typescript>",
                version: Some("npm:^4.5.2"),
            },
            TestCase {
                locator: "react@npm:18.2.0",
                version: None,
            },
        ];
        for tc in test_cases {
            let locator = Locator::try_from(tc.locator).unwrap();
            let patch_locator = locator.patched_locator();
            assert_eq!(
                tc.version,
                patch_locator.as_ref().map(|l| l.reference.as_ref()),
                "{}",
                tc.locator
            );
        }
    }
}
