use anyhow::{bail, Result};
use turbo_tasks::{Value, ValueToString, Vc};

use super::{Chunk, ChunkableAsset, ChunkingContext};
use crate::{
    asset::{Asset, Assets},
    context::AssetContext,
    reference_type::{EntryReferenceSubType, ReferenceType},
};

/// Marker trait for the chunking context to accept evaluated entries.
///
/// The chunking context implementation will resolve the dynamic entry to a
/// well-known value or trait object.
#[turbo_tasks::value_trait]
pub trait EvaluatableAsset: Asset + ChunkableAsset {}

#[turbo_tasks::value_trait]
pub trait EvaluatableAssetExt {
    async fn to_evaluatable(
        self: Vc<Self>,
        context: Vc<&'static dyn AssetContext>,
    ) -> Result<Vc<&'static dyn EvaluatableAsset>>;
}

#[turbo_tasks::value_impl]
impl EvaluatableAssetExt for &'static dyn Asset {
    #[turbo_tasks::function]
    pub async fn to_evaluatable(
        self: Vc<Self>,
        context: Vc<&'static dyn AssetContext>,
    ) -> Result<Vc<&'static dyn EvaluatableAsset>> {
        let asset = context.process(
            self,
            Value::new(ReferenceType::Entry(EntryReferenceSubType::Runtime)),
        );
        let Some(entry) = Vc::try_resolve_downcast::<&dyn EvaluatableAsset>(asset).await? else {
            bail!("{} is not a valid evaluated entry", asset.ident().to_string().await?)
        };
        Ok(entry)
    }
}

#[turbo_tasks::value(transparent)]
pub struct EvaluatableAssets(Vec<Vc<&'static dyn EvaluatableAsset>>);

#[turbo_tasks::value_impl]
impl EvaluatableAssets {
    #[turbo_tasks::function]
    pub fn empty() -> Vc<EvaluatableAssets> {
        EvaluatableAssets(vec![]).cell()
    }

    #[turbo_tasks::function]
    pub fn one(entry: Vc<&'static dyn EvaluatableAsset>) -> Vc<EvaluatableAssets> {
        EvaluatableAssets(vec![entry]).cell()
    }

    #[turbo_tasks::function]
    pub async fn with_entry(
        self: Vc<Self>,
        entry: Vc<&'static dyn EvaluatableAsset>,
    ) -> Result<Vc<EvaluatableAssets>> {
        let mut entries = self.await?.clone_value();
        entries.push(entry);
        Ok(EvaluatableAssets(entries).cell())
    }
}

/// Trait for chunking contexts which can generate evaluated chunks.
#[turbo_tasks::value_trait]
pub trait EvaluateChunkingContext: ChunkingContext {
    /// Create a chunk that evaluates the given entries.
    fn evaluate_chunk(
        self: Vc<Self>,
        entry_chunk: Vc<&'static dyn Chunk>,
        other_assets: Vc<Assets>,
        evaluatable_assets: Vc<EvaluatableAssets>,
    ) -> Vc<&'static dyn Asset>;
}
