# mdict-cli-rs

## Features
1. support stardict and mdict
2. anki mode

## Get start
1. put the mdict or stardict under `~/.local/share/mdict-cli-rs` 

    `mdict-cli-rs` will search dictionaries recursively

    mdict only support v1,v2

2. install [carbonyl](https://github.com/fathyb/carbonyl)
3. `cargo r -- awesome`

[![asciicast](https://asciinema.org/a/684675.svg)](https://asciinema.org/a/684675)

## Usage

```
# search word
mdict-cli-rs <word>

# anki-like review mode
# open http://127.0.0.1:3333 in browser
mdict-cli-rs anki

mdict-cli-rs --list-dicts
mdict-cli-rs --show-path
```

### blog in Chinese

https://rustcc.cn/article?id=f1875505-af4e-4043-ba92-f95a2e7e01a1
