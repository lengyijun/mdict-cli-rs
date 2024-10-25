use anyhow::Context;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;

pub fn sort_str(s: &str) -> String {
    let mut chars: Vec<char> = s.chars().collect();

    // Sort the vector of characters
    chars.sort();

    // Collect the sorted characters back into a string
    chars.into_iter().collect()
}

pub fn groom_name(folder_name: &str) -> String {
    // remove ' in folder_name
    folder_name.replace(|c| c == '\'', "")
}

pub fn create_sub_dir(base_dir: &Path, prefer_name: &str) -> Result<PathBuf> {
    let p = create_sub_dir_inner(base_dir, prefer_name);
    std::fs::create_dir(&p).context(format!("fail to create_dir {:?}", base_dir))?;
    Ok(p)
}

fn create_sub_dir_inner(base_dir: &Path, prefer_name: &str) -> PathBuf {
    let p = base_dir.join(prefer_name);
    if !p.exists() {
        return p;
    }
    for i in 1.. {
        let p = base_dir.join(format!("{prefer_name}-{i}"));
        if !p.exists() {
            return base_dir.join(prefer_name);
        }
    }
    unreachable!()
}
