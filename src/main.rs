#![feature(async_closure)]

use crate::fsrs::sqlite_history::add_history;
use crate::mdict_wrapper::Mdict;
use crate::stardict::StarDict;
use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use chrono::prelude::*;
use env_logger::Target;
use log::*;
use rayon::prelude::*;
use shadow_rs::shadow;
use std::ffi::OsStr;
use std::fs::create_dir;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::mpsc::channel;
use std::{
    env::{self},
    fs::File,
    io::Write,
    path::PathBuf,
    process::Command,
};
use walkdir::WalkDir;

mod anki;
mod fsrs;
mod mdict_wrapper;
mod spaced_repetition;
mod stardict;
mod utils;

shadow!(build);

#[tokio::main]
async fn main() -> Result<()> {
    let word = env::args().nth(1).unwrap();
    match &*word {
        "--help" => {
            println!("https://github.com/lengyijun/mdict-cli-rs/");
            Ok(())
        }
        "--version" => {
            println!("{}", build::VERSION); //print version const
            Ok(())
        }
        "--list-dict" | "--list-dicts" => {
            let v = load_dict();
            if v.is_empty() {
                println!("no dictionary found in {:?}", dictionary_dir());
                return Ok(());
            }
            for dict in v {
                println!("{:?}", dict.path());
            }
            Ok(())
        }
        "--show-path" => {
            println!("dictionary dir            {:?}", dictionary_dir());
            println!("history database          {:?}", db_path());
            println!("log dir                   {:?}", log_dir());
            Ok(())
        }
        "anki" => {
            let local: DateTime<Local> = Local::now();
            let log_path = log_dir().join(format!("log.{}", local.to_rfc3339()));

            let log_file = Box::new(
                OpenOptions::new()
                    .read(true)
                    .create(true)
                    .append(true)
                    .open(&log_path)?,
            );
            println!("log file: {:?}", log_path);

            env_logger::Builder::from_default_env()
                .target(Target::Pipe(log_file))
                .filter_level(log::LevelFilter::Info) // Set the minimum log level
                .init();
            anki::anki().await?;
            Ok(())
        }
        _ => {
            env_logger::Builder::from_default_env()
                .filter_level(log::LevelFilter::Info) // Set the minimum log level
                .init();

            let temp_dir = tempfile::Builder::new().prefix(&word).tempdir()?;
            let index_html = query(&word, temp_dir.path())?;
            add_history(&word).await?;
            let _ = Command::new("carbonyl").arg(index_html).status()?;
            Ok(())
        }
    }
}

fn query(word: &str, base_dir: &Path) -> Result<PathBuf> {
    info!("{word}");
    let (sender, receiver) = channel();

    load_dict()
        .into_par_iter()
        .for_each_with(sender, |s, dict| {
            if let Ok(p) = dict.lookup(word, base_dir) {
                s.send(format!(
                    r#"<button onclick="changeIframeSrc('{}/index.html', this)">{}</button>"#,
                    p.file_name().unwrap().to_str().unwrap(),
                    dict.name(),
                ))
                .unwrap();
            }
        });

    let buttons: Vec<_> = receiver.iter().collect();

    if buttons.is_empty() {
        error!("{word} not found");
        return Err(anyhow!("{word} not found"));
    }

    let buttons_str = buttons.join("\n");

    let index_html = base_dir.join("index.html");
    let html = format!(
        r#"
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Scrollable Button List with iframe and Active Button</title>
  <style>
    /* 页面布局 */
    body {{
      display: flex;
      height: 100vh;
      margin: 0;
    }}

    /* 左边的按钮列 */
    .sidebar {{
      width: 200px;
      background-color: #f4f4f4;
      padding: 20px;
      box-shadow: 2px 0 5px rgba(0, 0, 0, 0.1);
      overflow-y: auto; /* 启用垂直滚动条 */
      max-height: 100vh; /* 高度固定为视口高度 */
    }}

    /* 按钮样式 */
    .sidebar button {{
      display: block;
      width: 100%;
      padding: 10px;
      margin-bottom: 10px;
      font-size: 16px;
      cursor: pointer;
      border: none;
      background-color: #007BFF;
      color: white;
      border-radius: 5px;
    }}

    .sidebar button:hover {{
      background-color: #0056b3;
    }}

    /* 点击后激活状态的按钮样式 */
    .sidebar button.active {{
      background-color: #28a745;
    }}

    .sidebar button.visited {{
      background-color: gray;
    }}

    /* 右边的 iframe */
    .content {{
      flex-grow: 1;
      padding: 20px;
    }}

    iframe {{
      width: 100%;
      height: 100%;
      border: none;
    }}
  </style>
</head>
<body>

  <!-- 左边的可滚动按钮列 -->
  <div class="sidebar">
   {buttons_str}
  </div>

  <!-- 右边显示 iframe -->
  <div class="content">
    <iframe id="myIframe" src="page1.html"></iframe>
  </div>

  <script>
    // 更改 iframe 的 src 并改变按钮颜色
    function changeIframeSrc(newSrc, clickedButton) {{
      document.getElementById('myIframe').src = newSrc;

      // 获取所有按钮
      var buttons = document.querySelectorAll('.sidebar button');

      // 移除所有按钮的 active 类
      buttons.forEach(function(button) {{
        if(button.classList.contains('active')){{
          button.classList.remove('active');
          button.classList.add('visited');
    }}
    }});

      // 为点击的按钮添加 active 类
      clickedButton.classList.remove('visited');
      clickedButton.classList.add('active');
    }}
  </script>

</body>
</html>
"#
    );
    File::create(&index_html)?.write_all(html.as_bytes())?;

    Ok(index_html)
}

fn dictionary_dir() -> PathBuf {
    let path = dirs::data_local_dir().unwrap().join("mdict-cli-rs");
    if !path.exists() {
        create_dir(&path)
            .with_context(|| format!("Failed to create directory {:?}", path))
            .unwrap();
    }
    path
}

fn db_path() -> PathBuf {
    dictionary_dir().join("history.db")
}

fn log_dir() -> PathBuf {
    let path = dirs::cache_dir().unwrap().join("mdict-cli-rs");
    if !path.exists() {
        create_dir(&path)
            .with_context(|| format!("Failed to create directory {:?}", path))
            .unwrap();
    }
    path
}

// load mdict or stardict
fn load_dict() -> Vec<Box<dyn T>> {
    let d = dictionary_dir();

    let mut v: Vec<Box<dyn T>> = Vec::new();

    for entry in WalkDir::new(d).follow_links(true) {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_dir() {
            continue;
        }
        if let Some(extension) = entry.path().extension().and_then(OsStr::to_str) {
            match extension {
                "mdx" => {
                    v.push(Box::new(Mdict {
                        mdx_path: entry.path().to_path_buf(),
                    }));
                }
                "dz" => {
                    if let Ok(stardict) = StarDict::dz(entry.path()) {
                        v.push(Box::new(stardict));
                    }
                }
                "dict" => {
                    if let Ok(stardict) = StarDict::dict(entry.path()) {
                        v.push(Box::new(stardict));
                    }
                }
                _ => {}
            }
        }
    }
    v
}

trait T: Send {
    /// display on button
    fn name(&self) -> &str;

    /// path to dict
    fn path(&self) -> &Path;

    /// write the result in @return/index.html
    fn lookup(&self, word: &str, base_dir: &Path) -> Result<PathBuf>;
}
