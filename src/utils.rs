use anyhow::Context;
use anyhow::Result;
use fsrs::Rating;
use std::path::Path;
use std::path::PathBuf;

pub fn rating_from_u8(q: u8) -> Rating {
    match q {
        1 => fsrs::Rating::Again,
        2 => fsrs::Rating::Hard,
        3 => fsrs::Rating::Good,
        4 => fsrs::Rating::Easy,
        _ => unreachable!(),
    }
}

pub fn groom_name(folder_name: &str) -> String {
    // remove ' in folder_name
    folder_name.replace(|c| c == '\'', "")
}

pub fn create_sub_dir(base_dir: &Path, prefer_name: &str) -> Result<PathBuf> {
    let p = create_sub_dir_inner(base_dir, prefer_name);
    std::fs::create_dir(&p).context(format!("fail to create_dir {:?}", p))?;
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
