use std::{
            collections::HashMap, error::Error, fs, path::PathBuf
};

use petgraph::{
                graph::{
                    self, 
                    DiGraph, 
                    NodeIndex}, 
                visit::IntoNodeIdentifiers
            };
use crate::loge;
use crate::{mods::single::{ModFile, ProjectMap}, serde::yaml::{Config, Dep}};
#[derive(Debug)]
pub struct ModsManage{
    pub graph: DiGraph<String,()>,
    pub indices: HashMap<String, NodeIndex>,
    pub project_map: ProjectMap,
}
impl ModsManage{
    //构建mods之间的关联图
    pub fn build_mods_depsgraph()->Result<Self,Box<dyn Error>>{
        //1.获取工具运行目录的现有模块信息
        let mut project_map = ProjectMap::new();
        project_map.get_project_info()?;

        let mut graph = DiGraph::new();
        let mut indices = HashMap::new();
        //添加节点
        for modname in &project_map.modname{
            println!("add mod name is {}",modname);
            let idx = graph.add_node(modname.clone()); 
            indices.insert(modname.clone(), idx);
        }
        //确定边
        for modname in &project_map.modname{
            let cur_idx = indices[modname];
            //变量指向此mod的其他mod，添加到图中
            let hkmods = &project_map.indices[modname].config.dep.hkmod;
            for depmod in  hkmods {
                let dep_mod_clean = Dep::clean_hk_name_string(&depmod);
                if let Some(&dep_idx) = indices.get(dep_mod_clean.as_str()) {
                    graph.add_edge(dep_idx, cur_idx, ());
                }
            }
        }
        Ok(ModsManage { graph,indices,project_map })
    }
    fn check_mods(&self)->Result<Vec<NodeIndex> ,Box<dyn Error>>{
        //如果当前图剩余节点存在循环引用，返回false
        let graph = &self.graph;
        let list: Vec<NodeIndex> = graph
                    .node_identifiers().filter(|&node| {
                        graph.neighbors_directed(node, petgraph::Direction::Incoming)
                        .next()
                        .is_none()
                    }
                ).collect();

        Ok(list)
    }

    fn del_node(&mut self,list: Vec<NodeIndex>)->Result<(),Box<dyn Error>>{
        if !list.is_empty() {
            for node in list {
                self.graph.remove_node(node);
            }
        }

        Ok(())
    }
    pub fn get_next_build_mod(&mut self)->Result<Vec<ModFile>,Box<dyn Error>> {
        let mut next_build:Vec<ModFile> = Vec::new();
        //获取一个或多个没有入度的图节点
        let build_list = Self::check_mods(self)? ;
        
        if !build_list.is_empty(){
            for node_id in &build_list{
                match  self.graph.node_weight(*node_id) {
                    Some(name)=>{
                        let modfile = self.project_map.indices.get(name).cloned().unwrap();
                        next_build.push(modfile);
                        
                    }
                    None=>{}
                }
            }
        }else {
            loge!("循环引用错误");
        }
        
        //将没有入度的图节点存入Vec中

        //删除此节点以及其与其它节点的关系
        self.del_node(build_list)?;

        Ok(next_build)
    }
}