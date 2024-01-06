use anyhow::{Context, Result};
use ego_tree::NodeRef;
use mdict::{KeyMaker, MDictBuilder};
use rayon::prelude::*;
use scraper::{Html, Node};
use std::sync::mpsc::channel;
use std::{
    collections::HashSet,
    env::{self},
    fs::{self, File},
    io::Write,
    path::PathBuf,
    process::Command,
};
use tempfile::tempdir;
use walkdir::WalkDir;

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

fn main() -> Result<()> {
    let word = env::args().nth(1).unwrap();

    let (sender, receiver) = channel();

    load_dict().into_par_iter().for_each_with(sender, |s, p| {
        let Ok(mut mdx) = MDictBuilder::new(&p).build_with_key_maker(MyKeyMaker) else {
            return;
        };
        if let Ok(Some(definition)) = mdx.lookup(&word) {
            s.send((p, definition.definition)).unwrap();
        }
    });

    let results: Vec<_> = receiver.iter().collect();

    let items: Vec<_> = results.iter().map(|(p, _)| p.to_str().unwrap()).collect();

    if items.is_empty() {
        eprintln!("not found");
        return Ok(());
    }

    if items.len() == 1 {
        println!("only found in {:?}", results[0].0);
        fun_name(&results[0])?;
    } else {
        loop {
            let selection = dialoguer::Select::new()
                .with_prompt("What do you choose?")
                .items(&items)
                .interact()
                .unwrap();

            fun_name(&results[selection])?;
        }
    }

    Ok(())
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

fn fun_name(selected: &(PathBuf, String)) -> Result<()> {
    let temp_dir = tempdir()?;
    let index_html = temp_dir.path().join("index.html");
    File::create(&index_html)?.write_all(selected.1.as_bytes())?;

    let mdd_path = selected.0.with_extension("mdd");
    if let Ok(mut mdd) = MDictBuilder::new(&mdd_path).build_with_key_maker(MyKeyMaker) {
        let mut resources: HashSet<String> = HashSet::new();
        let dom = Html::parse_document(&selected.1);
        dfs(dom.tree.root(), &mut resources);
        for resource in resources {
            let p = {
                let mut p = selected.0.clone();
                p.pop();
                p.push(&resource);
                p
            };
            if p.exists() {
                let s = fs::read_to_string(p)?;
                let dest = temp_dir.path().join(resource);
                File::create(&dest)
                    .with_context(|| format!("fail to create {:?}", dest))?
                    .write_all(s.as_bytes())?;
                continue;
            }

            let mut resource = resource;
            if !resource.starts_with('/') {
                resource = "/".to_owned() + &resource;
            }
            if let Ok(Some(x)) = mdd.lookup(&resource.replace('/', "\\")) {
                let dest = temp_dir.path().join(&resource[1..]);
                File::create(&dest)
                    .with_context(|| format!("fail to create {:?}", dest))?
                    .write_all(x.definition.as_bytes())?;
            } else {
                eprintln!("failed to load {resource}");
            }
        }
    } else {
        eprintln!("{:?} not exists", mdd_path);
    }
    Command::new("carbonyl").arg(index_html).status()?;
    Ok(())
}

fn load_dict() -> Vec<PathBuf> {
    let d = dirs::data_local_dir().unwrap().join("mdict-cli-rs");

    let mut v = Vec::new();

    for entry in WalkDir::new(d).follow_links(true) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() && entry.file_name().to_str().unwrap().ends_with(".mdx") {
            v.push(PathBuf::from(entry.path()));
        }
    }
    v
}
