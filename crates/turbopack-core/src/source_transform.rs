use anyhow::Result;
use turbo_tasks::Vc;

use crate::asset::Asset;

#[turbo_tasks::value_trait]
pub trait SourceTransform {
    fn transform(self: Vc<Self>, source: Vc<&'static dyn Asset>) -> Vc<&'static dyn Asset>;
}

#[turbo_tasks::value(transparent)]
pub struct SourceTransforms(Vec<Vc<&'static dyn SourceTransform>>);

#[turbo_tasks::value_impl]
impl SourceTransforms {
    #[turbo_tasks::function]
    pub async fn transform(
        self: Vc<Self>,
        source: Vc<&'static dyn Asset>,
    ) -> Result<Vc<&'static dyn Asset>> {
        Ok(self
            .await?
            .iter()
            .fold(source, |source, transform| transform.transform(source)))
    }
}
