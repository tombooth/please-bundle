use std::{collections::HashMap, path::PathBuf};
use std::io::Read;
use std::fs::File;
use std::str::FromStr;

use anyhow::{Error, anyhow, bail};

use glob::glob;

use serde::Deserialize;

use swc_bundler::{Bundler, Load, Resolve, ModuleData};
use swc_common::{
    errors::{ColorConfig, Handler},
    sync::Lrc, 
    Globals, SourceMap, FilePathMapping, FileName,
};

use swc_ecma_ast::{EsVersion};
use swc_ecma_codegen::{
    text_writer::{JsWriter, WriteJs},
    Emitter,
};
use swc_ecma_parser::{parse_file_as_module, EsConfig, Syntax};



#[derive(Deserialize)]
struct PackageJson {
    #[serde(default)]
    name: Option<String>,

    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    browser: Option<String>,
    #[serde(default)]
    module: Option<String>,
}

/*#[derive(Deserialize)]
#[serde(untagged)]
enum Browser {
    Str(String),
    Obj(HashMap<String, StringOrBool>),
}*/

#[derive(Deserialize, Clone)]
#[serde(untagged)]
enum StringOrBool {
    Str(String),
    Bool(bool),
}




fn main() -> Result<(), Error> {

    // this should be loaded from a scan of package.json files inside of the
    // workspace
    let mut packages: HashMap<String, FileName> = HashMap::new();

    for entry in glob("./example/**/package.json").expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                let mut file = File::open(&path)?;
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;

                let package_json: PackageJson = serde_json::from_str(&contents)?;
                let package_dir = match path.parent() {
                    None => bail!("no package directory? {path:?}"),
                    Some(dir) => dir,
                };

                let name = match package_json.name {
                    None => bail!("no name for js package at {path:?}"),
                    Some(name) => name,
                };

                let entrypoints = [
                    package_json.browser.as_ref(),
                    package_json.module.as_ref(),
                    package_json.main.as_ref(),
                ];

                if let Some(Some(entrypoint)) = entrypoints.iter().find(|x| x.is_some()) {
                    let full_entrypoint = package_dir.join(entrypoint).canonicalize()?;
                    packages.insert(name, FileName::Real(full_entrypoint));
                } else {
                    bail!("you what mate");
                }
            },
            Err(e) => println!("{:?}", e),
        }
    }

    let globals = Globals::default();
    let cm = Lrc::new(SourceMap::new(FilePathMapping::empty()));
    let mut bundler = Bundler::new(
        &globals,
        cm.clone(),
        Loader { cm: cm.clone() },
        Resolver { packages: packages },
        swc_bundler::Config {
            require: false,
            disable_inliner: true, // !inline,
            external_modules: Default::default(),
            disable_fixer: false, // minify,
            disable_hygiene: false, // minify,
            disable_dce: false,
            module: Default::default(),
        },
        Box::new(Hook{}),
    );

    let mut entries: HashMap<String, FileName> = HashMap::new();
    entries.insert("main.js".to_string(), FileName::Real(PathBuf::from("./example/src/main.js")));

    let modules = match bundler.bundle(entries) {
        Err(why) => panic!("failed to bundle: {why:?}"),
        Ok(modules) => modules,
    };

    assert!(modules.len() == 1, "we only expect one module to exist not: {}", modules.len());

    let code = {
        let mut buf = vec![];

        {
            let wr = JsWriter::new(cm.clone(), "\n", &mut buf, None);
            let mut emitter = Emitter {
                cfg: swc_ecma_codegen::Config {
                    minify: false,
                    ..Default::default()
                },
                cm: cm.clone(),
                comments: None,
                wr: Box::new(wr) as Box<dyn WriteJs>,
            };

            emitter.emit_module(&modules[0].module).unwrap();
        }

        String::from_utf8_lossy(&buf).to_string()
    };

    println!("{}", code);

    Ok(())
}




pub struct Loader {
    pub cm: Lrc<SourceMap>,
}

impl Load for Loader {
    fn load(&self, f: &FileName) -> Result<ModuleData, Error> {
        let fm = match f {
            FileName::Real(path) => self.cm.load_file(path)?,
            _ => unreachable!(),
        };

        let module = parse_file_as_module(
            &fm,
            Syntax::Es(EsConfig {
                ..Default::default()
            }),
            EsVersion::Es2020,
            None,
            &mut vec![],
        )
        .unwrap_or_else(|err| {
            let handler =
                Handler::with_tty_emitter(ColorConfig::Always, false, false, Some(self.cm.clone()));
            err.into_diagnostic(&handler).emit();
            panic!("failed to parse")
        });

        Ok(ModuleData {
            fm,
            module,
            helpers: Default::default(),
        })
    }
}


pub struct Resolver {
    pub packages: HashMap<String, FileName>
}

impl Resolve for Resolver {
    fn resolve(&self, base: &swc_common::FileName, module_specifier: &str) -> Result<swc_common::FileName, Error> {
        if self.packages.contains_key(module_specifier) {
            return Ok(self.packages[module_specifier].clone());
        }

        if ! base.is_real() {
            return Err(anyhow!("base {base} isn't a real file, don't know what to do."));
        }

        // see if this is a path
        let path: std::path::PathBuf = std::path::PathBuf::from_str(module_specifier)?;
        
        if path.is_relative() {
            let base_path = match base {
                FileName::Real(path) => path,
                _ => bail!("base {base} isn't a real file, don't know what to do"),
            };

            let base_dir_path = match base_path.parent() {
                None => bail!("base '{base}' doesn't have a parent!"),
                Some(path) => path,
            };

            let full_path = base_dir_path.join(path).canonicalize()?;

            return Ok(FileName::Real(full_path));
        } else {
            return Ok(
                FileName::Real(path),
            );
        }
    }
}



struct Hook;

impl swc_bundler::Hook for Hook {
    fn get_import_meta_props(
            &self,
            _: swc_common::Span,
            _: &swc_bundler::ModuleRecord,
        ) -> Result<Vec<swc_ecma_ast::KeyValueProp>, Error> {
        panic!("unimpl hook");
    }
}