use crate::mdict_wrapper::Mdict;
use anyhow::Result;
use rayon::prelude::*;
use stardict::StarDict;
use std::ffi::OsStr;
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

mod mdict_wrapper;
mod stardict;

fn main() -> Result<()> {
    let word = env::args().nth(1).unwrap();
    let temp_dir = tempfile::Builder::new().prefix(&word).tempdir()?;

    let (sender, receiver) = channel();

    load_dict()
        .into_par_iter()
        .for_each_with(sender, |s, dict| {
            if let Ok(p) = dict.lookup(&word, temp_dir.path()) {
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
        eprintln!("not found");
        return Ok(());
    }

    let buttons_str = buttons.join("\n");

    let index_html = temp_dir.path().join("index.html");
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

    let _ = Command::new("carbonyl").arg(index_html).status()?;
    Ok(())
}

// load mdict or stardict
fn load_dict() -> Vec<Box<dyn T>> {
    let d = dirs::data_local_dir().unwrap().join("mdict-cli-rs");

    let mut v: Vec<Box<dyn T>> = Vec::new();

    for entry in WalkDir::new(d).follow_links(true) {
        let Ok(entry) = entry else { continue };
        if entry.file_type().is_dir() {
            continue;
        }
        if let Some(extension) = entry.path().extension().map(OsStr::to_str).flatten() {
            match extension {
                "mdx" => {
                    v.push(Box::new(Mdict {
                        mdx_path: entry.path().to_path_buf(),
                    }));
                }
                "idx" => {
                    if let Ok(stardict) = StarDict::idx(entry.path()) {
                        v.push(Box::new(stardict));
                    }
                }
                _ => {}
            }
        }
    }
    v
}

fn groom_name(folder_name: &str) -> String {
    // remove ' in folder_name
    folder_name.replace(|c| c == '\'', "")
}

fn create_sub_dir(base_dir: &Path, prefer_name: String) -> PathBuf {
    let p = base_dir.join(&prefer_name);
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

trait T: Send {
    /// display on button
    fn name(&self) -> &str;

    fn lookup(&self, word: &str, base_dir: &Path) -> Result<PathBuf>;
}
