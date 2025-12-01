
use serde_yaml;
pub mod yaml{
    use std::{collections::BTreeMap, fs::File, io::Read,error::Error};

    pub struct Dep{
        include:Vec<String>,
        hkmod:Vec<String>,
        lib:Vec<String>,
    }
    impl Dep {
        fn new () ->Dep
        {
            Dep { 
                    include: Vec::new(),
                    hkmod: Vec::new(), 
                    lib: Vec::new(),
                }
        }
    }
    pub struct Config{
        pub name:String,
        pub std:String,
        pub premacro:Vec<String>,
        pub dep:Dep,
    }

    impl Config {
        fn new() -> Config{
            Config { 
                name: String::from("App"),
                std: String::from("C99"),
                premacro: Vec::new(),
                dep: Dep::new(),
            }
        }
        fn load(&mut self,path:String)->Result<(),Box<dyn Error>>{
            let mut file = File::open(path)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            let deserialized_map:BTreeMap<String,serde_yaml::Value> = serde_yaml::from_str(&contents)?;
            if let Some(name_value) = deserialized_map.get("name") {
                if let Some(name_str) = name_value.as_str(){
                    self.name = name_str.to_string();
                }
            }

            if let Some(std_value) = deserialized_map.get("std") {
                if let Some(std_str) = std_value.as_str() {
                    self.std = std_str.to_string();
                }
            }

            if let Some(premacro_value) = deserialized_map.get("premacro") {
                if let Some(premacro_seq) = premacro_value.as_sequence() {
                    self.premacro = premacro_seq.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                }
            }

            if let Some(dep_value) = deserialized_map.get("dep")  {
                if let Some(dep_map) = dep_value.as_mapping() {
                    if let Some(include_value) =  dep_map.get("include"){
                        if let Some(include_seq) = include_value.as_sequence() {
                                self.dep.include = include_seq.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                        }
                    }
                    if let Some(hkmod_value) = dep_map.get("hkmod") {
                        if let Some(hkmod_seq) = hkmod_value.as_sequence() {
                                self.dep.hkmod = hkmod_seq.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                        }
                    }
                    if let Some(lib_value) = dep_map.get("lib") {
                        if let Some(lib_seq) = lib_value.as_sequence() {
                                self.dep.lib = lib_seq.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                        }
                    }

                }
            }

            Ok(())
        }
        fn from_yaml(path:String)->Result<Config, Box<dyn Error>>
        {
            let mut file = File::open(path)?;
            let mut contents = String::new();

            file.read_to_string(&mut contents)?;
            let deserialized_map:BTreeMap<String,serde_yaml::Value> = serde_yaml::from_str(&contents)?;
            let mut config = Config::new();
            if let Some(name_value) = deserialized_map.get("name") {
                if let Some(name_str) = name_value.as_str(){
                    config.name = name_str.to_string();
                }
            }

            if let Some(std_value) = deserialized_map.get("std") {
                if let Some(std_str) = std_value.as_str() {
                    config.std = std_str.to_string();
                }
            }

            if let Some(premacro_value) = deserialized_map.get("premacro") {
                if let Some(premacro_seq) = premacro_value.as_sequence() {
                    config.premacro = premacro_seq.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                }
            }

            if let Some(dep_value) = deserialized_map.get("dep")  {
                if let Some(dep_map) = dep_value.as_mapping() {
                    if let Some(include_value) =  dep_map.get("include"){
                        if let Some(include_seq) = include_value.as_sequence() {
                                config.dep.include = include_seq.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                        }
                    }
                    if let Some(hkmod_value) = dep_map.get("hkmod") {
                        if let Some(hkmod_seq) = hkmod_value.as_sequence() {
                                config.dep.hkmod = hkmod_seq.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                        }
                    }
                    if let Some(lib_value) = dep_map.get("lib") {
                        if let Some(lib_seq) = lib_value.as_sequence() {
                                config.dep.lib = lib_seq.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect();
                        }
                    }

                }
            }

            Ok(config)
        }
    }
} 