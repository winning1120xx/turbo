use anyhow::Result;
use turbo_tasks::Vc;
use turbo_tasks_fs::FileSystemPath;

use super::Issue;

#[turbo_tasks::value(shared)]
pub struct PackageJsonIssue {
    pub path: Vc<FileSystemPath>,
    pub error_message: String,
}

#[turbo_tasks::value_impl]
impl Issue for PackageJsonIssue {
    #[turbo_tasks::function]
    fn title(&self) -> Vc<String> {
        Vc::cell("Error parsing package.json file".to_string())
    }

    #[turbo_tasks::function]
    fn category(&self) -> Vc<String> {
        Vc::cell("parse".to_string())
    }

    #[turbo_tasks::function]
    fn context(&self) -> Vc<FileSystemPath> {
        self.path
    }

    #[turbo_tasks::function]
    fn description(&self) -> Vc<String> {
        Vc::cell(self.error_message.clone())
    }
}
