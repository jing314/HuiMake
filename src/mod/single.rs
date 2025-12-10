use std::collections::HashMap;
use std::fs;
use std::error::Error;
use std::path::PathBuf;

use clap::builder::Str;
use clap::parser::Indices;

use crate::serde::yaml::Config;

#[derive(Debug,Clone)]
pub  struct ModFile{
    pub bin: Vec<PathBuf>,
    pub include: Vec<PathBuf>,
    pub src: Vec<PathBuf>,
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
    pub fn gen(mod_name: &str) -> Result<(), Box<dyn Error>> {
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
    fn get_dir_info(dir_path:&PathBuf)->Result<Vec<PathBuf>,Box<dyn Error>>{
        println!("get_dir_info {:?}",dir_path);
        let mut file_name = Vec::new();
        match fs::read_dir(dir_path) {
            Ok(entrys)=>{
                for entry in entrys{
                     let path = entry.unwrap().path();
                     let absolute_path = fs::canonicalize(&path)?;
                     file_name.push(absolute_path);
             
                }
            }
            Err(_)=>{
                println!("未找到对于文件{:?}",dir_path);
            }
        }
        Ok(file_name)
    }
    pub fn get_info(path:&PathBuf)->Result<Self,Box<dyn Error>>{
        Ok(ModFile { 
            bin: Self::get_dir_info(&path.join("bin")).unwrap(), 
            include: Self::get_dir_info(&path.join("include")).unwrap(), 
            src: Self::get_dir_info(&path.join("src")).unwrap(),
            config: Config::from_yaml(&path.join("config.yaml"))?,
        })
    }
}
#[derive(Debug)]
pub struct  ProjectMap{
    pub modname:Vec<String>,
    pub indices :HashMap<String,ModFile>,
    pub index :HashMap<String,PathBuf>,
}
impl ProjectMap {
    pub fn new()->Self{
        ProjectMap { 
            modname:Vec::new(),
            indices:HashMap::new(),
            index: HashMap::new()
         }
    }
    pub fn get_project_info(&mut self)->Result<(),Box<dyn Error>>{
    let current_dir = PathBuf::from(".");
    let mods: Vec<PathBuf> = fs::read_dir(&current_dir)?
            .filter_map(|entry| {
                match entry {
                    Ok(e)=>{
                        let path = e.path();
                        if path.is_dir(){
                            Some(path)
                        }else {
                            println!("{:?}无法获得mod文件目录",path);
                            None
                        }
                    }
                    Err(_)=> None,
                }
            }).collect();
      for path in &mods {
          match  ModFile::get_info(path) {
              Ok(modfile) => {
                let name = modfile.config.name.clone();
                self.modname.push(name.clone());
                self.index.insert(name.clone(), path.clone());
                self.indices.insert(name.clone(), modfile);
              }
              Err(_)=>{
                println!("{:?} 不是合格的工程目录",path);
              }
          } 
      }
        Ok(())
    }
}