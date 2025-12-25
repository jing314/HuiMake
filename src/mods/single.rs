use std::collections::HashMap;
use std::process::Command;
use std::error::Error;
use std::path::{PathBuf,Path};
use cc::Build;
use std::fs;
use crate::utility::logo::print_logo;

use crate::serde::yaml::Config;
use crate::{logd,loge,logi};


/// 表示一个模块（mod）的元数据和构建上下文
#[derive(Debug, Clone)]
pub struct ModFile {
    /// 模块的绝对路径（根目录）
    pub absolute_path: PathBuf,
    /// 模块名
    pub name: String,
    /// 可执行源文件列表（位于 bin/ 目录下，如 main.c）
    pub bin_sources: Option<Vec<PathBuf>>,
    /// 头文件搜索路径（include/ 目录 + 配置中指定的路径）
    pub include_paths: Option<Vec<PathBuf>>,
    /// 库源文件列表（位于 src/ 目录下）
    pub lib_sources: Option<Vec<PathBuf>>,
    /// 从 config.yaml 加载的配置
    pub config: Option<Config>,
}

impl ModFile{
    /// 创建一个空的 `ModFile` 实例
    pub fn new() -> Self {
        Self {
            absolute_path: PathBuf::new(),
            name: String::new(),
            bin_sources: None,
            include_paths: None,
            lib_sources: None,
            config: None,
        }
    }

    /// 从给定路径加载模块信息（目录结构 + 配置）
    pub fn get_info(&mut self, path: &Path) -> Result<(), Box<dyn Error>> {
        logi!("Loading module info from: {:?}", path);
        self.absolute_path = fs::canonicalize(path)?;
        
        // 加载各子目录中的文件
        self.bin_sources = Self::load_c_files_from_dir(&path.join("bin")).ok();
        self.lib_sources = Self::load_all_files_from_dir(&path.join("src")).ok();
        self.include_paths = Self::load_all_files_from_dir(&path.join("include")).ok();

        // 加载 YAML 配置
        self.config = Config::from_yaml(&path.join("config.yaml")).ok();

        if let Some(cfg) = &self.config {
            self.name = cfg.name.clone();
        } else {
            return Err("Failed to load config.yaml".into());
        }

        // 从配置中补充 include 路径（依赖模块 + 显式 include）
        self.merge_includes_from_config()?;
        Ok(())
    }

    /// 获取配置引用，若不存在则返回错误
    pub fn get_config(&self) -> Result<&Config, Box<dyn Error>> {
        self.config.as_ref()
            .ok_or_else(|| "Module configuration is missing".into())
    }

    /// 执行完整构建流程：清理 → 编译库 → 编译二进制 → 链接
    pub fn build(&mut self) -> Result<(), Box<dyn Error>> {
        print_logo();
        println!("Building module: {}", self.absolute_path.display());
        
        self.ensure_build_dirs()?;
        let local_lib = self.build_lib()?;      // 构建静态库（.a）
        let object_files = self.build_bin()?;   // 编译 bin/ 下的 .c 为 .o
        self.link_executables(&object_files, local_lib)?; // 链接生成可执行文件
        
        Ok(())
    }

