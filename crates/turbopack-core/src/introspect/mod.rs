pub mod asset;

use indexmap::IndexSet;
use turbo_tasks::Vc;

#[turbo_tasks::value(transparent)]
pub struct IntrospectableChildren(IndexSet<(Vc<String>, Vc<&'static dyn Introspectable>)>);

#[turbo_tasks::value_trait]
pub trait Introspectable {
    fn ty(self: Vc<Self>) -> Vc<String>;
    fn title(self: Vc<Self>) -> Vc<String> {
        String::empty()
    }
    fn details(self: Vc<Self>) -> Vc<String> {
        String::empty()
    }
    fn children(self: Vc<Self>) -> Vc<IntrospectableChildren> {
        Vc::cell(IndexSet::new())
    }
}
