use crate::utils::create_sub_dir;
use crate::{
    query,
    spaced_repetition::{self, SpacedRepetiton},
};
use anyhow::Result;
use axum::{
    response::sse::{Event, Sse},
    routing::get,
    Router,
};
use axum_extra::TypedHeader;
use futures::stream::{self, Stream};
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::thread;
use std::{convert::Infallible, path::PathBuf, time::Duration};
use tokio_stream::StreamExt as _;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn anki() -> Result<()> {
    let temp_dir = tempfile::Builder::new().prefix("review-").tempdir()?;
    let temp_dir_path = temp_dir.path().to_path_buf();

    let spaced_repetition = crate::fsrs::sqlite_history::SQLiteHistory::default();
    let Ok(Some(word)) = spaced_repetition.next_to_review() else {
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
    <title>固定顶部和底部</title>
    <style>
        body {{
            margin: 0;
            padding: 0;
            font-family: Arial, sans-serif;
        }}
        .header {{
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
            background-color: #4CAF50;
            color: white;
            display: flex;
            justify-content: center; /* 居中对齐 */
            padding: 10px;
        }}
        .footer button {{
            margin: 0 10px; /* 按钮之间的间距 */
            padding: 10px 15px;
            background-color: white;
            color: #4CAF50;
            border: none;
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
        .footer .easy {{
            background-color: #4CAF50;
            color: white;
        }}
        .footer .good {{
            background-color: #2196F3;
            color: white;
        }}
        .footer .hard {{
            background-color: #FF9800;
            color: white;
        }}
        .footer .again {{
            background-color: #F44336;
            color: white;
        }}
    </style>
</head>
<body>

    <div class="header">
        {word}
    </div>

    <div class="content">
          <iframe id="answer" src="about:blank" ></iframe>
    </div>

    <div class="footer">
        <button id="jiting" class="button" onclick="showAnswer()">Show Answer</button>
        <button class="hewen button easy"  onclick="rate('easy')"  style="display: none;">Easy</button>
        <button class="hewen button good"  onclick="rate('good')"  style="display: none;">Good</button>
        <button class="hewen button hard"  onclick="rate('hard')"  style="display: none;">Hard</button>
        <button class="hewen button again" onclick="rate('again')" style="display: none;">Again</button>
    </div>

    <script>
        function showAnswer() {{
            document.getElementById('answer').src= "{}/index.html";
            document.querySelectorAll('.hewen').forEach(x => x.style.display = '' );
            document.getElementById('jiting').style.display = 'none';
        }}

        function rate(option) {{
            alert("You rated the card as: " + option);
        }}
    </script>

</body>
</html> "#,
        p.file_name().unwrap().to_str().unwrap()
    );

    File::create(temp_dir_path.join("index.html"))?.write_all(html.as_bytes())?;

    let server_thread = thread::spawn(async || {
        let static_files_service =
            ServeDir::new(temp_dir_path).append_index_html_on_directories(true);
        let app = Router::new()
            .fallback_service(static_files_service)
            .route("/again", get(sse_handler))
            .route("/hard", get(sse_handler))
            .route("/good", get(sse_handler))
            .route("/easy", get(sse_handler))
            .layer(TraceLayer::new_for_http());

        // run it
        let listener = tokio::net::TcpListener::bind("127.0.0.1:3333")
            .await
            .unwrap();
        tracing::debug!("listening on {}", listener.local_addr().unwrap());
        axum::serve(listener, app).await.unwrap();
    });

    let _ = server_thread.join().unwrap();
    let _ = Command::new("carbonyl")
        .arg("http://127.0.0.1:3333")
        .status()?;
    loop {}
    Ok(())
}

async fn sse_handler(
    TypedHeader(user_agent): TypedHeader<headers::UserAgent>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    println!("`{}` connected", user_agent.as_str());

    // A `Stream` that repeats an event every second
    //
    // You can also create streams from tokio channels using the wrappers in
    // https://docs.rs/tokio-stream
    let stream = stream::repeat_with(|| Event::default().data("hi!"))
        .map(Ok)
        .throttle(Duration::from_secs(1));

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(1))
            .text("keep-alive-text"),
    )
}
