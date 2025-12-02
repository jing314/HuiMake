use std::fs;
use std::error::Error;
use std::path::PathBuf;

use crate::serde::yaml::Config;

#[derive(Debug)]
pub  struct ModFile{
    pub bin: Vec<String>,
    pub include: Vec<String>,
    pub src: Vec<String>,
    pub config: Config,
}
impl ModFile{
    pub fn new()->ModFile{
        ModFile{
            bin: Vec::new(),
            include: Vec::new(),
            src: Vec::new(),
            config: Config::new(),
        }
    }
    pub fn gen(mod_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // 使用 &str 而不是 &String 更通用
        let base = PathBuf::from(mod_name);
        
        // 一次性创建所有目录
        let dirs = ["bin", "include", "src"];
        for dir in &dirs {
            fs::create_dir_all(base.join(dir))?;
        }
        let config_dir = base.join("config.yaml");
        let def_config = Config::new();
        if let Ok(content) = def_config.to_yaml(){
            fs::write(config_dir, content)?;
        }
        Ok(())
    }
    fn get_dir_info(dir_path:&PathBuf)->Result<Vec<String>,Box<dyn Error>>{
        let mut file_name = Vec::new();
        for entry in fs::read_dir(dir_path)?{
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                file_name.push(name.to_string());                
            }
        }
        Ok(file_name)
    }
    pub fn get_info(path:&PathBuf)->Result<Self,Box<dyn Error>>{
        Ok(ModFile { 
            bin: Self::get_dir_info(&path.join("bin"))?, 
            include: Self::get_dir_info(&path.join("include"))?, 
            src: Self::get_dir_info(&path.join("src"))?,
            config: Config::from_yaml(&path.join("config.yaml"))?,
        })
    }
}

