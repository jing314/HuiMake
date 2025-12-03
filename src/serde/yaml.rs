use serde::{Deserialize, Serialize};
use std::{error::Error, path:: {PathBuf,Path}};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dep {
    pub include: Vec<String>,
    pub hkmod: Vec<String>,
    pub lib: Vec<String>,
}
impl Dep{
    pub fn clean_hk_name_str(name: &str) -> Option<&str> {
    Path::new(name)
        .file_name()    // 获取路径的最后一部分
        .and_then(|os_str| os_str.to_str())  // OsStr 转 &str
    }
    pub fn clean_hk_name_string(name:&String)->String{
        Path::new(name)
        .file_name()
        .and_then(|os_str| os_str.to_str())
        .unwrap_or("")
        .to_string()
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub name: String,
    pub std: String,
    pub premacro: Vec<String>,
    pub dep: Dep,
}

impl Config {
    pub fn new() -> Self {
        Config {
            name: "App".to_string(),
            std: "C99".to_string(),
            premacro: Vec::new(),
            dep: Dep {
                include: Vec::new(),
                hkmod: Vec::new(),
                lib: Vec::new(),
            },
        }
    }
    
    pub fn from_yaml(path: &PathBuf) -> Result<Self, Box<dyn Error>> {
        let file = std::fs::File::open(path)?;
        let config: Config = serde_yaml::from_reader(file)?;
        
        Ok(config)
    }
    
    pub fn load(&mut self, path: &PathBuf) -> Result<(), Box<dyn Error>> {
        *self = Self::from_yaml(path)?;
        Ok(())
    }

    pub fn to_yaml(&self) -> Result<String,Box<dyn Error>>{
        let context = serde_yaml::to_string(self)?;
        Ok(context)
    }
}