use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dep {
    pub include: Vec<String>,
    pub mod_deps: Vec<String>,
    pub lib: Vec<String>,
}
impl Dep {
    pub fn clean_mod_name(name: &str) -> String {
        Path::new(name)
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .unwrap_or("")
            .to_string()
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Compiler {
    /// 交叉编译器名，默认 gcc
    #[serde(default = "default_cc")]
    pub cc: String,
    /// 目标三元组，如 aarch64-unknown-linux-gnu
    #[serde(default)]
    pub target: Option<String>,
    /// sysroot 路径，用于交叉编译
    #[serde(default)]
    pub sysroot: Option<String>,
    /// 额外编译参数
    #[serde(default)]
    pub flags: Vec<String>,
}

fn default_cc() -> String {
    "gcc".to_string()
}

impl Default for Compiler {
    fn default() -> Self {
        Compiler {
            cc: default_cc(),
            target: None,
            sysroot: None,
            flags: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub name: String,
    pub std: String,
    pub premacro: Vec<String>,
    pub dep: Dep,
    #[serde(default)]
    pub compiler: Compiler,
}

impl Config {
    pub fn new() -> Self {
        Config {
            name: "app".to_string(),
            std: "c99".to_string(),
            premacro: Vec::new(),
            dep: Dep {
                include: Vec::new(),
                mod_deps: Vec::new(),
                lib: Vec::new(),
            },
            compiler: Compiler::default(),
        }
    }

    pub fn from_yaml(path: &PathBuf) -> Result<Self, Box<dyn Error>> {
        let file = std::fs::File::open(path)?;
        let config: Config = serde_yaml::from_reader(file)?;

        Ok(config)
    }

    pub fn to_yaml(&self) -> Result<String, Box<dyn Error>> {
        let context = serde_yaml::to_string(self)?;
        Ok(context)
    }
}
