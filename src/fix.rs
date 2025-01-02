use std::collections::HashMap;
use std::fs;
use std::path::{self, Path, PathBuf};
use toml_edit::{value, DocumentMut, Value};
use walkdir::{DirEntry, WalkDir};

/// すべてのパートに *.lib の形式が一箇所しか表れず、それが最後になっているディレクトリ
/// "a.lib/b.lib" などは除外される
fn is_lib_dir(e: &DirEntry) -> bool {
    if !e.file_type().is_dir() {
        return false;
    }
    let cs = e
        .path()
        .components()
        .map(|c| {
            if let path::Component::Normal(s) = c {
                if let Some(s) = s.to_str() {
                    s.ends_with(".lib")
                } else {
                    false
                }
            } else {
                false
            }
        })
        .collect::<Vec<_>>();
    cs[..cs.len() - 1].iter().all(|&b| !b) && cs[cs.len() - 1]
}

/// /foo.lib -> foo
/// /foo/core.lib -> foo ("core" would be trimmed)
/// /foo/util.lib -> foo-util
/// /foo/util/bar.lib -> foo-util-bar
fn lib_rel_path_to_lib_name(lib_rel_path: &Path) -> String {
    let mut cs = lib_rel_path
        .components()
        .map(|c| {
            if let path::Component::Normal(s) = c {
                s.to_str().unwrap()
            } else {
                panic!("unexpected path component");
            }
        })
        .collect::<Vec<_>>();
    let last = cs.pop().unwrap().trim_end_matches(".lib");
    cs.push(last);
    if last == "core" {
        cs.pop().unwrap();
    }
    cs.join("-")
}

#[derive(Debug, Clone)]
struct TomlFile {
    text: String,
    doc: DocumentMut,
}
#[derive(Debug, Clone)]
struct LibAnalysis {
    lib_rel_path: PathBuf,
    lib_name: String,
    cargo_toml_access_path: PathBuf,
    cargo_toml: Option<TomlFile>,
}
fn analyze_lib(crates_dir: &path::Path, lib_rel_path: &path::Path) -> LibAnalysis {
    let cargo_toml_access_path = crates_dir.join(lib_rel_path).join("Cargo.toml");
    let cargo_toml_text = fs::read_to_string(&cargo_toml_access_path);
    let cargo_toml = cargo_toml_text
        .map(|s| {
            s.parse::<DocumentMut>()
                .ok()
                .map(|d| TomlFile { text: s, doc: d })
        })
        .unwrap_or(None);
    let lib_name = lib_rel_path_to_lib_name(lib_rel_path);
    LibAnalysis {
        lib_rel_path: lib_rel_path.to_owned(),
        lib_name,
        cargo_toml_access_path,
        cargo_toml,
    }
}

#[derive(Debug, Clone)]
struct LibAnalysisBundle {
    libs_map: HashMap<String, LibAnalysis>,
}
impl LibAnalysisBundle {
    fn new(libs: Vec<LibAnalysis>) -> Self {
        let libs_map = libs
            .iter()
            .map(|l| (l.lib_name.clone(), l.clone()))
            .collect();
        Self { libs_map }
    }
    fn get(&self, lib_name: &str) -> Option<&LibAnalysis> {
        self.libs_map.get(lib_name)
    }
}

fn common_prefix(a: impl AsRef<path::Path>, b: impl AsRef<path::Path>) -> PathBuf {
    let a = a.as_ref();
    let b = b.as_ref();
    let mut common = PathBuf::new();
    for (a, b) in a.components().zip(b.components()) {
        if a == b {
            common.push(a);
        } else {
            break;
        }
    }
    common
}

fn relative_path(from: impl AsRef<path::Path>, to: impl AsRef<path::Path>) -> PathBuf {
    let from = from.as_ref();
    let to = to.as_ref();
    let common = common_prefix(from, to);
    let pop_count = from.components().count() - common.components().count();
    let mut rel = PathBuf::new();
    for _ in 0..pop_count {
        rel.push("..");
    }
    for c in to.components().skip(common.components().count()) {
        rel.push(c);
    }
    rel
}

fn check_libs(libs: &[LibAnalysis]) -> Result<(), Vec<String>> {
    let mut lib_map = HashMap::new();
    for lib in libs.iter() {
        lib_map.entry(&lib.lib_name).or_insert(vec![]).push(lib);
    }
    let mut errors = vec![];
    for (lib_name, libs) in lib_map.iter() {
        if libs.len() > 1 {
            errors.push(format!("multiple libs with the same name: {}", lib_name));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn fix_lib(lib: &mut LibAnalysis, libs: &LibAnalysisBundle) {
    if let Some(TomlFile { doc, .. }) = &mut lib.cargo_toml {
        // Cargo.tomlのpackage.nameの修正をする
        if let Some(package) = doc.as_table_mut().get_mut("package") {
            package["name"] = value(&lib.lib_name);
        }
        // Cargo.tomlのdependencies指定のパスの修正をする
        // commutative-ring = { path = "" }  ---> commutative-ring = { path = "../commutative-ring.lib" }
        if let Some(deps) = doc
            .as_table_mut()
            .get_mut("dependencies")
            .and_then(|v| v.as_table_like_mut())
        {
            for (dep_name, dep) in deps.iter_mut() {
                if let Some(dep) = dep.as_inline_table_mut() {
                    if let Some(path) = dep.get_mut("path") {
                        *path = Value::from(
                            libs.get(&dep_name.to_string())
                                .map(|dep_lib| {
                                    relative_path(&lib.lib_rel_path, &dep_lib.lib_rel_path)
                                        .to_str()
                                        .unwrap()
                                        .to_owned()
                                })
                                .unwrap_or_else(|| "NOT_FOUND".to_owned()),
                        );
                    }
                }
            }
            // dependenciesを辞書順に並びかえる
            deps.sort_values();
        }
    }
}

fn write_lib(lib: &LibAnalysis) -> bool {
    if let Some(TomlFile { text, doc }) = &lib.cargo_toml {
        let text_new = doc.to_string();
        if *text != text_new {
            fs::write(&lib.cargo_toml_access_path, text_new).unwrap();
            return true;
        }
    }
    false
}

pub fn fix(root_dir: impl AsRef<Path>) -> Result<(), Vec<String>> {
    let crates_dir = root_dir.as_ref().join("crates");
    let libs = WalkDir::new(&crates_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(is_lib_dir)
        .map(|e| e.into_path())
        .map(|e| e.strip_prefix(&crates_dir).unwrap().to_owned())
        .collect::<Vec<_>>();

    let mut libs = libs
        .iter()
        .map(|p| analyze_lib(&crates_dir, p))
        .collect::<Vec<_>>();
    check_libs(&libs)?;
    let libs_original = LibAnalysisBundle::new(libs.clone());

    for lib in libs.iter_mut() {
        fix_lib(lib, &libs_original);
        write_lib(lib);
    }

    Ok(())
}
