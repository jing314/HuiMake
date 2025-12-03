use std::{fs, path::PathBuf,error::Error , collections::HashMap};
use petgraph::graph::{DiGraph, NodeIndex};

use crate::{hkmod::modlue::ModFile, serde::yaml::{Config, Dep}};
#[derive(Debug)]
pub struct ModsManage{
    pub graph: DiGraph<String,()>,
    indices: HashMap<String, NodeIndex>,
}
impl ModsManage{
    //获取目录信息解析为ModFile
    pub fn get_mods_list()->Result<Vec<ModFile>,Box<dyn Error>>
    {
        let current_dir = PathBuf::from(".");
        let modules: Result<Vec<ModFile>, Box<dyn Error>>  = fs::read_dir(&current_dir)?
                .filter_map(|entry| {
                    match entry {
                        Ok(e)=>{
                            let path = e.path();
                            if path.is_dir(){
                                Some(path)
                            }else {
                                None
                            }
                        }
                        Err(_)=> None,
                    }
                })
                .filter_map( |dir_path|{
                    match ModFile::get_info(&dir_path) {
                        Ok(mod_info) => Some(Ok(mod_info)),
                        Err(e)=>{
                            eprintln!("警告: 无法加载目录 {:?}: {}", dir_path, e);
                            None
                        }
                        
                    }
                })
                .collect();
        modules
    }
    //构建mods之间的关联图
    pub fn build_mods_depsgraph()->Result<Self,Box<dyn Error>>{
        //1.获取工具运行目录的现有模块信息
        let list = Self::get_mods_list()?;

        let mut graph = DiGraph::new();
        let mut indices = HashMap::new();
        //添加节点
        for module in &list{
            let idx = graph.add_node(module.config.name.clone()); 
            indices.insert(module.config.name.clone(), idx);
        }
        //确定边
        for modlue in list{
            let cur_idx = indices[&modlue.config.name];
            //变量指向此mod的其他mod，添加到图中
            for depmod in &modlue.config.dep.hkmod {
                let dep_mod_clean = Dep::clean_hk_name_string(&depmod);
                if let Some(&dep_idx) = indices.get(dep_mod_clean.as_str()) {
                    graph.add_edge(dep_idx, cur_idx, ());
                }
            }
        }
        Ok(ModsManage { graph,indices })
    }
    fn check_mods(&self)->bool{
        //如果当前图剩余节点存在循环引用，返回false
        true
    }

    fn get_next_build_mod(&mut self)->Result<Vec<ModFile>,Box<dyn Error>> {
        let next_build = Vec::new();
        //获取一个或多个没有入度的图节点

        //将没有入度的图节点存入Vec中

        //删除此节点以及其与其它节点的关系

        Ok(next_build)
    }
}