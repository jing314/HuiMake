use std::process::Command;
use std::path::PathBuf;
use std::error::Error;
use cc::Build;
use std::fs;
use crate::logd;
use crate::loge;
use crate::logi;
use crate::mods::{analyzer::ModsManage, single::{ModFile, ProjectMap}};
#[derive(Debug)]
enum RunSatus {
    ProjectRoot,
    ModRoot,
    Unknown
}

/*
检测当前工具运行命令环境是 项目根目录 还是 模块根目录
*/
#[derive(Debug)]
pub struct CmdNeedData{
    status:RunSatus,
    mods:Option<ModsManage>,
    cur_mod:Option<ModFile>,
}



impl CmdNeedData {
    pub fn new()->Self{
        CmdNeedData { 
            status: RunSatus::Unknown, 
            mods: None, 
            cur_mod: None 
        }
    }

    pub fn check(&mut self)->Result<(),Box<dyn Error>>{
        let run_dir = PathBuf::from(".");   //获取当前文件夹路径
        logi!("check run_dir:{:#?}",run_dir);
        match ModsManage::gen_mods_depsgraph(&run_dir) {    //如果是项目根目录
            Ok(projetc) =>{
                logi!("{:#?} try gen_mods_depsgraph success",run_dir);
                self.status = RunSatus::ProjectRoot;
                self.mods = Some(projetc);
            }
            Err(_) =>{
                let mut mod_ = ModFile::new();
                if mod_.get_info(&run_dir).is_ok(){//如果是mod目录
                    logi!("{:#?} try get mod info success",run_dir);
                    self.status = RunSatus::ModRoot;
                    self.cur_mod = Some(mod_);
                    self.mods = Some(ModsManage::gen_mods_depsgraph(&run_dir.join(".."))?);
                }else {
                    self.status = RunSatus::Unknown;
                    return Err("err hk project struct".into());
                } 
            }      
        }
        Ok(())
    }
    
    /*
    构建命令，构建当前模块以及其所依赖的其余模块
    基于其所在目录区分逻辑
    */
    pub fn build(&mut self,is_run:bool)->Result<Vec<PathBuf>,Box<dyn Error>>{
        let exe_path = Vec::new();
        match self.status {
            RunSatus::ModRoot =>{
                let mods = self.mods.as_mut().unwrap();
                while  let mut next = mods.get_next()? {
                    if next.is_empty(){
                        break;
                    }
                    for mod_ in &mut next {
                        mod_.build()?;
                        if is_run {
                            mod_.run()?;
                        }
                        if mod_.config.as_ref().unwrap().name == self.cur_mod.as_ref().unwrap().config.as_ref().unwrap().name {
                            return Ok(exe_path);
                        }
                    }
                }

            }
            RunSatus::ProjectRoot =>{
                let mods = self.mods.as_mut().unwrap();
                while  let mut next = mods.get_next()? {
                    if next.is_empty(){
                        break;
                    }
                    for mod_ in &mut next {
        
                        mod_.build()?;
                        if is_run {
                            mod_.run()?;
                        }
                    }
                }
            }
            RunSatus::Unknown =>{
                return Err("Err project".into());
            }
        }
     
        Ok(exe_path)

    }
    
    pub fn run_dir(&mut self)->Result<(),Box<dyn Error>>{
        self.build(true)?;
        Ok(())
    }
    pub fn clean_cmd(&mut self)->Result<(),Box<dyn Error>>{
        match self.status {
            RunSatus::ModRoot =>{
                self.cur_mod.as_mut().unwrap().clean_build()?;
            }
            RunSatus::ProjectRoot =>{
                let mods = self.mods.as_mut().unwrap();
                while  let mut next = mods.get_next()? {
                    if next.is_empty(){
                        break;
                    }
                    for mod_ in &mut next {
                        mod_.clean_build()?;
                    }
                }
            }
            RunSatus::Unknown =>{
                return Err("Err project".into());
            }
        }
        Ok(())
    }
}
