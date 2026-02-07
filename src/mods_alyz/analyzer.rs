use crate::{logd, loge, logi};
use crate::{
    mods_alyz::single::{ModFile, ProjectMap},
    utility::yaml::{Config, Dep},
};
use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::IntoNodeIdentifiers,
};
use std::{collections::HashMap, error::Error, path::PathBuf};

#[derive(Debug)]
pub struct ModsManage {
    pub graph: DiGraph<String, ()>,
    pub project_map: ProjectMap,
}

impl ModsManage {
    pub fn new() -> Self {
        ModsManage {
            graph: DiGraph::new(),
            project_map: ProjectMap::new(),
        }
    }

    ///构建mods之间的关联图
    pub fn gen_mods_depsgraph(&mut self, path: &PathBuf) -> Result<(), Box<dyn Error>> {
        logi!("gen_mods_depsgraph path:{:#?}", path);
        let mut indices = HashMap::new();

        //1.获取工具运行目录的现有模块信息
        self.project_map.get_info(path)?;

        //添加节点
        for modname in &self.project_map.modname {
            logi!("add mod name is {}", modname);
            let idx = self.graph.add_node(modname.clone());
            indices.insert(modname.clone(), idx);
        }

        //确定边
        for modname in &self.project_map.modname {
            let cur_idx = indices[modname];

            //变量指向此mod的其他mod，添加到图中
            let cfg = self.project_map.indices[modname].get_config()?;
            let hkmods = &cfg.dep.hkmod;
            for depmod in hkmods {
                let dep_mod_clean = Dep::clean_hk_name_string(&depmod);
                if let Some(&dep_idx) = indices.get(dep_mod_clean.as_str()) {
                    self.graph.add_edge(dep_idx, cur_idx, ());
                }
            }
        }
        Ok(())
    }

    ///获取下一个可以构建的mod列表
    pub fn get_next(&mut self) -> Result<Vec<ModFile>, Box<dyn Error>> {
        let mut next_build: Vec<ModFile> = Vec::new();

        //获取一个或多个没有入度的图节点
        let build_list = Self::check_loopdep(self)?;

        for node_id in &build_list {
            match self.graph.node_weight(*node_id) {
                Some(name) => {
                    if let Some(modfile) = self.project_map.indices.get(name) {
                        next_build.push(modfile.clone());
                    }
                }
                None => {}
            }
        }

        //删除此节点以及其与其它节点的关系
        self.del_node(build_list);
        Ok(next_build)
    }

    ///检查是否存在环依赖，返回无入度节点列表
    fn check_loopdep(&self) -> Result<Vec<NodeIndex>, Box<dyn Error>> {
        let graph = &self.graph;
        if graph.node_count() > 0 {
            let list: Vec<NodeIndex> = graph
                .node_identifiers()
                .filter(|&node| {
                    graph
                        .neighbors_directed(node, petgraph::Direction::Incoming)
                        .next()
                        .is_none()
                })
                .collect();
            if list.is_empty() {
                return Err("cur project exit loop include".into());
            }
            Ok(list)
        } else {
            Ok(Vec::new())
        }
    }

    ///删除节点
    fn del_node(&mut self, list: Vec<NodeIndex>) {
        for node in list {
            self.graph.remove_node(node);
        }
    }
}
