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
use crate::{loge, logi};
use crate::{mods::single::{ModFile, ProjectMap}, serde::yaml::{Config, Dep}};
#[derive(Debug)]
pub struct ModsManage{
    pub graph: DiGraph<String,()>,
    pub indices: HashMap<String, NodeIndex>,
    pub project_map: ProjectMap,
}
impl ModsManage{
    pub fn new()->Self{
        ModsManage { 
            graph:DiGraph::new() , 
            indices: HashMap::new(), 
            project_map: ProjectMap::new() 
        } 
    }
    /*
    构建mods之间的关联图
    */
    pub fn gen_mods_depsgraph(path:&PathBuf)->Result<Self,Box<dyn Error>>{
        logi!("gen_mods_depsgraph path:{:#?}",path);
        //1.获取工具运行目录的现有模块信息
        let mut project_map = ProjectMap::new();
        project_map.get_info(path)?;

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
            let cfg = project_map.indices[modname].get_config()?;
            let hkmods = &cfg.dep.hkmod;
            for depmod in  hkmods {
                let dep_mod_clean = Dep::clean_hk_name_string(&depmod);
                if let Some(&dep_idx) = indices.get(dep_mod_clean.as_str()) {
                    graph.add_edge(dep_idx, cur_idx, ());
                }
            }
        }
        Ok(ModsManage { graph,indices,project_map })
    }

    /*
    如果图节点个数不为0
        判断当前图节点中是否存在无入度的节点
        如果存在，将其打包到Vec中返回
        如果不存在，返回err
    如果节点个数为0
        返回空Vec
    */
    fn check_loopdep(&self)->Result<Vec<NodeIndex> ,Box<dyn Error>>{
        let graph = &self.graph;
        if graph.node_count() > 0{
            let list: Vec<NodeIndex> = graph
                .node_identifiers().filter(|&node| {
                    graph.neighbors_directed(node, petgraph::Direction::Incoming)
                    .next()
                    .is_none()
                }
            ).collect();
            if list.is_empty() {
                return Err("循环引用".into());
            }
            Ok(list)
        }else {
            Ok(Vec::new())
        }
    }

    fn del_node(&mut self,list: Vec<NodeIndex>){
        for node in list {
            self.graph.remove_node(node);
        }
    }

    pub fn get_next(&mut self)->Result<Vec<ModFile>,Box<dyn Error>> {
        let mut next_build:Vec<ModFile> = Vec::new();
        let build_list = Self::check_loopdep(self)?;//获取一个或多个没有入度的图节点
        
        for node_id in &build_list{
            match self.graph.node_weight(*node_id) 
            {
                Some(name)=>{
                    let modfile = self.project_map.indices.get(name).cloned().unwrap();
                    next_build.push(modfile);
                }
                None=>{}
            }
        }       
        self.del_node(build_list);//删除此节点以及其与其它节点的关系
        Ok(next_build)
    }
}