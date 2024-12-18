//! copy and modified from <https://github.com/vaaandark/dioxionary/blob/master/src/stardict.rs>
//! Look up words form the offline stardicts.
use crate::utils::create_sub_dir;
use crate::utils::groom_name;
use crate::T;
use anyhow::{anyhow, Context, Result};
use eio::FromBytes;
use flate2::read::GzDecoder;
use std::cell::OnceCell;
use std::error::Error;
use std::fmt::{Debug, Display};
use std::fs::{self, read, File};
use std::io::{prelude::*, BufReader};
use std::path::{Path, PathBuf};

/// The stardict to be looked up.
pub struct StarDict {
    ifo: Ifo,
    idx: Idx,
    dict: Dict,
}

/// A word entry of the stardict.
pub struct Entry {
    pub word: String,
}

// only used in fuzzy search selection
pub struct EntryWrapper<'b> {
    pub dict_name: &'b str,
    pub entry: Entry,
}

impl std::fmt::Display for EntryWrapper<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{} {}", self.entry.word, self.dict_name)
    }
}

impl StarDict {
    pub fn dz(dz_path: &Path) -> Result<StarDict> {
        let ifo = Ifo::new(dz_path.with_extension("ifo"))?;
        let idx = Idx::new(dz_path.with_extension("idx"), ifo.version)?;
        let dict = Dict::new(DictType::Dz(dz_path.to_path_buf()));

        Ok(StarDict { ifo, idx, dict })
    }

    pub fn dict(dict_path: &Path) -> Result<StarDict> {
        let ifo = Ifo::new(dict_path.with_extension("ifo"))?;
        let idx = Idx::new(dict_path.with_extension("idx"), ifo.version)?;
        let dict = Dict::new(DictType::Dict(dict_path.to_path_buf()));

        Ok(StarDict { ifo, idx, dict })
    }
}

impl StarDict {
    fn exact_lookup(&self, word: &str) -> Option<&str> {
        let word = word.to_lowercase();
        if let Ok(pos) = self
            .idx
            .items
            .binary_search_by(|probe| probe.0.to_lowercase().cmp(&word))
        {
            let (_word, offset, size) = &self.idx.items[pos];
            self.dict.get(*offset, *size).ok()
        } else {
            None
        }
    }

    fn dict_name(&self) -> &str {
        &self.ifo.bookname
    }
}

/// bookname=      // required
/// wordcount=     // required
/// synwordcount=  // required if ".syn" file exists.
/// idxfilesize=   // required
/// idxoffsetbits= // New in 3.0.0
/// author=
/// email=
/// website=
/// description=   // You can use <br> for new line.
/// date=
/// sametypesequence= // very important.
/// dicttype=

#[derive(Debug)]
pub struct Ifo {
    pub version: Version,
    pub bookname: String,
    pub wordcount: usize,
    pub synwordcount: usize,
    pub idxfilesize: usize,
    pub idxoffsetbits: usize,
    pub author: String,
    pub email: String,
    pub website: String,
    pub description: String,
    pub date: String,
    pub sametypesequence: String,
    pub dicttype: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Version {
    V242,
    V300,
    Unknown,
}

impl Version {
    const V242_STR: &'static str = "2.4.2";
    const V300_STR: &'static str = "3.0.0";
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::V242 => write!(f, "{}", Self::V242_STR),
            Version::V300 => write!(f, "{}", Self::V300_STR),
            Version::Unknown => panic!("Unknown.to_string()"),
        }
    }
}

impl Ifo {
    fn new(path: PathBuf) -> Result<Ifo> {
        let mut ifo = Ifo {
            version: Version::Unknown,
            bookname: String::new(),
            wordcount: 0,
            synwordcount: 0,
            idxfilesize: 0,
            idxoffsetbits: 0,
            author: String::new(),
            email: String::new(),
            website: String::new(),
            description: String::new(),
            date: String::new(),
            sametypesequence: String::new(),
            dicttype: String::new(),
        };

        for line in BufReader::new(
            File::open(&path).with_context(|| format!("Failed to open ifo file {:?}", path))?,
        )
        .lines()
        {
            let line = line?;
            let Some((key, val)) = line.split_once('=') else {
                continue;
            };

            match key {
                "version" => {
                    ifo.version = if val == Version::V242_STR {
                        Version::V242
                    } else if val == Version::V300_STR {
                        Version::V300
                    } else {
                        Version::Unknown
                    }
                }
                "bookname" => ifo.bookname = val.to_owned(),
                "wordcount" => {
                    ifo.wordcount = val
                        .parse()
                        .with_context(|| format!("Failed to parse info file {:?}", path))?
                }
                "synwordcount" => {
                    ifo.synwordcount = val
                        .parse()
                        .with_context(|| format!("Failed to parse info file {:?}", path))?
                }
                "idxfilesize" => {
                    ifo.idxfilesize = val
                        .parse()
                        .with_context(|| format!("Failed to parse info file {:?}", path))?
                }
                "idxoffsetbits" => {
                    ifo.idxoffsetbits = val
                        .parse()
                        .with_context(|| format!("Failed to parse info file {:?}", path))?
                }
                "author" => ifo.author = val.to_owned(),
                "email" => ifo.email = val.to_owned(),
                "website" => ifo.website = val.to_owned(),
                "description" => ifo.description = val.to_owned(),
                "date" => ifo.date = val.to_owned(),
                "sametypesequence" => ifo.sametypesequence = val.to_owned(),
                "dicttype" => ifo.dicttype = val.to_owned(),
                _ => (),
            };
        }
        Ok(ifo)
    }
}

