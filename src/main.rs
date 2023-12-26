use anyhow::{Context, Result};
use html_parser::Dom;
use mdict::Mdx;
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

fn main() -> Result<()> {
    let word = env::args().nth(1).unwrap();

    let mut dicts = load_dict()
        .into_iter()
        .filter_map(|dict| Mdx::from(&dict).map(|m| (dict, m)).ok())
        .collect::<Vec<_>>();

    let results: Vec<_> = dicts
        .iter_mut()
        .filter_map(|(p, ref mut mdx)| {
            if let Ok(Some(definition)) = mdx.lookup_as_string(&word) {
                Some((p.clone(), definition))
            } else {
                None
            }
        })
        .collect();

    let items: Vec<_> = results.iter().map(|(p, _)| p.to_str().unwrap()).collect();
    let selection = dialoguer::Select::new()
        .with_prompt("What do you choose?")
        .items(&items)
        .interact()
        .unwrap();

    let temp_dir = tempdir()?;
    let index_html = temp_dir.path().join("index.html");
    File::create(&index_html)?.write_all(results[selection].1.as_bytes())?;

    let mut resources: HashSet<&str> = HashSet::new();
    let dom = Dom::parse(&results[selection].1)?;
    for (k, v) in dom
        .children
        .iter()
        .filter_map(|node| node.element())
        .flat_map(|element| &element.attributes)
    {
        let Some(v) = v else { continue };
        if (k == "href" || k == "src") && (v.ends_with(".js") || v.ends_with(".css")) {
            resources.insert(v);
        }
    }

    let mdd_path = results[selection].0.with_extension("mdd");
    if let Ok(mut mdd) = Mdx::from(&mdd_path) {
        for resource in resources {
            let p = {
                let mut p = results[selection].0.clone();
                p.pop();
                p.push(resource);
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

            let mut resource = resource.to_owned();
            if !resource.starts_with('/') {
                resource = "/".to_owned() + &resource;
            }
            if let Ok(Some(x)) = mdd.lookup(&resource.replace('/', "\\")) {
                let dest = temp_dir.path().join(&resource[1..]);
                File::create(&dest)
                    .with_context(|| format!("fail to create {:?}", dest))?
                    .write_all(x.definition)?;
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

    for entry in WalkDir::new(d).max_depth(2) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() && entry.file_name().to_str().unwrap().ends_with(".mdx") {
            v.push(PathBuf::from(entry.path()));
        }
    }
    v
}
