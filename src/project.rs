//! Project discovery: locate the project root, load modules, resolve dependencies
//! and compute a topological order.
//!
//! Dependency model: deps.mods in config.yaml holds a module name (or a directory
//! name, or a relative path). Modules are sibling directories and an unresolved
//! dependency is a hard error, never silently ignored.

use crate::config::{Config, GlobalConfig, MODULE_FILE_NAME};
use crate::error::{HkError, Result, ResultExt};
use crate::util;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum directory depth when scanning for modules.
const MAX_SCAN_DEPTH: usize = 6;

/// Directory names that are never descended into.
const SKIP_DIRS: [&str; 5] = ["node_modules", "target", ".git", ".svn", ".hg"];

#[derive(Debug, Clone)]
pub struct ModInfo {
    pub name: String,
    /// Absolute (normalized) module directory.
    pub dir: PathBuf,
    pub config: Config,
}

impl ModInfo {}

#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub global_file: Option<PathBuf>,
    pub global: GlobalConfig,
    pub modules: IndexMap<String, ModInfo>,
    /// Set when hk runs inside a module directory.
    pub current: Option<String>,
}

impl Project {
    pub fn discover(cwd: &Path) -> Result<Project> {
        let cwd = util::normalize(cwd).with_context(|| format!("resolving the current directory {}", cwd.display()))?;
        let mod_dir = if cwd.join(MODULE_FILE_NAME).is_file() { Some(cwd.clone()) } else { None };
        let global_root = GlobalConfig::find_root(&cwd);
        let root = match (&global_root, &mod_dir) {
            (Some(dir), _) => dir.clone(),
            (None, Some(dir)) => dir.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| dir.clone()),
            (None, None) => cwd.clone(),
        };

        let global = GlobalConfig::load(global_root.as_deref())?;

        let mut dirs = find_module_dirs(&root, &global.build_dir)?;
        if let Some(local) = &mod_dir {
            if !dirs.iter().any(|d| d == local) {
                dirs.push(local.clone());
            }
        }
        if dirs.is_empty() {
            return Err(HkError::new(format!(
                "no modules found under {} (a module directory must contain {})",
                root.display(),
                MODULE_FILE_NAME
            )));
        }
        dirs.sort();
        dirs.dedup();

        let mut modules: IndexMap<String, ModInfo> = IndexMap::new();
        let mut by_dir: HashMap<PathBuf, String> = HashMap::new();
        for dir in dirs {
            let config_file = dir.join(MODULE_FILE_NAME);
            let config = Config::load_file(&config_file)?;
            config.validate(&config_file.display().to_string())?;
            let name = config.effective_name(&dir)?;
            let abs = util::normalize(&dir)?;
            if let Some(prev) = by_dir.get(&abs) {
                return Err(HkError::new(format!(
                    "module directory {} loaded twice (modules {prev} and {name})",
                    abs.display()
                )));
            }
            if let Some(existing) = modules.get(&name) {
                return Err(HkError::new(format!(
                    "duplicate module name {name:?}:\n     {}\n     {}",
                    existing.dir.display(),
                    abs.display()
                ))
                .context(
                    "if these modules belong to different projects you are running in a parent\
                     directory: cd into the project, or select one with --module NAME",
                )
                .context("if they do belong to the same project, give them different explicit names in their config.yaml"));
            }
            by_dir.insert(abs.clone(), name.clone());
            modules.insert(name.clone(), ModInfo { name, dir: abs, config });
        }

        let current = match &mod_dir {
            Some(dir) => {
                let abs = util::normalize(dir)?;
                by_dir.get(&abs).cloned()
            }
            None => None,
        };

