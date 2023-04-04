use std::fmt::Display;

use glob_match::glob_match;
use turborepo_paths::AbsolutePathBuf;

pub enum WalkType {
    Files,
    Folders,
    All,
}

#[derive(Debug)]
pub enum WalkError {
    Error(walkdir::Error),
}

impl Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalkError::Error(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for WalkError {}

/// Performs a glob walk, yielding paths that
/// _are_ included in the include list (if it is nonempty)
/// and _not_ included in the exclude list.
///
/// In the case of an empty include, then all
/// files are included.
pub fn globwalk<'a>(
    base_path: AbsolutePathBuf,
    include: &'a [String],
    exclude: &'a [String],
    walk_type: WalkType,
) -> impl Iterator<Item = Result<AbsolutePathBuf, WalkError>> + 'a {
    let walker = walkdir::WalkDir::new(base_path.as_path());

    walker
        .into_iter()
        // we need to eagerly filter folders, preventing traversal into
        // folders that aren't included or are excluded. we do this by
        // evaluating 'potential matches' for the include list meaning
        // 'it is possible that a subfolder may match', as well as definite
        // matches for the exclude list
        .filter_entry(|e| {
            include
                .iter()
                .any(|p| potential_match(e.path().to_string_lossy().as_ref(), p))
                && !exclude
                    .iter()
                    .any(|glob| glob_match(glob, &e.path().to_string_lossy()))
        })
        .map(|res| {
            res.map(|dir| {
                dir.into_path()
                    .try_into()
                    .expect("these are absolute paths")
            })
            .map_err(WalkError::Error)
        })
        .filter(move |res: &Result<AbsolutePathBuf, _>| match res {
            Ok(path) => match walk_type {
                // if we encounter a file, then we need to check if it matches
                WalkType::Files | WalkType::All if path.is_file() => {
                    include
                        .iter()
                        .any(|glob| glob_match(glob, &path.to_string_lossy()))
                        && !exclude
                            .iter()
                            .any(|glob| glob_match(glob, &path.to_string_lossy()))
                }
                WalkType::Files => false,
                // any folders encountered are always included, since bad folders are not traversed
                // at all by the filter_entry step
                WalkType::Folders | WalkType::All => path.is_dir(),
            },
            Err(_) => true,
        })
}

/// Checks if a path is a partial match for a glob, meaning that a
/// subfolder could match.
fn potential_match(path: &str, glob: &str) -> bool {
    let matches = glob_match(glob, path);

    // pop last chunk from glob and try again
    if !matches {
        if let Some((prefix, _)) = glob.rsplit_once('/') {
            potential_match(path, prefix)
        } else {
            false
        }
    } else {
        true
    }
}

#[cfg(test)]
mod test {
    use test_case::test_case;

    #[test_case("/a/b/c/d", "/a/b/c/d", true; "exact match")]
    #[test_case("/a", "/a/b/c", true; "minimal match")]
    #[test_case("/a/b/c/d", "**", true; "doublestar")]
    #[test_case("/a/b/c", "/b", false; "no match")]
    #[test_case("a", "a/b/**", true; "relative path")]
    #[test_case("a/b", "a/**/c/d", true; "doublestar with later folders")]
    #[test_case("/a/b/c", "/a/*/c", true; "singlestar")]
    #[test_case("/a/b/c/d/e", "/a/**/d/e", true; "doublestar middle")]
    #[test_case("/a/b/c/d/e", "/a/**/e", true; "doublestar skip folders")]
    #[test_case("/a/b/c/d/e", "/a/**/*", true; "doublestar singlestar combination")]
    #[test_case("/a/b/c/d/e", "/a/*/*/d/*", true; "multiple singlestars")]
    #[test_case("/a/b/c/d/e", "/**/c/d/*", true; "leading doublestar")]
    #[test_case("/a/b/c/d/e", "/*/b/**", true; "leading singlestar and doublestar")]
    #[test_case("/a/b/c/d", "/a/b/c/?", true; "question mark match")]
    #[test_case("/a/b/c/d/e/f", "/a/b/**/e/?", true; "doublestar question mark combination")]
    #[test_case("/a/b/c/d/e/f", "/a/*/c/d/*/?", true; "singlestar doublestar question mark combination")]
    #[test_case("/a/b/c/d", "/a/b/c/?/e", true; "question mark over match")]
    #[test_case("/a/b/c/d/e/f", "/a/b/*/e/f", false; "singlestar no match")]
    #[test_case("/a/b/c/d/e", "/a/b/**/e/f/g", true; "doublestar over match")]
    #[test_case("/a/b/c/d/e", "/a/b/*/d/z", false; "multiple singlestars no match")]

    fn potential_match(path: &str, glob: &str, exp: bool) {
        assert_eq!(super::potential_match(path, glob), exp);
    }
}
