use anyhow::{Context, Result};
use ego_tree::NodeRef;
use mdict::{KeyMaker, MDictBuilder};
use rayon::prelude::*;
use scraper::{Html, Node};
use std::path::Path;
use std::sync::mpsc::channel;
use std::{
    collections::HashSet,
    env::{self},
    fs::{self, File},
    io::Write,
    path::PathBuf,
    process::Command,
};
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
    let temp_dir = tempfile::Builder::new().prefix(&word).tempdir()?;

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

    if results.is_empty() {
        eprintln!("not found");
        return Ok(());
    }

    for x in &results {
        fun_name(temp_dir.path(), x)?;
    }

    let buttons: String = results
        .into_iter()
        .map(|(p, _)| {
            let display_name = p.file_name().unwrap().to_str().unwrap();
            let name = groom_name(display_name);
            format!(
                r#"<button onclick="changeIframeSrc('{name}/index.html', this)">{display_name}</button>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

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
   {buttons}
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

fn fun_name(base_path: &Path, selected: &(PathBuf, String)) -> Result<()> {
    let base_path = base_path.join(groom_name(
        selected.0.file_name().unwrap().to_str().unwrap(),
    ));
    fs::create_dir(&base_path).context(format!("fail to create_dir {:?}", base_path))?;

    let index_html = base_path.join("index.html");
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
                let dest = base_path.join(resource);
                fs::copy(p, dest)?;
                continue;
            }

            let mut resource = resource;
            if !resource.starts_with('/') {
                resource = "/".to_owned() + &resource;
            }
            if let Ok(Some(x)) = mdd.lookup(&resource.replace('/', "\\")) {
                let dest = base_path.join(&resource[1..]);
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

fn groom_name(folder_name: &str) -> String {
    // remove ' in folder_name
    folder_name.replace(|c| c == '\'', "")
}