    /// 运行所有已构建的可执行文件
    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        let exe_paths = self.get_executable_paths()?;
        for exe in exe_paths {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&exe)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&exe, perms)?;
            }

            logi!("Running executable: {:?}", exe);
            let status = Command::new(&exe).status()?;
            if !status.success() {
                return Err(format!("Execution failed: {}", exe.display()).into());
            }
        }
        Ok(())
    }

    /// 判断路径是否为有效模块目录（能成功加载 info）
    pub fn is_mod_dir(path: &Path) -> bool {
        let mut mod_file = Self::new();
        mod_file.get_info(path).is_ok()
    }

    /// 清理 build/ 目录
    pub fn clean_build(&mut self) -> Result<(), Box<dyn Error>> {
        let build_dir = self.absolute_path.join("build");
        if build_dir.exists() {
            fs::remove_dir_all(build_dir)?;
        }
        Ok(())
    }

    /// 生成新模块骨架目录（bin/, include/, src/, config.yaml）
    pub fn gen(mod_name: &str) -> Result<(), Box<dyn Error>> {
        let base = PathBuf::from(mod_name);
        for dir in ["bin", "include", "src"] {
            fs::create_dir_all(base.join(dir))?;
        }

        let config_path = base.join("config.yaml");
        let default_config = Config::new();
        if let Ok(yaml) = default_config.to_yaml() {
            fs::write(config_path, yaml)?;
        }
        Ok(())
    }

    // ———————————————————————— 私有辅助方法 ————————————————————————
    /// 获取所有可执行文件路径（build/bin/ 下）
    fn get_executable_paths(&self) -> Result<Vec<PathBuf>, Box<dyn Error>> {
        let bin_dir = self.get_build_bin_path()?;
        let mut paths = Vec::new();

        if let Some(bin_list) = &self.bin_sources {
            for source in bin_list {
                let exe = bin_dir.join(
                    source.file_stem()
                        .ok_or_else(|| format!("Invalid binary source name: {}", source.display()))?
                );
                if !exe.exists() {
                    return Err(format!("Executable not found: {}", exe.display()).into());
                }
                paths.push(exe);
            }
        }
        Ok(paths)
    }

    /// 构建静态库（使用 cc crate）
    fn build_lib(&self) -> Result<Option<PathBuf>, Box<dyn Error>> {
        if self.lib_sources.is_none() {
            return Ok(None);
        }

        // let config = self.get_config()?;
        let lib_name = self.name.as_str();
        let target_triple = "x86_64-unknown-linux-gnu"; // TODO: 支持多平台

        let object_dir = self.get_build_object_path()?;
        let lib_dir = self.get_build_lib_path()?;

        // 使用 cc 构建静态库
        let mut builder = Build::new();
        if let Some(sources) = &self.lib_sources {
            builder.files(sources);
        }
        if let Some(includes) = &self.include_paths {
            builder.includes(includes);
        }

        builder
            .target(target_triple)
            .host(target_triple)
            .cpp(false) // TODO: 从 config 推断
            .out_dir(&object_dir)
            .opt_level(2)
            .compile(lib_name);

        // 移动库文件到 build/lib/
        let lib_file = if cfg!(windows) {
            object_dir.join(format!("{}.lib", lib_name))
        } else {
            object_dir.join(format!("lib{}.a", lib_name))
        };

        let dest = lib_dir.join(lib_file.file_name().unwrap());
        fs::rename(&lib_file, &dest)?;
        Ok(Some(dest))
    }

    /// 编译 bin/ 下的 .c 文件为 .o 对象文件
    fn build_bin(&mut self) -> Result<Vec<PathBuf>, Box<dyn Error>> {
        let object_dir = self.get_build_object_path()?;

        // 若无二进制源文件，直接返回空
        let sources = match &self.bin_sources {
            Some(list) if !list.is_empty() => list,
            _ => return Ok(Vec::new()),
        };

        let mut object_files = Vec::new();
        for source in sources {
            let stem = source.file_stem()
                .ok_or_else(|| format!("Invalid source filename: {}", source.display()))?;
            let obj = object_dir.join(stem).with_extension("o");

            let mut cmd = Command::new("gcc");
            if let Some(includes) = &self.include_paths {
                for inc in includes {
                    cmd.arg("-I").arg(inc);
                }
            }
            cmd.arg("-c").arg(source).arg("-o").arg(&obj);

            logi!("Compiling: {:?}", cmd);
            let status = cmd.status()?;
            if !status.success() {
                return Err(format!("Compilation failed: {}", source.display()).into());
            }
            object_files.push(obj);
        }
        Ok(object_files)
    }

    /// 获取依赖模块的库路径和链接目录
    fn get_dependency_libs(&self) -> Result<(Vec<PathBuf>, Vec<PathBuf>), Box<dyn Error>> {
        let config = self.get_config()?;
        let mut lib_dirs = Vec::new();
        let mut lib_files = Vec::new();

        for dep_name in &config.dep.hkmod {
            let dep_root = self.absolute_path.join(dep_name);
            let lib_dir = dep_root.join("build").join("lib");
            let lib_file = lib_dir.join(format!("lib{}.a", dep_name));

            lib_dirs.push(lib_dir);
            lib_files.push(lib_file);
        }
        Ok((lib_dirs, lib_files))
    }

        /// 链接所有可执行文件
    fn link_executables(
        &mut self,
        object_files: &[PathBuf],
        local_lib: Option<PathBuf>,
    ) -> Result<(), Box<dyn Error>> {
        let bin_out_dir = self.get_build_bin_path()?;
        let local_lib_dir = self.get_build_lib_path()?;

        let (dep_lib_dirs, dep_lib_files) = self.get_dependency_libs()?;

        for obj in object_files {
            let exe = bin_out_dir.join(
                obj.file_stem()
                    .ok_or_else(|| format!("Invalid object file: {}", obj.display()))?
            );

            let mut cmd = Command::new("gcc");
            cmd.arg("-L").arg(&local_lib_dir);
            for dir in &dep_lib_dirs {
                cmd.arg("-L").arg(dir);
            }
            cmd.arg(obj);

            if let Some(lib) = &local_lib {
                cmd.arg(lib);
            }
            for lib in &dep_lib_files {
                cmd.arg(lib);
            }

            cmd.arg("-o").arg(&exe);
            for sys_lib in self.get_system_libs() {
                cmd.arg(sys_lib);
            }

            logi!("Linking: {:?}", cmd);
            let status = cmd.status()?;
            if !status.success() {
                return Err(format!("Linking failed: {}", exe.display()).into());
            }
        }
        Ok(())
    }


    /// 从配置中提取系统库（如 -lpthread）
    fn get_system_libs(&self) -> Vec<String> {
        self.config
            .iter()
            .flat_map(|cfg| &cfg.dep.lib)
            .map(|name| format!("-l{}", name))
            .collect()
    }


    /// 确保 build/ 子目录存在
    fn ensure_build_dirs(&self) -> Result<(), Box<dyn Error>> {
        let base = self.absolute_path.join("build");
        for subdir in ["bin", "object", "lib"] {
            fs::create_dir_all(base.join(subdir))?;
        }
        logd!("Build directories created under: {:?}", base);
        Ok(())
    }

    /// 获取构建输出路径（带存在性检查）
    fn get_build_bin_path(&self)->Result<PathBuf,Box<dyn Error>>{
        let path = self.absolute_path.join("build").join("bin");
        if !path.exists(){
            return Err("build bin path not exists".into());
        }
        Ok(path)
    }

    fn get_build_object_path(&self)->Result<PathBuf,Box<dyn Error>>{
        let path = self.absolute_path.join("build").join("object");
        if !path.exists(){
            return Err("build object path not exists".into());
        }
        Ok(path)
    }

    fn get_build_lib_path(&self)->Result<PathBuf,Box<dyn Error>>{
        let path = self.absolute_path.join("build").join("lib");
        if !path.exists(){
            return Err("build lib path not exists".into());
        }
        Ok(path)
    }
    
    /// 从 config.yaml 合并 include 路径：
    /// - 依赖模块的 include/
    /// - 配置中显式列出的 include 路径
    fn merge_includes_from_config(&mut self) -> Result<(), Box<dyn Error>> {
        let config = match &self.config {
            Some(cfg) => cfg,
            None => return Err(format!("No config found in {:?}", self.absolute_path).into()), // 无配置则跳过
        };

        let includes = self.include_paths.get_or_insert_with(Vec::new);

        // 添加依赖模块的 include/
        for dep in &config.dep.hkmod {
            let path = self.absolute_path.join(dep).join("include");
            if let Ok(abs) = fs::canonicalize(&path) {
                if !includes.contains(&abs) {
                    includes.push(abs);
                }
            } else {
                loge!("Failed to canonicalize dependency include: {:?}", path);
            }
        }

        // 添加配置中显式 include 路径
        for rel_path in &config.dep.include {
            let path = self.absolute_path.join(rel_path);
            if let Ok(abs) = fs::canonicalize(&path) {
                if !includes.contains(&abs) {
                    includes.push(abs);
                }
            } else {
                loge!("Failed to canonicalize include path: {:?}", path);
            }
        }

        Ok(())
    }

    /// 加载目录下所有 .c 文件（用于 bin/）
    fn load_c_files_from_dir(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
        logi!("Scanning C files in: {:?}", dir);
        let mut files = Vec::new();

        if !dir.exists() {
            return Ok(files); // 允许目录不存在
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if path.extension().map_or(false, |ext| ext == "c") {
                    files.push(fs::canonicalize(&path)?);
                }
            }
        }
        Ok(files)
    }

/// 加载目录下所有文件（用于 src/ 和 include/）
    fn load_all_files_from_dir(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
        logi!("Scanning all files in: {:?}", dir);
        let mut files = Vec::new();

        if !dir.exists() {
            return Ok(files); // 目录可选
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                files.push(fs::canonicalize(&path)?);
            }
        }
        Ok(files)
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

        //读取当前目录下的所有模块目录
        let mods: Vec<PathBuf> = fs::read_dir(&current_dir)?
                .filter_map(|entry| {
                    match entry {
                        Ok(e)=>{
                            let path = e.path();
                            if ModFile::is_mod_dir(&path){
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
                self.modname.push(modfile.name.clone());
                self.index.insert(modfile.name.clone(), path.clone());
                self.indices.insert(modfile.name.clone(), modfile);
            } 
        Ok(())
    
    }
}