        let project = Project {
            root,
            global_file: global_root.as_ref().and_then(|r| GlobalConfig::find_file(r)),
            global,
            modules,
            current,
        };
        project.check_deps()?;
        Ok(project)
    }

    pub fn module(&self, name: &str) -> Result<&ModInfo> {
        self.modules
            .get(name)
            .ok_or_else(|| HkError::new(format!("unknown module {name:?}; available: {}", self.module_names())))
    }

    pub fn module_names(&self) -> String {
        self.modules.keys().cloned().collect::<Vec<_>>().join(", ")
    }

    /// Every dependency must resolve and the graph must be acyclic.
    fn check_deps(&self) -> Result<()> {
        for (name, module) in &self.modules {
            for dep in &module.config.deps.mods {
                let resolved = self.resolve_dep(name, dep).map_err(|e| {
                    e.context(format!(
                        "the deps.mods entry {dep:?} of module {name} ({})",
                        module.dir.display()
                    ))
                })?;
                if resolved == *name {
                    return Err(HkError::new(format!("module {name} depends on itself")));
                }
            }
        }
        self.topo_levels()?;
        Ok(())
    }

    /// Resolve a dependency entry to a module name: accepts a module name, a
    /// directory name or a relative path.
    pub fn resolve_dep(&self, from: &str, dep: &str) -> Result<String> {
        if self.modules.contains_key(dep) {
            return Ok(dep.to_string());
        }
        let trimmed = dep.trim_end_matches(['/', '\\']);
        if let Some((name, _)) = self
            .modules
            .iter()
            .find(|(_, m)| m.dir.file_name().map(|f| f == trimmed).unwrap_or(false))
        {
            return Ok(name.clone());
        }
        if dep.contains('/') || dep.contains('\\') || dep == "." || dep == ".." {
            if let Some(origin) = self.modules.get(from) {
                let candidate = origin.dir.join(dep);
                if let Ok(abs) = util::normalize(&candidate) {
                    if let Some((name, _)) = self.modules.iter().find(|(_, m)| m.dir == abs) {
                        return Ok(name.clone());
                    }
                }
            }
        }
        Err(HkError::new(format!(
            "cannot resolve dependency {dep:?} (tried module name, directory name and relative path); modules in this project: {}",
            self.module_names()
        )))
    }

    /// Transitive dependencies in topological order (dependencies first, self excluded).
    pub fn dep_closure(&self, name: &str) -> Result<Vec<String>> {
        let mut seen = HashSet::new();
        let mut visiting = Vec::new();
        let mut out = Vec::new();
        self.visit_deps(name, &mut seen, &mut visiting, &mut out)?;
        out.retain(|n| n != name);
        Ok(out)
    }

    fn visit_deps(
        &self,
        name: &str,
        seen: &mut HashSet<String>,
        visiting: &mut Vec<String>,
        out: &mut Vec<String>,
    ) -> Result<()> {
        if seen.contains(name) {
            return Ok(());
        }
        if visiting.iter().any(|v| v == name) {
            let mut chain = visiting.clone();
            chain.push(name.to_string());
            return Err(HkError::new(format!("dependency cycle detected: {}", chain.join(" -> "))));
        }
        visiting.push(name.to_string());
        let module = self.module(name)?;
        for dep in &module.config.deps.mods {
            let dep_name = self.resolve_dep(name, dep)?;
            self.visit_deps(&dep_name, seen, visiting, out)?;
        }
        visiting.pop();
        seen.insert(name.to_string());
        out.push(name.to_string());
        Ok(())
    }

    /// Topological levels of the whole project; modules in one level can build in parallel.
    pub fn topo_levels(&self) -> Result<Vec<Vec<String>>> {
        let mut indegree: HashMap<String, usize> =
            self.modules.keys().map(|k| (k.clone(), 0)).collect();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for (name, module) in &self.modules {
            for dep in &module.config.deps.mods {
                let dep_name = self.resolve_dep(name, dep)?;
                dependents.entry(dep_name).or_default().push(name.clone());
                *indegree.entry(name.clone()).or_insert(0) += 1;
            }
        }

        let mut remaining: Vec<String> = self.modules.keys().cloned().collect();
        let mut levels = Vec::new();
        loop {
            let level: Vec<String> = remaining
                .iter()
                .filter(|name| indegree.get(*name).copied().unwrap_or(0) == 0)
                .cloned()
                .collect();
            if level.is_empty() {
                break;
            }
            for name in &level {
                remaining.retain(|n| n != name);
                if let Some(children) = dependents.get(name) {
                    for child in children {
                        if let Some(entry) = indegree.get_mut(child) {
                            *entry = entry.saturating_sub(1);
                        }
                    }
                }
            }
            levels.push(level);
        }

        if !remaining.is_empty() {
            return Err(HkError::new(format!(
                "dependency cycle detected, involving: {}",
                remaining.join(", ")
            )));
        }
        Ok(levels)
    }

    /// Flattened topological order (used by --print-plan and by run).
    pub fn topo_order(&self, scope: Option<&str>) -> Result<Vec<String>> {
        let keep: Option<HashSet<String>> = match scope {
            Some(name) => {
                let mut set: HashSet<String> = self.dep_closure(name)?.into_iter().collect();
                set.insert(name.to_string());
                Some(set)
            }
            None => None,
        };
        let mut out = Vec::new();
        for level in self.topo_levels()? {
            for name in level {
                if keep.as_ref().map(|k| k.contains(&name)).unwrap_or(true) {
                    out.push(name);
                }
            }
        }
        Ok(out)
    }
}

