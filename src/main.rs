use std::{env, io::Write, process::Command};

use anyhow::Result;
use mdict::Mdx;

const MDX_V2: &str =
    "/home/lyj/.cache/github/Rita推荐！/2.牛津高阶8双解(推荐)/牛津高阶英汉双解词典(第8版).mdx";

fn main() -> Result<()> {
    let word = env::args().nth(1).unwrap();

    let mut mdx = Mdx::from(MDX_V2)?;
    let definition = mdx.lookup(&word)?;
    let Some(definition) = definition else {
        panic!("not found")
    };
    let mut f = tempfile::Builder::new().suffix(".html").tempfile()?;
    f.write_all(definition.definition)?;
    Command::new("carbonyl").arg(f.path()).status()?;

    Ok(())
}
