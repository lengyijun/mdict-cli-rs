use crate::utils::create_sub_dir;
use crate::utils::groom_name;
use crate::T;
use anyhow::anyhow;
use anyhow::{Context, Result};
use ego_tree::NodeRef;
use log::*;
use mdict::{KeyMaker, MDictBuilder};
use scraper::{Html, Node};
use std::path::Path;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

struct MyKeyMaker;

impl KeyMaker for MyKeyMaker {
    fn make(&self, key: &std::borrow::Cow<str>, _resource: bool) -> String {
        fn strip_punctuation(w: &str) -> String {
            w.to_lowercase()
                .chars()
                .filter(|c| !c.is_ascii_punctuation() && !c.is_whitespace())
                .collect()
        }
        strip_punctuation(key)
    }
}

pub struct Mdict {
    pub mdx_path: PathBuf,
}

impl T for Mdict {
    fn name(&self) -> &str {
        self.mdx_path.file_name().unwrap().to_str().unwrap()
    }

    fn path(&self) -> &Path {
        &self.mdx_path
    }

    fn lookup(&self, word: &str, base_dir: &Path) -> Result<PathBuf> {
        let mut mdx = MDictBuilder::new(&self.mdx_path).build_with_key_maker(MyKeyMaker)?;
        let definition = mdx.lookup(word)?;
        let Some(definition) = definition else {
            return Result::Err(anyhow!("not found"));
        };

        let base_dir = create_sub_dir(
            base_dir,
            &groom_name(self.mdx_path.file_name().unwrap().to_str().unwrap()),
        )?;

        let index_html = base_dir.join("index.html");
        File::create(&index_html)?.write_all(definition.definition.as_bytes())?;

        let mdd_path = self.mdx_path.with_extension("mdd");
        let mut mdd = MDictBuilder::new(&mdd_path).build_with_key_maker(MyKeyMaker);

        let mut resources: HashSet<String> = HashSet::new();
        let dom = Html::parse_document(&definition.definition);
        dfs(dom.tree.root(), &mut resources);
        for resource in resources {
            let p = {
                let mut p = self.mdx_path.clone();
                p.pop();
                p.push(&resource);
                p
            };
            if p.exists() {
                let dest = base_dir.join(resource);
                fs::copy(p, dest)?;
                continue;
            }

            let mut resource = resource;
            if !resource.starts_with('/') {
                resource = "/".to_owned() + &resource;
            }
            let word = &resource.replace('/', "\\");
            match mdd.as_mut().map(|mdd| mdd.lookup(word)) {
                Ok(Ok(Some(x))) => {
                    let dest = base_dir.join(&resource[1..]);
                    File::create(&dest)
                        .with_context(|| format!("fail to create {:?}", dest))?
                        .write_all(x.definition.as_bytes())?;
                }
                Ok(Ok(None)) => {
                    error!("{} failed to load {resource}", self.name());
                }
                Ok(Err(e)) => {
                    error!("{} failed to load {resource} {e}", self.name());
                }
                Err(e) => {
                    error!("{:?} not exist; {e}", mdd_path);
                }
            }
        }

        Ok(base_dir)
    }
}

fn dfs(root: NodeRef<Node>, hm: &mut HashSet<String>) {
    if let Node::Element(e) = root.value() {
        for (_, v) in e.attrs() {
            if v.ends_with(".css") || v.ends_with(".js") {
                hm.insert(v.to_owned());
            }
        }
    }
    for x in root.children() {
        dfs(x, hm);
    }
}
