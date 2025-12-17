use cc::Build;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::loge;
use crate::logd;
use crate::logi;
use crate::mods::single::ModFile;
/*
    buid目录构建
*/
pub fn gen_build_dir(modfile: &ModFile)->(PathBuf,PathBuf,PathBuf)
{
    let out_dir = modfile.absolute_path.join("build");
    let bin_dir = out_dir.join("bin");
    let object_dir = out_dir.join("object");
    let lib_dir = out_dir.join("lib");
    fs::create_dir_all(&bin_dir).ok();
    fs::create_dir_all(&object_dir).ok();
    fs::create_dir_all(&lib_dir).ok();
    logd!("{:#?} {:#?} {:#?}",bin_dir,object_dir,lib_dir);     
    (bin_dir,object_dir,lib_dir)  
}

pub fn clean_build_dir(modfile:&ModFile)->Result<(),Box<dyn Error>>{
    let build_dir = modfile.absolute_path.join("build");
    if build_dir.exists(){
        fs::remove_dir_all(build_dir)?;
    }
    Ok(())
}
fn get_output_name(binfile: &PathBuf) -> String {
    binfile.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("app")
        .to_string()
}

fn get_system_lib(slib: &Vec<String>) -> Vec<String> {
    let mut result = Vec::new();
    for lib_name in slib {
        result.push(format!("-l{}", lib_name));
    }
    result
}

pub fn build_single_c_mod(modfile: &ModFile)->Result<(),Box<dyn Error>>{
    build_single_c(modfile)
    
}

/*
构建顺序暂定为：
0.创建build目录
1.基于非bin目录下所有源文件链接为单个库
2.基于编译后的库文件加上bin目录的main生成可执行文件
*/

/*

*/


pub fn build_single_c(modfile: &ModFile)->Result<(),Box<dyn Error>>{
    let lib_name = modfile.config.name.as_str();
    let target_triple = "x86_64-unknown-linux-gnu";    //TODO:当前先支持固定平台，后续拓展多平台
    clean_build_dir(modfile)?;
    let (bin_dir,object_dir,lib_dir) = gen_build_dir(modfile);
                                                                                      
/*
    1. 编译源文件为静态库（排除bin目录下的源文件）
*/
    let mut lib_build = Build::new();// 生成静态库
    lib_build
        .files(&modfile.src)
        .includes(&modfile.include)
        .target(target_triple)
        .host(target_triple)
        .cpp(false) // TODO：基于config.yaml获取类型
        .out_dir(&object_dir)
        .opt_level(2);
        lib_build.compile(&lib_name);// 编译静态库

    let lib_file = if cfg!(windows) {
        object_dir.join(format!("lib{}.lib", lib_name))
    } else {
        object_dir.join(format!("lib{}.a", lib_name))
    };
    let dest_lib_file = lib_dir.join(lib_file.file_name().unwrap());
    fs::rename(&lib_file, &dest_lib_file)?;
/*
    2. 直接调用gcc编译bin目录的main文件并链接静态库
*/
    for mainfile in &modfile.bin{
        let object_file = object_dir.join(mainfile.file_stem().unwrap()).with_extension("o");
        let output_file = bin_dir.join(mainfile.file_stem().unwrap());

        let mut compile_cmd = Command::new("gcc");
        for include_file in &modfile.include{
            compile_cmd.arg("-I").arg(include_file);
        }
        
        compile_cmd
            .arg("-c")  // 仅编译，不链接
            .arg(mainfile)
            .arg("-o")
            .arg(&object_file);
        logi!("Compiling: {:?}", compile_cmd);
        let status = compile_cmd.status()?;
        if !status.success() {
            return Err(format!("Compilation of {} failed", mainfile.display()).into());
        }

        let mut link_cmd = Command::new("gcc");
        let lib = get_system_lib(&modfile.config.dep.lib);
        link_cmd
                .arg("-L")  // 指定库目录
                .arg(&lib_dir)
                .arg(&object_file)
                .arg("-l")  // 链接库（-lmylib → libmylib.a）
                .arg(lib_name)
                .arg("-o")
                .arg(&output_file);
        for l in lib{
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