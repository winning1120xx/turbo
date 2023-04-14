use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use turbo_tasks::{trace::TraceRawVcs, Vc};
use turbo_tasks_fs::FileSystemPath;

use crate::environment::Environment;

// TODO stringify split map collect could be optimized with a marco
#[macro_export]
macro_rules! definable_name_map_internal {
    ($map:ident, $($name:ident).+ = $value:expr) => {
        $map.insert(
            $crate::definable_name_map_internal!($($name).+).into(),
            $value.into()
        );
    };
    ($map:ident, $($name:ident).+ = $value:expr,) => {
        $map.insert(
            $crate::definable_name_map_internal!($($name).+).into(),
            $value.into()
        );
    };
    ($map:ident, $($name:ident).+ = $value:expr, $($more:tt)+) => {
        $crate::definable_name_map_internal!($map, $($name).+ = $value);
        $crate::definable_name_map_internal!($map, $($more)+);
    };
    ($name:ident) => {
        [stringify!($name).to_string()]
    };
    ($name:ident . $($more:ident).+) => {
        $crate::definable_name_map_internal!($($more).+, [stringify!($name).to_string()])
    };
    ($name:ident, [$($array:expr),+]) => {
        [$($array),+, stringify!($name).to_string()]
    };
    ($name:ident . $($more:ident).+, [$($array:expr),+]) => {
        $crate::definable_name_map_internal!($($more).+, [$($array),+, stringify!($name).to_string()])
    };
}

#[macro_export]
macro_rules! compile_time_defines {
    ($($more:tt)+) => {
        {
            let mut map = std::collections::HashMap::new();
            $crate::definable_name_map_internal!(map, $($more)+);
            $crate::compile_time_info::CompileTimeDefines(map)
        }
    };
}

#[macro_export]
macro_rules! free_var_references {
    ($($more:tt)+) => {
        {
            let mut map = std::collections::HashMap::new();
            $crate::definable_name_map_internal!(map, $($more)+);
            $crate::compile_time_info::FreeVarReferences(map)
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TraceRawVcs)]
pub enum CompileTimeDefineValue {
    Bool(bool),
    String(String),
}

impl From<bool> for CompileTimeDefineValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<String> for CompileTimeDefineValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for CompileTimeDefineValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

#[turbo_tasks::value(transparent)]
pub struct CompileTimeDefines(pub HashMap<Vec<String>, CompileTimeDefineValue>);

#[turbo_tasks::value_impl]
impl CompileTimeDefines {
    #[turbo_tasks::function]
    pub fn empty() -> Vc<Self> {
        Vc::cell(HashMap::new())
    }
}

#[turbo_tasks::value]
pub enum FreeVarReference {
    EcmaScriptModule {
        request: String,
        context: Option<Vc<FileSystemPath>>,
        export: Option<String>,
    },
}

#[turbo_tasks::value(transparent)]
pub struct FreeVarReferences(pub HashMap<Vec<String>, FreeVarReference>);

#[turbo_tasks::value_impl]
impl FreeVarReferences {
    #[turbo_tasks::function]
    pub fn empty() -> Vc<Self> {
        Vc::cell(HashMap::new())
    }
}

#[turbo_tasks::value(shared)]
pub struct CompileTimeInfo {
    pub environment: Vc<Environment>,
    pub defines: Vc<CompileTimeDefines>,
    pub free_var_references: Vc<FreeVarReferences>,
}

impl CompileTimeInfo {
    pub fn builder(environment: Vc<Environment>) -> CompileTimeInfoBuilder {
        CompileTimeInfoBuilder {
            environment,
            defines: None,
            free_var_references: None,
        }
    }
}

#[turbo_tasks::value_impl]
impl CompileTimeInfo {
    #[turbo_tasks::function]
    pub fn new(environment: Vc<Environment>) -> Vc<Self> {
        CompileTimeInfo {
            environment,
            defines: CompileTimeDefines::empty(),
            free_var_references: FreeVarReferences::empty(),
        }
        .cell()
    }

    #[turbo_tasks::function]
    pub async fn environment(self: Vc<Self>) -> Result<Vc<Environment>> {
        Ok(self.await?.environment)
    }
}

pub struct CompileTimeInfoBuilder {
    environment: Vc<Environment>,
    defines: Option<Vc<CompileTimeDefines>>,
    free_var_references: Option<Vc<FreeVarReferences>>,
}

impl CompileTimeInfoBuilder {
    pub fn defines(mut self, defines: Vc<CompileTimeDefines>) -> Self {
        self.defines = Some(defines);
        self
    }

    pub fn free_var_references(mut self, free_var_references: Vc<FreeVarReferences>) -> Self {
        self.free_var_references = Some(free_var_references);
        self
    }

    pub fn build(self) -> CompileTimeInfo {
        CompileTimeInfo {
            environment: self.environment,
            defines: self.defines.unwrap_or_else(CompileTimeDefines::empty),
            free_var_references: self
                .free_var_references
                .unwrap_or_else(FreeVarReferences::empty),
        }
    }

    pub fn cell(self) -> Vc<CompileTimeInfo> {
        self.build().cell()
    }
}