fn find_module_dirs(root: &Path, build_dir: &str) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));
    while let Some((dir, depth)) = queue.pop_front() {
        if dir.join(MODULE_FILE_NAME).is_file() {
            found.push(dir.clone());
        }
        if depth >= MAX_SCAN_DEPTH {
            continue;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };
            if name == build_dir || name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            // A child directory that carries its own project config belongs to another
            // project and acts as a scan boundary; otherwise modules of several projects
            // would be merged into one and easily collide by name.
            if GlobalConfig::find_file(&path).is_some() {
                crate::log_info!("skipping nested project {} (it has its own project config)", path.display());
                continue;
            }
            queue.push_back((path, depth + 1));
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn temp_project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hk-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn discovers_modules_and_orders_by_dependency() {
        let root = temp_project("order");
        write(&root.join("app/config.yaml"), "name: app\ndeps:\n  mods: [hal]\n");
        write(&root.join("hal/config.yaml"), "name: hal\n");
        write(&root.join("drv/config.yaml"), "kind: static-lib\n");
        let project = Project::discover(&root).unwrap();
        assert_eq!(project.modules.len(), 3);
        let levels = project.topo_levels().unwrap();
        assert_eq!(levels[0].len(), 2, "modules without dependencies belong to the first level: {levels:?}");
        assert_eq!(levels[1], vec!["app".to_string()]);
        assert_eq!(project.dep_closure("app").unwrap(), vec!["hal".to_string()]);
    }

    #[test]
    fn duplicate_module_names_are_rejected() {
        let root = temp_project("dup");
        write(&root.join("a/config.yaml"), "name: same\n");
        write(&root.join("b/config.yaml"), "name: same\n");
        let err = Project::discover(&root).unwrap_err().to_string();
        assert!(err.contains("duplicate module name"), "{err}");
    }

    #[test]
    fn missing_dependency_is_an_error_with_module_context() {
        let root = temp_project("missing");
        write(&root.join("app/config.yaml"), "name: app\ndeps:\n  mods: [nope]\n");
        let err = Project::discover(&root).unwrap_err().to_string();
        assert!(err.contains("app") && err.contains("nope"), "{err}");
    }

    #[test]
    fn cycles_are_reported() {
        let root = temp_project("cycle");
        write(&root.join("a/config.yaml"), "name: a\ndeps:\n  mods: [b]\n");
        write(&root.join("b/config.yaml"), "name: b\ndeps:\n  mods: [a]\n");
        let err = Project::discover(&root).unwrap_err().to_string();
        assert!(err.contains("cycle"), "{err}");
    }

    #[test]
    fn nested_project_roots_act_as_scan_boundaries() {
        let root = temp_project("nested");
        // an ordinary module in the parent directory
        write(&root.join("top/config.yaml"), "name: top\n");
        // two nested projects: both carry hk.yaml and both have a module named bsp
        write(&root.join("alpha/hk.yaml"), "version: 1\n");
        write(&root.join("alpha/bsp/config.yaml"), "name: bsp\n");
        write(&root.join("beta/hk.yaml"), "version: 1\n");
        write(&root.join("beta/bsp/config.yaml"), "name: bsp\n");

        // running in the parent directory only sees the parent's own modules
        let project = Project::discover(&root).unwrap();
        assert_eq!(
            project.modules.keys().cloned().collect::<Vec<_>>(),
            vec!["top".to_string()],
            "nested projects must act as scan boundaries"
        );

        // inside a nested project only its own modules are visible
        let alpha = Project::discover(&root.join("alpha")).unwrap();
        assert_eq!(alpha.modules.keys().cloned().collect::<Vec<_>>(), vec!["bsp".to_string()]);
        assert_eq!(alpha.root, util::normalize(&root.join("alpha")).unwrap());
    }

    #[test]
    fn duplicate_names_point_to_the_wrong_directory_hint() {
        let root = temp_project("duphint");
        write(&root.join("a/config.yaml"), "name: same\n");
        write(&root.join("b/config.yaml"), "name: same\n");
        let msg = Project::discover(&root).unwrap_err().to_string();
        assert!(msg.contains("duplicate module name"), "{msg}");
        assert!(msg.contains("parent"), "the error should hint at the wrong directory: {msg}");
    }

    #[test]
    fn running_inside_a_module_marks_it_current() {
        let root = temp_project("current");
        write(&root.join("app/config.yaml"), "name: app\n");
        write(&root.join("lib/config.yaml"), "name: lib\n");
        let project = Project::discover(&root.join("app")).unwrap();
        assert_eq!(project.current.as_deref(), Some("app"));
        assert_eq!(project.modules.len(), 2);
    }
}
