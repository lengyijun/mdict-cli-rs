use crate::fsrs::sqlite_history::SQLiteHistory;
use crate::utils::create_sub_dir;
use crate::utils::rating_from_u8;
use crate::{query, spaced_repetition::SpacedRepetiton};
use anyhow::Result;
use axum::extract::State;
use axum::routing::post;
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::thread;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

pub async fn anki() -> Result<()> {
    let temp_dir = tempfile::Builder::new().prefix("review-").tempdir()?;
    let temp_dir_path = temp_dir.path().to_path_buf();

    let spaced_repetition = SQLiteHistory::default().await;
    let Ok(word) = spaced_repetition.next_to_review().await else {
        println!("no word to review");
        return Ok(());
    };

    let p = create_sub_dir(&temp_dir_path, &word)?;

    let _ = query(&word, &p)?;

    let html = format!(
        r#"
    <!DOCTYPE html>
<html lang="zh">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Review</title>
    <style>
        body {{
            margin: 0;
            padding: 0;
            font-family: Arial, sans-serif;
        }}
        #header {{
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            background-color: #4CAF50;
            color: white;
            text-align: center;
            padding: 10px;
        }}
        .footer {{
            position: fixed;
            bottom: 0;
            left: 0;
            right: 0;
            color: white;
            display: flex;
            justify-content: center; /* 居中对齐 */
            padding: 10px;
        }}
        .footer button {{
            margin: 0 10px; /* 按钮之间的间距 */
            padding: 10px 15px;
            border-radius: 5px;
            cursor: pointer;
            transition: background-color 0.3s;
        }}
        .footer button:hover {{
            background-color: #ddd; /* 悬停效果 */
        }}
        .content {{
            margin-top: 50px; /* 给内容留出顶部空间 */
            margin-bottom: 50px; /* 给内容留出底部空间 */
            padding: 20px;
            height: calc(100vh - 100px); /* 计算内容区域的高度 */
            overflow: auto; /* 如果内容超出，则添加滚动条 */
        }}
        #answer {{
            width: 100%;
            height: 100%;
            border: none;
        }}
        .button {{
            margin-top: 20px;
            padding: 10px 20px;
            font-size: 16px;
            border: none;
            border-radius: 5px;
            cursor: pointer;
        }}
        #easy {{
            background-color: #4CAF50;
            color: white;
        }}
        #good {{
            background-color: #2196F3;
            color: white;
        }}
        #hard {{
            background-color: #FF9800;
            color: white;
        }}
        #again {{
            background-color: #F44336;
            color: white;
        }}
    </style>
</head>
<body>

    <div id="header">
        {word}
    </div>

    <div class="content">
          <iframe id="answer" src="about:blank" ></iframe>
    </div>

    <div class="footer">
        <button id="showanswer" class="button" onclick="showAnswer()">Show Answer</button>
        <button id="again"      class="button" onclick="rate(1)" style="display: none;">Again</button>
        <button id="hard"       class="button" onclick="rate(2)" style="display: none;">Hard</button>
        <button id="good"       class="button" onclick="rate(3)" style="display: none;">Good</button>
        <button id="easy"       class="button" onclick="rate(4)" style="display: none;">Easy</button>
    </div>

    <script>
        var word = "{word}";
        var path = "{}";

        function showAnswer() {{
            document.getElementById('answer').src= path + "/index.html";
            document.getElementById('easy').style.display = '';
            document.getElementById('good').style.display = '';
            document.getElementById('hard').style.display = '';
            document.getElementById('again').style.display = '';
            document.getElementById('showanswer').style.display = 'none';
        }}

        function rate(rating) {{
            /* hide buttons */
            /* avoid double click */
            document.getElementById('easy').style.display = 'none';
            document.getElementById('good').style.display = 'none';
            document.getElementById('hard').style.display = 'none';
            document.getElementById('again').style.display = 'none';

            fetch("/ppppp", {{
                method: 'POST',
                headers: {{
                    'Content-Type': 'application/json'
                }},
               body: JSON.stringify({{
                    word : word,
                    rating : rating
               }}) 
            }})
            .then(response => response.json())
            .then(data => {{
                if(data.finished) {{
                    console.log("Congratulation! All cards reviewed");
                    alert("Congratulation! All cards reviewed")
                }}else{{
                    document.getElementById("header").textContent = data.word;
                    document.getElementById('answer').src = "about:blank";
                    word = data.word;
                    path = data.p;

                    document.getElementById('easy').style.display = 'none';
                    document.getElementById('good').style.display = 'none';
                    document.getElementById('hard').style.display = 'none';
                    document.getElementById('again').style.display = 'none';
                    document.getElementById('showanswer').style.display = '';
                }}
            }})
            .catch((error) => {{
                console.error('Error:', error);
            }});
        }}
    </script>

</body>
</html> "#,
        p.file_name().unwrap().to_str().unwrap()
    );

    File::create(temp_dir_path.join("index.html"))?.write_all(html.as_bytes())?;

    /*
    let server_thread = thread::spawn(|| {
        let _ = Command::new("carbonyl")
            .arg("http://127.0.0.1:3333")
            .status()
            .unwrap();
    });
     */

    // async fn handler(Path(params): Path<Params>) -> impl IntoResponse {
    let handler = async move |State(spaced_repetition): State<SQLiteHistory>,
                              Json(params): Json<Params>|
                -> Json<Value> {
        let rating = rating_from_u8(params.rating);
        println!("{} {:?}", params.word, rating);
        spaced_repetition
            .update(&params.word, rating)
            .await
            .unwrap();
        match spaced_repetition.next_to_review().await {
            Ok(word) => {
                let p = create_sub_dir(&temp_dir_path, &word).unwrap();
                match query(&word, &p) {
                    Ok(_) => {
                        let filename = p.file_name().unwrap().to_str().unwrap().to_owned();
                        println!("{word}");
                        Json(json!({ "word": word, "p" : filename }))
                    }
                    Err(_) => Json(json!({ "finished": true })),
                }
            }
            _ => Json(json!({ "finished": true })),
        }
    };

    let static_files_service =
        ServeDir::new(temp_dir.path()).append_index_html_on_directories(true);
    let app = Router::new()
        .fallback_service(static_files_service)
        .route("/ppppp", post(handler))
        .with_state(spaced_repetition)
        .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3333")
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    println!("open http://127.0.0.1:3333");
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct Params {
    word: String,
    rating: u8,
}
