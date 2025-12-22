use std::collections::HashMap;
use std::fmt::format;
use std::process::Command;
use std::error::Error;
use std::path::PathBuf;
use cc::Build;
use std::fs;
use crate::utility::logo;
use crate::utility::logo::print_logo;
use clap::builder::Str;
use clap::parser::Indices;

use crate::serde::yaml::Config;
use crate::loge;
use crate::logi;
use crate::logd;
use crate::utility::log;
#[derive(Debug,Clone,PartialEq)]
pub enum BuildStatus {
    BuildSuccess,
    BuildFail,
    NoBuild,
}

#[derive(Debug,Clone)]
pub  struct ModFile{
    pub status:BuildStatus,
    pub absolute_path:PathBuf,
    pub bin: Option<Vec<PathBuf>>,
    pub include: Option<Vec<PathBuf>>,
    pub src: Option<Vec<PathBuf>>,
    pub config: Option<Config>,
}
impl ModFile{
    pub fn new()->Self{
        ModFile{
            status:BuildStatus::NoBuild,
            absolute_path:PathBuf::new(),
            bin: None,
            include: None,
            src: None,
            config: None,
        }
    }

    pub fn get_info(&mut self,path:&PathBuf)->Result<(),Box<dyn Error>>{
        logi!("ModFile get_info path:{:#?}",path);
        self.absolute_path = fs::canonicalize(&path).unwrap();
        self.bin = Self::get_dir_bin_info(&path.join("bin")).ok(); 
        self.include = Self::get_dir_info(&path.join("include")).ok(); 
        self.src = Self::get_dir_info(&path.join("src")).ok();
        self.config = Config::from_yaml(&path.join("config.yaml")).ok();
        
        self.get_include_of_config()?;
        Ok(())
    }

    pub fn get_config(&self)->Result<&Config,Box<dyn Error>>{
        Ok(match &self.config {
           Some(cfg) => cfg,
           None=>return Err("无效配置文件".into()),
        })
    }

    pub fn build(&mut self)->Result<(),Box<dyn Error>>{
        print_logo();
        logi!("cur build mod is {}",self.absolute_path.display());
        self.gen_build_dir();
        match self.status {
            BuildStatus::BuildSuccess=> return Ok(()),
            _=>{
                let local_lib = self.build_lib()?;//构建当前模块静态库
                let local_bins = self.build_bin()?;//构建当前模块mian.o文件
                self.build_link(&local_bins,local_lib)?;//链接生成可执行文件
                self.status = BuildStatus::BuildSuccess;
            }
        }
        Ok(())
    }

