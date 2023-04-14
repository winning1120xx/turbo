use std::fmt::Write;

use anyhow::Result;
use turbo_tasks::{Value, ValueToString, Vc};
use turbo_tasks_fs::FileSystemPath;

use crate::resolve::ModulePart;

#[turbo_tasks::value(serialization = "auto_for_input")]
#[derive(Clone, Debug, PartialOrd, Ord, Hash)]
pub struct AssetIdent {
    /// The primary path of the asset
    pub path: Vc<FileSystemPath>,
    /// The query string of the asset (e.g. `?foo=bar`)
    pub query: Option<Vc<String>>,
    /// The fragment of the asset (e.g. `#foo`)
    pub fragment: Option<Vc<String>>,
    /// The assets that are nested in this asset
    pub assets: Vec<(Vc<String>, Vc<AssetIdent>)>,
    /// The modifiers of this asset (e.g. `client chunks`)
    pub modifiers: Vec<Vc<String>>,
    /// The part of the asset that is a (ECMAScript) module
    pub part: Option<Vc<ModulePart>>,
}

impl AssetIdent {
    pub fn add_modifier(&mut self, modifier: Vc<String>) {
        self.modifiers.push(modifier);
    }

    pub fn add_asset(&mut self, key: Vc<String>, asset: Vc<AssetIdent>) {
        self.assets.push((key, asset));
    }
}

#[turbo_tasks::value_impl]
impl ValueToString for AssetIdent {
    #[turbo_tasks::function]
    async fn to_string(&self) -> Result<Vc<String>> {
        let mut s = self.path.to_string().await?.clone_value();
        if let Some(query) = &self.query {
            write!(s, "?{}", query.await?)?;
        }
        if let Some(fragment) = &self.fragment {
            write!(s, "#{}", fragment.await?)?;
        }
        for (key, asset) in &self.assets {
            write!(s, "/({})/{}", key.await?, asset.to_string().await?)?;
        }
        if !self.modifiers.is_empty() {
            s.push_str(" (");
            for (i, modifier) in self.modifiers.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&modifier.await?);
            }
            s.push(')');
        }
        Ok(Vc::cell(s))
    }
}

#[turbo_tasks::value_impl]
impl AssetIdent {
    #[turbo_tasks::function]
    pub fn new(ident: Value<AssetIdent>) -> Vc<Self> {
        ident.into_value().cell()
    }

    /// Creates an [AssetIdent] from a [Vc<FileSystemPath>]
    #[turbo_tasks::function]
    pub fn from_path(path: Vc<FileSystemPath>) -> Vc<Self> {
        Self::new(Value::new(AssetIdent {
            path,
            query: None,
            fragment: None,
            assets: Vec::new(),
            modifiers: Vec::new(),
            part: None,
        }))
    }

    #[turbo_tasks::function]
    pub async fn with_modifier(self: Vc<Self>, modifier: Vc<String>) -> Result<Vc<Self>> {
        let mut this = self.await?.clone_value();
        this.add_modifier(modifier);
        Ok(Self::new(Value::new(this)))
    }

    #[turbo_tasks::function]
    pub async fn with_part(self: Vc<Self>, part: Vc<ModulePart>) -> Result<Vc<Self>> {
        let mut this = self.await?.clone_value();
        this.part = Some(part);
        Ok(Self::new(Value::new(this)))
    }

    #[turbo_tasks::function]
    pub async fn path(self: Vc<Self>) -> Result<Vc<FileSystemPath>> {
        Ok(self.await?.path)
    }
}