impl Display for Ifo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "StarDict's dict ifo file")?;
        writeln!(f, "version={}", self.version)?;
        writeln!(f, "wordcount={}", self.wordcount)?;
        writeln!(f, "idxfilesize={}", self.idxfilesize)?;
        writeln!(f, "bookname={}", self.bookname)?;
        writeln!(f, "sametypesequence={}", self.sametypesequence)?;
        Ok(())
    }
}

enum DictType {
    Dz(PathBuf),
    Dict(PathBuf),
}

impl DictType {
    fn load_dz(path: &Path) -> Result<String> {
        let s =
            read(path).with_context(|| format!("Failed to open stardict directory {:?}", path))?;
        let mut d = GzDecoder::new(s.as_slice());
        let mut contents = String::new();
        d.read_to_string(&mut contents).with_context(|| {
            format!("Failed to open stardict directory {:?} as dz format", path)
        })?;
        Ok(contents)
    }

    fn load(&self) -> Result<String> {
        match self {
            DictType::Dz(path) => Self::load_dz(path),
            DictType::Dict(path) => Ok(fs::read_to_string(path)?),
        }
    }

    fn path(&self) -> &Path {
        match self {
            DictType::Dz(path_buf) => path_buf,
            DictType::Dict(path_buf) => path_buf,
        }
    }
}

pub struct Dict {
    contents: OnceCell<Result<String>>,
    dict_type: DictType,
}

impl Dict {
    fn new(dict_type: DictType) -> Self {
        Self {
            dict_type,
            contents: OnceCell::new(),
        }
    }

    fn get(&self, offset: usize, size: usize) -> Result<&str> {
        let res = self
            .contents
            .get_or_init(|| self.dict_type.load())
            .as_ref()
            .map(|x| x.get(offset..offset + size));

        match res {
            Ok(Some(res)) => Ok(res),
            Ok(None) => Err(anyhow!("not found")),
            Err(_error) => Err(anyhow!("not found")), // TODO: lost _error here
        }
    }
}

#[derive(Debug)]
pub struct Idx {
    items: Vec<(String, usize, usize)>,
}

impl Idx {
    fn read_bytes<const N: usize, T>(path: PathBuf) -> Result<Self>
    where
        T: FromBytes<N> + TryInto<usize>,
        <T as TryInto<usize>>::Error: Debug + Error + Send + Sync + 'static,
    {
        let f = File::open(&path).with_context(|| format!("Failed to open idx file {:?}", path))?;
        let mut f = BufReader::new(f);

        let mut items: Vec<_> = Vec::new();

        let mut buf: Vec<u8> = Vec::new();
        let mut b = [0; N];
        loop {
            buf.clear();

            let read_bytes = f
                .read_until(0, &mut buf)
                .with_context(|| format!("Failed to parse idx file {:?}", path))?;

            if read_bytes == 0 {
                break;
            }

            if buf.last() == Some(&b'\0') {
                buf.pop();
            }

            let word: String = String::from_utf8_lossy(&buf)
                .chars()
                .filter(|&c| c != '\u{fffd}')
                .collect();

            f.read(&mut b)
                .with_context(|| format!("Failed to parse idx file {:?}", path))?;
            let offset = T::from_be_bytes(b).try_into()?;

            f.read(&mut b)
                .with_context(|| format!("Failed to parse idx file {:?}", path))?;
            let size = T::from_be_bytes(b).try_into()?;

            if !word.is_empty() {
                items.push((word, offset, size))
            }
        }
        Ok(Self { items })
    }

    fn new(path: PathBuf, version: Version) -> Result<Idx> {
        match version {
            Version::V242 => Ok(Idx::read_bytes::<4, u32>(path)?),
            Version::V300 => Ok(Idx::read_bytes::<8, u64>(path)?),
            Version::Unknown => Err(anyhow!("Wrong stardict version in idx file {:?}", path)),
        }
    }
}

impl T for StarDict {
    fn name(&self) -> &str {
        self.dict_name()
    }

    fn path(&self) -> &Path {
        self.dict.dict_type.path()
    }

    fn lookup(&self, word: &str, base_dir: &std::path::Path) -> Result<PathBuf> {
        match self.exact_lookup(word) {
            Some(entry) => {
                let base_dir = create_sub_dir(base_dir, &groom_name(self.dict_name()))?;

                let index_html = base_dir.join("index.html");
                File::create(&index_html)?.write_all(entry.as_bytes())?;
                Ok(base_dir)
            }
            None => Result::Err(anyhow!("not found")),
        }
    }
}