    pub fn run(&self)->Result<(),Box<dyn Error>>{
        let exe_paths = self.get_exe_path()?;
        for exe in exe_paths{
            let mut perms = fs::metadata(&exe)?.permissions();
            #[cfg(unix)] {
                use std::os::unix::fs::PermissionsExt;
                perms.set_mode(0o755); // 设置为可执行权限
                fs::set_permissions(&exe, perms)?;
            }
            let mut run_cmd = Command::new(&exe);
            logi!("run exe file:{:#?}",exe);
            let status = run_cmd.status()?;
            if !status.success() {
                return Err(format!("run of {} failed", exe.display()).into());
            }
        }
        Ok(())
    }

    pub fn is_mod_dir(path:&PathBuf)->bool{
        let mut mod_ = Self::new();
        mod_.get_info(path).is_ok()
    } 

    pub fn clean_build(&mut self)->Result<(),Box<dyn Error>>{
        let build_dir = self.absolute_path.join("build");
        if build_dir.exists(){
            fs::remove_dir_all(build_dir)?;
        }
        Ok(())
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

    fn get_exe_path(&self)->Result<Vec<PathBuf>,Box<dyn Error>>{
        if self.status != BuildStatus::BuildSuccess{
            return Err("mod have no build".into());
        }
        let bin_dir = self.get_build_bin_path()?;
        let mut exe_path = Vec::new();
        if let Some(bin) = &self.bin{
            for main in bin {
                let output_file = bin_dir.join(main.file_stem().unwrap());
                if !output_file.exists(){
                    return Err("exe file not exists".into());
                }
                exe_path.push(output_file);
            }
        }
        Ok(exe_path)
    }

    fn build_lib(&self)->Result<Option<PathBuf>,Box<dyn Error>>{
        if self.src.is_none(){
            return Ok(None);
        }
        let config = self.get_config()?;
        let local_lib = config.name.as_str();
        let target_triple = "x86_64-unknown-linux-gnu";//TODO:当前先支持固定平台，后续拓展多平台
        let object_dir = self.get_build_object_path()?;
        let lib_dir = self.get_build_lib_path()?;

        /*
            1. 编译源文件为静态库（排除bin目录下的源文件）
        */
        let mut lib_build = Build::new();// 生成静态库
        if let Some(src) = &self.src {
            lib_build.files(src);
        }
        if let Some(header) = &self.include {
            lib_build.includes(header);
        }
        lib_build.target(target_triple)
            .host(target_triple)
            .cpp(false) // TODO：基于config.yaml获取类型
            .out_dir(&object_dir)
            .opt_level(2)
            .compile(local_lib);// 编译静态库

        let lib_file = if cfg!(windows) {
            object_dir.join(format!("lib{}.lib", local_lib))
        } else {
            object_dir.join(format!("lib{}.a", local_lib))
        };
        let dest_lib_file = lib_dir.join(lib_file.file_name().unwrap());
        fs::rename(&lib_file, &dest_lib_file)?;
        Ok(Some(dest_lib_file))
    }

    fn build_bin(&mut self)->Result<Vec<PathBuf>,Box<dyn Error>>{
        let object_dir = self.get_build_object_path()?;
        // let bin_dir = self.get_build_bin_path()?;
        let mut output_dirs = Vec::new();
        if let Some(bin) = &self.bin{
            for main in bin {
                let object_file = object_dir.join(main.file_stem().unwrap()).with_extension("o");
                // let output_file = bin_dir.join(main.file_stem().unwrap());//获取后续构建输出文件路径
                let mut compile_cmd = Command::new("gcc");
                if let Some(header) = &self.include {
                    for file in header{
                        compile_cmd.arg("-I").arg(file);
                    }
                }
                compile_cmd
                    .arg("-c")  // 仅编译，不链接
                    .arg(main)
                    .arg("-o")
                    .arg(&object_file);
                logi!("Compiling: {:?}", compile_cmd);
                let status = compile_cmd.status()?;
                if !status.success() {
                    return Err(format!("Compilation of {} failed", main.display()).into());
                }
                output_dirs.push(object_file);
            }}
        Ok(output_dirs)
    }

    fn get_all_dep_mods(&self)->Result<(Vec<PathBuf>,Vec<PathBuf>),Box<dyn Error>>{
        let cfg = self.get_config()?;
        let mods_path = &cfg.dep.hkmod;
        let mut libsdir_path = Vec::new();
        let mut libs_path = Vec::new();
        for name in mods_path {
            let path = PathBuf::from(name);
            let lib_name = path.file_name().unwrap();
            libsdir_path.push(self.absolute_path.join(&path).join("build").join("lib"));
            libs_path.push(self.absolute_path.join(&path).join("build").join("lib").join(format!("lib{}.a",lib_name.to_str().unwrap())));
        }
        Ok((libsdir_path,libs_path))
    }

    fn build_link(&mut self,local_bins:&Vec<PathBuf>,local_lib:Option<PathBuf>)->Result<(),Box<dyn Error>>{
        let local_bin_dir = self.get_build_bin_path()?;
        let local_lib_dir = self.get_build_lib_path()?;
        for bin in local_bins{
            logd!("###@link bin file:{:#?}",bin);
            let output_file = local_bin_dir.join(bin.file_stem().unwrap());
            let mut link_cmd = Command::new("gcc");

            let (other_lib_dir,other_lib_path) = self.get_all_dep_mods()?;
            link_cmd.arg("-L").arg(&local_lib_dir);//链接本地库目录
            for old in &other_lib_dir  {
                link_cmd.arg("-L").arg(old);
            }
            link_cmd.arg(&bin);
            
            if let Some(lib) = &local_lib {
                link_cmd.arg(lib); // 链接库（-lmylib → libmylib.a） 
            }
            for olp in &other_lib_path{
                link_cmd.arg(olp);
            }
            link_cmd.arg("-o")
                    .arg(&output_file);
            for l in self.get_system_lib(){
                link_cmd.arg(l);
            }
            logi!("Linking: {:?}", link_cmd);
            let status = link_cmd.status()?;
            if !status.success() {
                return Err(format!("Linking of {} failed", output_file.display()).into());
            }
        }
        Ok(())
    }

    fn get_system_lib(&self) -> Vec<String> {
        self.config
        .iter()
        .flat_map(|cfg| &cfg.dep.lib)
        .map(|lib_name| format!("-l{}",lib_name))
        .collect()
    }

    fn gen_build_dir(&self){
        let out_dir = self.absolute_path.join("build");
        let bin_dir = out_dir.join("bin");
        let object_dir = out_dir.join("object");
        let lib_dir = out_dir.join("lib");
        fs::create_dir_all(&bin_dir).ok();
        fs::create_dir_all(&object_dir).ok();
        fs::create_dir_all(&lib_dir).ok();
        logd!("{:#?} {:#?} {:#?}",bin_dir,object_dir,lib_dir);     
    }

    fn get_build_bin_path(&self)->Result<PathBuf,Box<dyn Error>>{
        Ok(self.absolute_path.join("build").join("bin"))
    }

    fn get_build_object_path(&self)->Result<PathBuf,Box<dyn Error>>{
        Ok(self.absolute_path.join("build").join("object"))
    }

    fn get_build_lib_path(&self)->Result<PathBuf,Box<dyn Error>>{
        Ok(self.absolute_path.join("build").join("lib"))
    }
    
    fn get_include_of_config(&mut self)->Result<(),Box<dyn Error>>{
        let include = self.get_config()?.dep.include.clone();
        let mods = self.get_config()?.dep.hkmod.clone();
        // 再可变借用 include
        let include_list = self.include.get_or_insert(Vec::new());
        
        for mod_name in mods {
            let path = self.absolute_path.join(mod_name).join("include");
            if let Ok(absolute_path) = fs::canonicalize(&path) {
                if !include_list.contains(&absolute_path) {
                    include_list.push(absolute_path);
                }
            } else {
                loge!("get mod include absolute_path:{:#?} fail", path);
            }
        }

        for file in include {
            let path = self.absolute_path.join(file);
            if let Ok(absolute_path) = fs::canonicalize(&path) {
                if !include_list.contains(&absolute_path) {
                    include_list.push(absolute_path);
                }
            } else {
                loge!("get include absolute_path:{:#?} fail", path);
            }
        }
        Ok(())
    }

    fn get_dir_bin_info(dir_path:&PathBuf)->Result<Vec<PathBuf>,Box<dyn Error>>{
        logi!("get_dir_info {:?}",dir_path);
        let mut file_name = Vec::new();
        match fs::read_dir(dir_path) {
            Ok(entrys)=>{
                for entry in entrys{
                     let path = entry.unwrap().path();
                     let absolute_path = fs::canonicalize(&path)?;
                     if absolute_path.extension().is_some_and(|ext| ext == "c"){//后缀判断
                        file_name.push(absolute_path);
                     }
                }
            }
            Err(_)=>{
                loge!("未找到对于文件{:?}",dir_path);
            }
        }
        Ok(file_name)
    }

    fn get_dir_info(dir_path:&PathBuf)->Result<Vec<PathBuf>,Box<dyn Error>>{
        logi!("get_dir_info {:?}",dir_path);
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
                loge!("未找到对于文件{:?}",dir_path);
            }
        }
        if file_name.is_empty(){
            return Err("have no file".into());
        }
        Ok(file_name)
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
    pub fn get_info(&mut self,path:&PathBuf)->Result<(),Box<dyn Error>>{
        let current_dir = path.clone();
        let mods: Vec<PathBuf> = fs::read_dir(&current_dir)?
                .filter_map(|entry| {
                    match entry {
                        Ok(e)=>{
                            let path = e.path();
                            if ModFile::is_mod_dir(&path){
                                logi!("{:?}获得mod文件目录",path);
                                Some(path)
                            }else {
                                None
                            }
                        }
                        Err(_)=> None,
                    }
                }).collect();
        if mods.is_empty() {
            return Err("have no mod".into());
        }
        for path in &mods {
                let mut modfile = ModFile::new();
                modfile.get_info(path)?;
                let cfg = modfile.get_config()?;
                let name = cfg.name.clone();
                self.modname.push(name.clone());
                self.index.insert(name.clone(), path.clone());
                self.indices.insert(name.clone(), modfile);
            } 
        Ok(())
    
    }
}