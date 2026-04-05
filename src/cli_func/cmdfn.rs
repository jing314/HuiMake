use crate::mods_alyz::{analyzer::ModMgr, single::ModFile};
use crate::{logd, loge, logi};
use std::error::Error;
use std::path::PathBuf;

#[derive(Debug)]
enum RunStatus {
    ProjectRoot,
    ModRoot,
    Unknown,
}

#[derive(Debug)]
pub struct CmdCtx {
    status: RunStatus,
    mods: Option<ModMgr>,
    cur_mod: Option<ModFile>,
}

impl CmdCtx {
    pub fn new() -> Self {
        CmdCtx {
            status: RunStatus::Unknown,
            mods: None,
            cur_mod: None,
        }
    }

    /// 检测当前工具运行命令环境是 项目根目录 还是 模块根目录
    pub fn detect_env(&mut self) -> Result<(), Box<dyn Error>> {
        let run_dir = PathBuf::from("."); //获取当前文件夹路径
        logi!("detect_env run_dir:{:#?}", run_dir);
        let mut manage = ModMgr::new();

        //如果是项目根目录
        if manage.build_dep_graph(&run_dir).is_ok() {
            logd!("{:#?} try build_dep_graph success", run_dir);
            self.status = RunStatus::ProjectRoot;
            self.mods = Some(manage);
        } else {
            let mut mod_ = ModFile::new();

            //如果是mod目录
            if mod_.load(&run_dir).is_ok() {
                logd!("{:#?} try get mod info success", run_dir);
                self.status = RunStatus::ModRoot;
                self.cur_mod = Some(mod_);
                manage.build_dep_graph(&run_dir.join(".."))?;
                self.mods = Some(manage);
            } else {
                self.status = RunStatus::Unknown;
                return Err("err hk project struct".into());
            }
        }
        Ok(())
    }

    ///构建命令，构建当前模块以及其所依赖的其余模块
    ///基于其所在目录区分逻辑
    pub fn build(&mut self, is_run: bool) -> Result<(), Box<dyn Error>> {
        match self.status {
            RunStatus::ModRoot => {
                let mods = self.mods.as_mut().unwrap();
                loop {
                    let mut next = mods.get_next_buildable()?;
                    if next.is_empty() {
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
            RunStatus::ProjectRoot => {
                let mods = self.mods.as_mut().unwrap();
                loop {
                    let mut next = mods.get_next_buildable()?;
                    if next.is_empty() {
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
            RunStatus::Unknown => {
                return Err("Err project".into());
            }
        }

        Ok(())
    }

    ///运行命令
    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        self.build(true)?;
        Ok(())
    }

    ///清理构建文件
    pub fn clean(&mut self) -> Result<(), Box<dyn Error>> {
        match self.status {
            RunStatus::ModRoot => {
                self.cur_mod.as_mut().unwrap().clean_build()?;
            }
            RunStatus::ProjectRoot => {
                let mods = self.mods.as_mut().unwrap();
                loop {
                    let mut next = mods.get_next_buildable()?;
                    if next.is_empty() {
                        break;
                    }
                    for mod_ in &mut next {
                        mod_.clean_build()?;
                    }
                }
            }
            RunStatus::Unknown => {
                return Err("Err project".into());
            }
        }
        Ok(())
    }

    ///生成新项目mod目录
    pub fn gen(&self, name: &str) -> Result<(), Box<dyn Error>> {
        ModFile::gen(name)?;
        Ok(())
    }
}
