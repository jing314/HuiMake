use std::path::PathBuf;
use std::error::Error;
use crate::{logi, logd, loge};
use crate::mods::{analyzer::ModsManage, single::ModFile};

#[derive(Debug)]
enum RunSatus {
    ProjectRoot,
    ModRoot,
    Unknown
}

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

    ///检测当前工具运行命令环境是 项目根目录 还是 模块根目录
    pub fn check(&mut self)->Result<(),Box<dyn Error>>{
        let run_dir = PathBuf::from(".");   //获取当前文件夹路径
        logi!("check run_dir:{:#?}",run_dir);
        let mut manage = ModsManage::new();
        
        //如果是项目根目录
        if manage.gen_mods_depsgraph(&run_dir).is_ok() {    
                logd!("{:#?} try gen_mods_depsgraph success",run_dir);
                self.status = RunSatus::ProjectRoot;
                self.mods = Some(manage);
        }else {
            let mut mod_ = ModFile::new();
        
            //如果是mod目录
            if mod_.get_info(&run_dir).is_ok(){
                logd!("{:#?} try get mod info success",run_dir);
                self.status = RunSatus::ModRoot;
                self.cur_mod = Some(mod_);
                manage.gen_mods_depsgraph(&run_dir.join(".."))?;
                self.mods = Some(manage);
            }else {
                self.status = RunSatus::Unknown;
                return Err("err hk project struct".into());
            }
        }
        Ok(())
    }

    ///构建命令，构建当前模块以及其所依赖的其余模块
    ///基于其所在目录区分逻辑
    pub fn build(&mut self,is_run:bool)->Result<(),Box<dyn Error>>{
        match self.status {
            RunSatus::ModRoot =>{
                let mods = self.mods.as_mut().unwrap();
                loop {
                    let mut next = mods.get_next()?;
                    if next.is_empty(){
                        break;
                    }
                    for mod_ in &mut next {
                        mod_.build()?;
                        if is_run {
                            mod_.run()?;
                        }
                        if mod_.name == self.cur_mod.as_ref().unwrap().name {
                            return Ok(());
                        }
                    }
                }

            }
            RunSatus::ProjectRoot =>{
                let mods = self.mods.as_mut().unwrap();
                loop {
                    let mut next = mods.get_next()?;
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
     
        Ok(())

    }
    
    ///运行命令
    pub fn run(&mut self)->Result<(),Box<dyn Error>>{
        self.build(true)?;
        Ok(())
    }

    ///清理构建文件
    pub fn clean(&mut self)->Result<(),Box<dyn Error>>{
        match self.status {
            RunSatus::ModRoot =>{
                self.cur_mod.as_mut().unwrap().clean_build()?;
            }
            RunSatus::ProjectRoot =>{
                let mods = self.mods.as_mut().unwrap();
                loop {
                    let mut next = mods.get_next()?;
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

    ///生成新项目mod目录
    pub fn gen(&self,name:&str)->Result<(),Box<dyn Error>>{
        ModFile::gen(name)?;
        Ok(())
    }
}
